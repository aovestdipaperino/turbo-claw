use std::cell::RefCell;
use std::rc::Rc;

use turbo_vision::core::command::CommandId;
use turbo_vision::core::event::{Event, EventType, KB_ENTER};
use turbo_vision::core::geometry::Rect;
use turbo_vision::core::palette::{Attr, TvColor};
use turbo_vision::core::state::StateFlags;
use turbo_vision::terminal::Terminal;
use turbo_vision::views::terminal_widget::TerminalWidget;
use turbo_vision::views::view::View;
use turbo_vision::views::window::Window;

use crate::claude::protocol::UiEvent;

pub const CM_OPEN_PROMPT: CommandId = 201;

struct SharedWidget(Rc<RefCell<TerminalWidget>>);

impl View for SharedWidget {
    fn bounds(&self) -> Rect { self.0.borrow().bounds() }
    fn set_bounds(&mut self, bounds: Rect) { self.0.borrow_mut().set_bounds(bounds); }
    fn draw(&mut self, terminal: &mut Terminal) { self.0.borrow_mut().draw(terminal); }
    fn handle_event(&mut self, event: &mut Event) { self.0.borrow_mut().handle_event(event); }
    fn can_focus(&self) -> bool { true }
    fn state(&self) -> StateFlags { self.0.borrow().state() }
    fn set_state(&mut self, state: StateFlags) { self.0.borrow_mut().set_state(state); }
    fn get_palette(&self) -> Option<turbo_vision::core::palette::Palette> { self.0.borrow().get_palette() }
}

const ATTR_TEXT: Attr = Attr::new(TvColor::White, TvColor::Blue);
const ATTR_THINKING: Attr = Attr::new(TvColor::DarkGray, TvColor::Blue);
const ATTR_TOOL_FRAME: Attr = Attr::new(TvColor::LightCyan, TvColor::Blue);
const ATTR_TOOL_OK: Attr = Attr::new(TvColor::LightGreen, TvColor::Blue);
const ATTR_TOOL_ERR: Attr = Attr::new(TvColor::LightRed, TvColor::Blue);
const ATTR_SEPARATOR: Attr = Attr::new(TvColor::DarkGray, TvColor::Blue);
const ATTR_BOLD: Attr = Attr::new(TvColor::White, TvColor::Blue);
const ATTR_CODE: Attr = Attr::new(TvColor::LightGreen, TvColor::Blue);

pub struct OutputView {
    window: Window,
    widget: Rc<RefCell<TerminalWidget>>,
    text_buffer: String,
    thinking_buffer: String,
    in_thinking: bool,
}

impl OutputView {
    pub fn new(bounds: Rect, title: &str) -> Self {
        let mut window = Window::new(bounds, title);
        let mut interior = bounds;
        interior.grow(-1, -1);
        let widget = Rc::new(RefCell::new(TerminalWidget::new(interior).with_scrollbar()));
        window.add(Box::new(SharedWidget(Rc::clone(&widget))));
        Self { window, widget, text_buffer: String::new(), thinking_buffer: String::new(), in_thinking: false }
    }

    pub fn handle_ui_event(&mut self, event: &UiEvent) {
        match event {
            UiEvent::Init { model, .. } => {
                self.window.set_title(&format!("Flow — {model}"));
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
                self.in_thinking = true;
                self.thinking_buffer.push_str(text);
                while let Some(pos) = self.thinking_buffer.find('\n') {
                    let line = self.thinking_buffer[..pos].to_string();
                    let display = format!("[thinking] {line}");
                    self.widget.borrow_mut().append_line_colored(display, ATTR_THINKING);
                    self.thinking_buffer = self.thinking_buffer[pos + 1..].to_string();
                }
            }
            UiEvent::ToolStart { tool_name, input, .. } => {
                self.flush_text();
                self.flush_thinking();
                let header = format!("┌─ {tool_name} ─────────────────────────");
                self.widget.borrow_mut().append_line_colored(header, ATTR_TOOL_FRAME);
                if let Some(input_str) = input {
                    let display = if input_str.len() > 120 {
                        format!("│ {}...", &input_str[..117])
                    } else {
                        format!("│ {input_str}")
                    };
                    self.widget.borrow_mut().append_line_colored(display, ATTR_TOOL_FRAME);
                }
            }
            UiEvent::ToolProgress { content, .. } => {
                for line in content.lines() {
                    let display = format!("│ {line}");
                    self.widget.borrow_mut().append_line_colored(display, ATTR_TOOL_FRAME);
                }
            }
            UiEvent::ToolDone { is_error, output, .. } => {
                let (attr, marker) = if *is_error { (ATTR_TOOL_ERR, "✗ error") } else { (ATTR_TOOL_OK, "✓ done") };
                if let Some(out) = output {
                    let first_line = out.lines().next().unwrap_or("");
                    let display = if first_line.len() > 100 { format!("│ {}...", &first_line[..97]) } else { format!("│ {first_line}") };
                    self.widget.borrow_mut().append_line_colored(display, attr);
                }
                let footer = format!("└─ {marker} ─────────────────────────");
                self.widget.borrow_mut().append_line_colored(footer, attr);
            }
            UiEvent::Result { duration_ms, cost_usd, input_tokens, output_tokens, .. } => {
                self.flush_text();
                self.flush_thinking();
                let secs = *duration_ms as f64 / 1000.0;
                let total_tokens = input_tokens + output_tokens;
                let sep = format!("── Done ({secs:.1}s, ${cost_usd:.4}, {total_tokens} tokens) ──");
                self.widget.borrow_mut().append_line_colored(sep, ATTR_SEPARATOR);
            }
            UiEvent::Error { message } => {
                self.widget.borrow_mut().append_line_colored(format!("ERROR: {message}"), ATTR_TOOL_ERR);
            }
            UiEvent::StderrLine(line) => {
                self.widget.borrow_mut().append_line_colored(format!("stderr: {line}"), ATTR_THINKING);
            }
            UiEvent::ProcessExited(code) => {
                self.widget.borrow_mut().append_line_colored(format!("Process exited with code {code}"), ATTR_SEPARATOR);
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
            self.widget.borrow_mut().append_line_colored(display, ATTR_THINKING);
        }
        self.in_thinking = false;
    }

    fn append_markdown_line(&mut self, line: &str) {
        if line.starts_with("```") {
            self.widget.borrow_mut().append_line_colored(line.to_string(), ATTR_CODE);
        } else if line.starts_with("# ") || line.starts_with("## ") || line.starts_with("### ") {
            self.widget.borrow_mut().append_line_colored(line.to_string(), ATTR_BOLD);
        } else {
            self.widget.borrow_mut().append_line_colored(line.to_string(), ATTR_TEXT);
        }
    }
}

impl View for OutputView {
    fn bounds(&self) -> Rect { self.window.bounds() }
    fn set_bounds(&mut self, bounds: Rect) { self.window.set_bounds(bounds); }
    fn draw(&mut self, terminal: &mut Terminal) { self.window.draw(terminal); }
    fn handle_event(&mut self, event: &mut Event) {
        if event.what == EventType::Keyboard && event.key_code == KB_ENTER {
            *event = Event::command(CM_OPEN_PROMPT);
            return;
        }
        self.window.handle_event(event);
    }
    fn can_focus(&self) -> bool { true }
    fn state(&self) -> StateFlags { self.window.state() }
    fn set_state(&mut self, state: StateFlags) { self.window.set_state(state); }
    fn options(&self) -> u16 { self.window.options() }
    fn set_options(&mut self, options: u16) { self.window.set_options(options); }
    fn get_palette(&self) -> Option<turbo_vision::core::palette::Palette> { self.window.get_palette() }
    fn get_end_state(&self) -> CommandId { self.window.get_end_state() }
    fn set_end_state(&mut self, cmd: CommandId) { self.window.set_end_state(cmd); }
}
