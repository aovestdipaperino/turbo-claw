use turbo_vision::core::command::{CM_CANCEL, CommandId};
use turbo_vision::core::event::Event;
use turbo_vision::core::geometry::Rect;
use turbo_vision::core::state::StateFlags;
use turbo_vision::terminal::Terminal;
use turbo_vision::views::button::Button;
use turbo_vision::views::dialog::Dialog;
use turbo_vision::views::static_text::StaticText;
use turbo_vision::views::view::{View, ViewId};

pub struct ProgressDialog {
    dialog: Dialog,
    status_id: ViewId,
    cost_id: ViewId,
}

impl ProgressDialog {
    pub fn new(screen_width: u16, screen_height: u16) -> Box<Self> {
        let dialog_w: i16 = 40.min(screen_width as i16 - 4);
        let dialog_h: i16 = 9.min(screen_height as i16 - 4);
        let x = ((screen_width as i16) - dialog_w) / 2;
        let y = ((screen_height as i16) - dialog_h) / 2;

        let bounds = Rect::new(x, y, x + dialog_w, y + dialog_h);
        let mut dialog = Dialog::new(bounds, "Running");

        let status = StaticText::new_centered(Rect::new(2, 2, dialog_w - 2, 3), "Starting...");
        let status_id = dialog.add(Box::new(status));

        let cost = StaticText::new_centered(Rect::new(2, 3, dialog_w - 2, 4), "");
        let cost_id = dialog.add(Box::new(cost));

        let btn_x = dialog_w / 2 - 6;
        let btn_y = dialog_h - 3;
        let cancel_btn = Button::new(
            Rect::new(btn_x, btn_y, btn_x + 12, btn_y + 2),
            "Cancel",
            CM_CANCEL,
            true,
        );
        dialog.add(Box::new(cancel_btn));

        dialog.set_initial_focus();

        use turbo_vision::core::state::SF_MODAL;
        let current_state = dialog.state();
        dialog.set_state(current_state | SF_MODAL);

        Box::new(Self {
            dialog,
            status_id,
            cost_id,
        })
    }

    pub fn set_status(&mut self, status: &str) {
        if let Some(view) = self.dialog.child_by_id_mut(self.status_id) {
            unsafe {
                let ptr = view as *mut dyn View as *mut StaticText;
                *ptr = StaticText::new_centered((*ptr).bounds(), status);
            }
        }
    }

    pub fn set_cost(&mut self, cost_usd: f64, tokens: u64) {
        let text = format!("${cost_usd:.4} | {tokens} tokens");
        if let Some(view) = self.dialog.child_by_id_mut(self.cost_id) {
            unsafe {
                let ptr = view as *mut dyn View as *mut StaticText;
                *ptr = StaticText::new_centered((*ptr).bounds(), &text);
            }
        }
    }
}

impl View for ProgressDialog {
    fn bounds(&self) -> Rect {
        self.dialog.bounds()
    }
    fn set_bounds(&mut self, bounds: Rect) {
        self.dialog.set_bounds(bounds);
    }
    fn draw(&mut self, terminal: &mut Terminal) {
        self.dialog.draw(terminal);
    }
    fn handle_event(&mut self, event: &mut Event) {
        self.dialog.handle_event(event);
    }
    fn can_focus(&self) -> bool {
        true
    }
    fn state(&self) -> StateFlags {
        self.dialog.state()
    }
    fn set_state(&mut self, state: StateFlags) {
        self.dialog.set_state(state);
    }
    fn options(&self) -> u16 {
        self.dialog.options()
    }
    fn set_options(&mut self, options: u16) {
        self.dialog.set_options(options);
    }
    fn get_palette(&self) -> Option<turbo_vision::core::palette::Palette> {
        self.dialog.get_palette()
    }
    fn get_end_state(&self) -> CommandId {
        self.dialog.get_end_state()
    }
    fn set_end_state(&mut self, cmd: CommandId) {
        self.dialog.set_end_state(cmd);
    }
}
