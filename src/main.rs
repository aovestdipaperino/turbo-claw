use turbo_vision::app::Application;
use turbo_vision::core::command::{CM_QUIT, CommandId};
use turbo_vision::core::event::{KB_CTRL_N, KB_F10};
use turbo_vision::core::geometry::Rect;
use turbo_vision::core::menu_data::{Menu, MenuItem};
use turbo_vision::views::menu_bar::{MenuBar, SubMenu};
use turbo_vision::views::status_line::{StatusItem, StatusLine};

/// Custom command IDs for Turbo Claw
const CM_NEW_FLOW: CommandId = 200;

fn main() -> turbo_vision::core::error::Result<()> {
    let mut app = Application::new()?;

    // Menu bar
    let (width, _height) = app.terminal.size();
    let mut menu_bar = MenuBar::new(Rect::new(0, 0, width, 1));
    let flow_menu = Menu::from_items(vec![
        MenuItem::with_shortcut("~N~ew Flow", CM_NEW_FLOW, KB_CTRL_N, "Ctrl+N", 0),
        MenuItem::separator(),
        MenuItem::with_shortcut("E~x~it", CM_QUIT, KB_F10, "F10", 0),
    ]);
    menu_bar.add_submenu(SubMenu::new("~F~low", flow_menu));
    app.set_menu_bar(menu_bar);

    // Status line
    let (_width, height) = app.terminal.size();
    let status_line = StatusLine::new(
        Rect::new(0, height - 1, width, height),
        vec![
            StatusItem::new("~Ctrl+N~ New Flow", KB_CTRL_N, CM_NEW_FLOW),
            StatusItem::new("~F10~ Quit", KB_F10, CM_QUIT),
        ],
    );
    app.set_status_line(status_line);

    app.run();

    Ok(())
}
