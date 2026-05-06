use crate::claude::protocol::UiEvent;
use crate::claude::session::Session;
use crate::ui::output_view::OutputView;
use std::collections::HashSet;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FlowState {
    Idle,
    Running,
    Done,
}

pub struct Flow {
    pub output_view: OutputView,
    pub session_id: Option<String>,
    pub state: FlowState,
    pub allowed_tools: HashSet<String>,
    pub session: Option<Session>,
    pub total_cost: f64,
    pub total_tokens: u64,
}

impl Flow {
    pub fn new(output_view: OutputView) -> Self {
        Self {
            output_view,
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
        let mut completed = false;
        while let Ok(event) = session.receiver.try_recv() {
            match &event {
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
            self.output_view.handle_ui_event(&event);
        }
        if !completed && let Some(code) = session.try_wait() {
            self.output_view
                .handle_ui_event(&UiEvent::ProcessExited(code));
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
}
