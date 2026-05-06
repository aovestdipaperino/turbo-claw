use std::cell::RefCell;
use std::rc::Rc;

use turbo_vision::core::command::{CM_CANCEL, CM_OK, CommandId};
use turbo_vision::core::event::Event;
use turbo_vision::core::geometry::Rect;
use turbo_vision::core::state::StateFlags;
use turbo_vision::terminal::Terminal;
use turbo_vision::views::button::Button;
use turbo_vision::views::dialog::Dialog;
use turbo_vision::views::memo::Memo;
use turbo_vision::views::view::View;

struct SharedMemo(Rc<RefCell<Memo>>);

impl View for SharedMemo {
    fn bounds(&self) -> Rect { self.0.borrow().bounds() }
    fn set_bounds(&mut self, bounds: Rect) { self.0.borrow_mut().set_bounds(bounds); }
    fn draw(&mut self, terminal: &mut Terminal) { self.0.borrow_mut().draw(terminal); }
    fn handle_event(&mut self, event: &mut Event) { self.0.borrow_mut().handle_event(event); }
    fn can_focus(&self) -> bool { true }
    fn state(&self) -> StateFlags { self.0.borrow().state() }
    fn set_state(&mut self, state: StateFlags) { self.0.borrow_mut().set_state(state); }
    fn get_palette(&self) -> Option<turbo_vision::core::palette::Palette> { self.0.borrow().get_palette() }
}

pub struct PromptDialog {
    dialog: Dialog,
    memo: Rc<RefCell<Memo>>,
}

impl PromptDialog {
    pub fn new(screen_width: u16, screen_height: u16) -> Box<Self> {
        let dialog_w: i16 = 60.min(screen_width as i16 - 4);
        let dialog_h: i16 = 14.min(screen_height as i16 - 4);
        let x = ((screen_width as i16) - dialog_w) / 2;
        let y = ((screen_height as i16) - dialog_h) / 2;

        let bounds = Rect::new(x, y, x + dialog_w, y + dialog_h);
        let mut dialog = Dialog::new(bounds, "Enter Prompt");

        let memo_bounds = Rect::new(2, 1, dialog_w - 2, dialog_h - 4);
        let memo = Rc::new(RefCell::new(Memo::new(memo_bounds).with_scrollbars(true)));
        dialog.add(Box::new(SharedMemo(Rc::clone(&memo))));

        let btn_y = dialog_h - 3;
        let ok_x = dialog_w / 2 - 12;
        dialog.add(Box::new(Button::new(
            Rect::new(ok_x, btn_y, ok_x + 10, btn_y + 2),
            "~O~K",
            CM_OK,
            true,
        )));
        let cancel_x = ok_x + 12;
        dialog.add(Box::new(Button::new(
            Rect::new(cancel_x, btn_y, cancel_x + 12, btn_y + 2),
            "Cancel",
            CM_CANCEL,
            false,
        )));

        dialog.set_initial_focus();

        use turbo_vision::core::state::SF_MODAL;
        let current_state = dialog.state();
        dialog.set_state(current_state | SF_MODAL);

        Box::new(Self { dialog, memo })
    }

    pub fn get_text(&self) -> String {
        self.memo.borrow().get_text()
    }

    pub fn memo_ref(&self) -> Rc<RefCell<Memo>> {
        Rc::clone(&self.memo)
    }
}

impl View for PromptDialog {
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
