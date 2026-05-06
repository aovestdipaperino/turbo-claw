use turbo_vision::core::command::{CM_CANCEL, CM_OK, CM_YES, CommandId};
use turbo_vision::core::event::Event;
use turbo_vision::core::geometry::Rect;
use turbo_vision::core::state::StateFlags;
use turbo_vision::terminal::Terminal;
use turbo_vision::views::button::Button;
use turbo_vision::views::dialog::Dialog;
use turbo_vision::views::static_text::StaticText;
use turbo_vision::views::view::View;

pub const CM_ALWAYS_ALLOW: CommandId = CM_OK;
pub const CM_APPROVE: CommandId = CM_YES;
pub const CM_DENY: CommandId = CM_CANCEL;

pub struct PermissionDialog {
    dialog: Dialog,
}

impl PermissionDialog {
    pub fn new(screen_width: i16, screen_height: i16, tool_name: &str, input_preview: &str) -> Box<Self> {
        let dialog_w: i16 = 55.min(screen_width - 4);
        let dialog_h: i16 = 12.min(screen_height - 4);
        let x = (screen_width - dialog_w) / 2;
        let y = (screen_height - dialog_h) / 2;

        let title = format!("Permission: {tool_name}");
        let bounds = Rect::new(x, y, x + dialog_w, y + dialog_h);
        let mut dialog = Dialog::new(bounds, &title);

        let label_text = format!("Tool: {tool_name}");
        dialog.add(Box::new(StaticText::new(Rect::new(2, 1, dialog_w - 2, 2), &label_text)));

        let preview = if input_preview.len() > ((dialog_w - 4) as usize * 3) {
            let max = (dialog_w - 4) as usize * 3;
            format!("{}...", &input_preview[..max])
        } else {
            input_preview.to_string()
        };
        dialog.add(Box::new(StaticText::new(Rect::new(2, 3, dialog_w - 2, 6), &preview)));

        let btn_y = dialog_h - 3;
        dialog.add(Box::new(Button::new(Rect::new(2, btn_y, 14, btn_y + 2), "~A~pprove", CM_APPROVE, true)));
        dialog.add(Box::new(Button::new(Rect::new(16, btn_y, 32, btn_y + 2), "A~l~ways Allow", CM_ALWAYS_ALLOW, false)));
        dialog.add(Box::new(Button::new(Rect::new(34, btn_y, 46, btn_y + 2), "~D~eny", CM_DENY, false)));

        dialog.set_initial_focus();

        use turbo_vision::core::state::SF_MODAL;
        let current_state = dialog.state();
        dialog.set_state(current_state | SF_MODAL);

        Box::new(Self { dialog })
    }
}

impl View for PermissionDialog {
    fn bounds(&self) -> Rect { self.dialog.bounds() }
    fn set_bounds(&mut self, bounds: Rect) { self.dialog.set_bounds(bounds); }
    fn draw(&mut self, terminal: &mut Terminal) { self.dialog.draw(terminal); }
    fn handle_event(&mut self, event: &mut Event) { self.dialog.handle_event(event); }
    fn can_focus(&self) -> bool { true }
    fn state(&self) -> StateFlags { self.dialog.state() }
    fn set_state(&mut self, state: StateFlags) { self.dialog.set_state(state); }
    fn options(&self) -> u16 { self.dialog.options() }
    fn set_options(&mut self, options: u16) { self.dialog.set_options(options); }
    fn get_palette(&self) -> Option<turbo_vision::core::palette::Palette> { self.dialog.get_palette() }
    fn get_end_state(&self) -> CommandId { self.dialog.get_end_state() }
    fn set_end_state(&mut self, cmd: CommandId) { self.dialog.set_end_state(cmd); }
}
