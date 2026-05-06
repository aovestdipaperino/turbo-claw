use std::cell::RefCell;
use std::rc::Rc;

use turbo_vision::core::palette::{Attr, TvColor};
use turbo_vision::views::terminal_widget::TerminalWidget;

use crate::claude::protocol::UiEvent;
use crate::claude::session::Session;
use crate::ui::markdown::render_markdown;
use std::collections::HashSet;

const ATTR_TEXT: Attr = Attr::new(TvColor::White, TvColor::Blue);
const ATTR_THINKING: Attr = Attr::new(TvColor::DarkGray, TvColor::Blue);
const ATTR_TOOL_FRAME: Attr = Attr::new(TvColor::LightCyan, TvColor::Blue);
const ATTR_TOOL_OK: Attr = Attr::new(TvColor::LightGreen, TvColor::Blue);
const ATTR_TOOL_ERR: Attr = Attr::new(TvColor::LightRed, TvColor::Blue);
const ATTR_SEPARATOR: Attr = Attr::new(TvColor::DarkGray, TvColor::Blue);
const ATTR_BOLD: Attr = Attr::new(TvColor::White, TvColor::Blue);
const ATTR_CODE: Attr = Attr::new(TvColor::LightGreen, TvColor::Blue);

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FlowState {
    Idle,
    Running,
    Done,
}

pub struct Flow {
    /// Shared terminal widget — the OutputView on the desktop holds the other Rc
    widget: Rc<RefCell<TerminalWidget>>,
    /// Streaming text accumulator
    text_buffer: String,
    /// Streaming thinking accumulator
    thinking_buffer: String,

    pub session_id: Option<String>,
    pub state: FlowState,
    pub allowed_tools: HashSet<String>,
    pub session: Option<Session>,
    pub total_cost: f64,
    pub total_tokens: u64,
}

impl Flow {
    pub fn new(widget: Rc<RefCell<TerminalWidget>>) -> Self {
        Self {
            widget,
            text_buffer: String::new(),
            thinking_buffer: String::new(),
            session_id: None,
            state: FlowState::Idle,
            allowed_tools: HashSet::new(),
            session: None,
            total_cost: 0.0,
            total_tokens: 0,
        }
    }

    pub fn start_prompt(&mut self, prompt: &str) -> Result<(), String> {
        let session = Session::spawn(prompt, self.session_id.as_deref(), &self.allowed_tools)?;
        self.session = Some(session);
        self.state = FlowState::Running;
        Ok(())
    }

    /// Drain pending events from the session. Returns true if session completed.
    pub fn poll(&mut self) -> bool {
        let session = match self.session.as_mut() {
            Some(s) => s,
            None => return false,
        };

        // Collect events first to avoid double-borrow of self
        let mut events: Vec<UiEvent> = Vec::new();
        while let Ok(event) = session.receiver.try_recv() {
            events.push(event);
        }
        let exited = session.try_wait();

        // Now process collected events
        let mut completed = false;
        for event in &events {
            match event {
                UiEvent::Init { session_id, .. } => {
                    self.session_id = Some(session_id.clone());
                }
                UiEvent::Result {
                    cost_usd,
                    input_tokens,
                    output_tokens,
                    ..
                } => {
                    self.total_cost += cost_usd;
                    self.total_tokens += input_tokens + output_tokens;
                    completed = true;
                }
                UiEvent::ProcessExited(_) => {
                    completed = true;
                }
                _ => {}
            }
            self.handle_ui_event(event);
        }
        if !completed && let Some(code) = exited {
            self.handle_ui_event(&UiEvent::ProcessExited(code));
            completed = true;
        }
        if completed {
            self.state = FlowState::Done;
            self.session = None;
        }
        completed
    }

    pub fn cancel(&mut self) {
        if let Some(ref mut session) = self.session {
            session.kill();
        }
        self.session = None;
        self.state = FlowState::Done;
    }

    /// Process a UiEvent and render it into the shared terminal widget.
    fn handle_ui_event(&mut self, event: &UiEvent) {
        match event {
            UiEvent::Init { .. } => {
                // Title update skipped — OutputView on desktop keeps its initial title.
                // Could be enhanced later with a shared title cell.
            }
            UiEvent::TextDelta { text } => {
                self.flush_thinking();
                self.text_buffer.push_str(text);
                while let Some(pos) = self.text_buffer.find('\n') {
                    let line = self.text_buffer[..pos].to_string();
                    self.append_markdown_line(&line);
                    self.text_buffer = self.text_buffer[pos + 1..].to_string();
                }
            }
            UiEvent::ThinkingDelta { text } => {
                self.flush_text();
                self.thinking_buffer.push_str(text);
                while let Some(pos) = self.thinking_buffer.find('\n') {
                    let line = self.thinking_buffer[..pos].to_string();
                    let display = format!("[thinking] {line}");
                    self.widget
                        .borrow_mut()
                        .append_line_colored(display, ATTR_THINKING);
                    self.thinking_buffer = self.thinking_buffer[pos + 1..].to_string();
                }
            }
            UiEvent::ToolStart {
                tool_name, input, ..
            } => {
                self.flush_text();
                self.flush_thinking();
                let header = format!("┌─ {tool_name} ─────────────────────────");
                self.widget
                    .borrow_mut()
                    .append_line_colored(header, ATTR_TOOL_FRAME);
                if let Some(input_str) = input {
                    let display = if input_str.len() > 120 {
                        format!("│ {}...", &input_str[..117])
                    } else {
                        format!("│ {input_str}")
                    };
                    self.widget
                        .borrow_mut()
                        .append_line_colored(display, ATTR_TOOL_FRAME);
                }
            }
            UiEvent::ToolProgress { content, .. } => {
                for line in content.lines() {
                    let display = format!("│ {line}");
                    self.widget
                        .borrow_mut()
                        .append_line_colored(display, ATTR_TOOL_FRAME);
                }
            }
            UiEvent::ToolDone {
                is_error, output, ..
            } => {
                let (attr, marker) = if *is_error {
                    (ATTR_TOOL_ERR, "✗ error")
                } else {
                    (ATTR_TOOL_OK, "✓ done")
                };
                if let Some(out) = output {
                    let first_line = out.lines().next().unwrap_or("");
                    let display = if first_line.len() > 100 {
                        format!("│ {}...", &first_line[..97])
                    } else {
                        format!("│ {first_line}")
                    };
                    self.widget.borrow_mut().append_line_colored(display, attr);
                }
                let footer = format!("└─ {marker} ─────────────────────────");
                self.widget.borrow_mut().append_line_colored(footer, attr);
            }
            UiEvent::Result {
                duration_ms,
                cost_usd,
                input_tokens,
                output_tokens,
                ..
            } => {
                self.flush_text();
                self.flush_thinking();
                let secs = *duration_ms as f64 / 1000.0;
                let total_tokens = input_tokens + output_tokens;
                let sep =
                    format!("── Done ({secs:.1}s, ${cost_usd:.4}, {total_tokens} tokens) ──");
                self.widget
                    .borrow_mut()
                    .append_line_colored(sep, ATTR_SEPARATOR);
            }
            UiEvent::Error { message } => {
                self.widget
                    .borrow_mut()
                    .append_line_colored(format!("ERROR: {message}"), ATTR_TOOL_ERR);
            }
            UiEvent::StderrLine(line) => {
                self.widget
                    .borrow_mut()
                    .append_line_colored(format!("stderr: {line}"), ATTR_THINKING);
            }
            UiEvent::ProcessExited(code) => {
                self.widget.borrow_mut().append_line_colored(
                    format!("Process exited with code {code}"),
                    ATTR_SEPARATOR,
                );
            }
        }
    }

    fn flush_text(&mut self) {
        if !self.text_buffer.is_empty() {
            let remaining = std::mem::take(&mut self.text_buffer);
            self.append_markdown_line(&remaining);
        }
    }

    fn flush_thinking(&mut self) {
        if !self.thinking_buffer.is_empty() {
            let remaining = std::mem::take(&mut self.thinking_buffer);
            let display = format!("[thinking] {remaining}");
            self.widget
                .borrow_mut()
                .append_line_colored(display, ATTR_THINKING);
        }
    }

    fn append_markdown_line(&mut self, line: &str) {
        let rendered = render_markdown(line);
        for rline in rendered {
            let attr = if rline.is_heading {
                ATTR_BOLD
            } else if rline.is_code {
                ATTR_CODE
            } else {
                ATTR_TEXT
            };
            self.widget
                .borrow_mut()
                .append_line_colored(rline.text, attr);
        }
    }
}
