use std::cell::RefCell;
use std::rc::Rc;

use turbo_vision::core::command::CommandId;
use turbo_vision::core::event::{Event, EventType, KB_ENTER};
use turbo_vision::core::geometry::Rect;
use turbo_vision::core::state::StateFlags;
use turbo_vision::terminal::Terminal;
use turbo_vision::views::terminal_widget::TerminalWidget;
use turbo_vision::views::view::View;
use turbo_vision::views::window::Window;

pub const CM_OPEN_PROMPT: CommandId = 201;

struct SharedWidget(Rc<RefCell<TerminalWidget>>);

impl View for SharedWidget {
    fn bounds(&self) -> Rect {
        self.0.borrow().bounds()
    }
    fn set_bounds(&mut self, bounds: Rect) {
        self.0.borrow_mut().set_bounds(bounds);
    }
    fn draw(&mut self, terminal: &mut Terminal) {
        self.0.borrow_mut().draw(terminal);
    }
    fn handle_event(&mut self, event: &mut Event) {
        self.0.borrow_mut().handle_event(event);
    }
    fn can_focus(&self) -> bool {
        true
    }
    fn state(&self) -> StateFlags {
        self.0.borrow().state()
    }
    fn set_state(&mut self, state: StateFlags) {
        self.0.borrow_mut().set_state(state);
    }
    fn get_palette(&self) -> Option<turbo_vision::core::palette::Palette> {
        self.0.borrow().get_palette()
    }
}

pub struct OutputView {
    window: Window,
    widget: Rc<RefCell<TerminalWidget>>,
}

impl OutputView {
    pub fn new(bounds: Rect, title: &str) -> Self {
        let mut window = Window::new(bounds, title);
        let mut interior = bounds;
        interior.grow(-1, -1);
        let widget = Rc::new(RefCell::new(TerminalWidget::new(interior).with_scrollbar()));
        window.add(Box::new(SharedWidget(Rc::clone(&widget))));
        Self { window, widget }
    }

    /// Get a shared reference to the terminal widget for feeding content.
    pub fn widget_ref(&self) -> Rc<RefCell<TerminalWidget>> {
        Rc::clone(&self.widget)
    }
}

impl View for OutputView {
    fn bounds(&self) -> Rect {
        self.window.bounds()
    }
    fn set_bounds(&mut self, bounds: Rect) {
        self.window.set_bounds(bounds);
    }
    fn draw(&mut self, terminal: &mut Terminal) {
        self.window.draw(terminal);
    }
    fn handle_event(&mut self, event: &mut Event) {
        if event.what == EventType::Keyboard && event.key_code == KB_ENTER {
            *event = Event::command(CM_OPEN_PROMPT);
            return;
        }
        self.window.handle_event(event);
    }
    fn can_focus(&self) -> bool {
        true
    }
    fn state(&self) -> StateFlags {
        self.window.state()
    }
    fn set_state(&mut self, state: StateFlags) {
        self.window.set_state(state);
    }
    fn options(&self) -> u16 {
        self.window.options()
    }
    fn set_options(&mut self, options: u16) {
        self.window.set_options(options);
    }
    fn get_palette(&self) -> Option<turbo_vision::core::palette::Palette> {
        self.window.get_palette()
    }
    fn get_end_state(&self) -> CommandId {
        self.window.get_end_state()
    }
    fn set_end_state(&mut self, cmd: CommandId) {
        self.window.set_end_state(cmd);
    }
}
