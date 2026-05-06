mod bridge;
mod claude;
mod ui;

use std::time::Duration;

use turbo_vision::app::Application;
use turbo_vision::core::command::{CM_OK, CM_QUIT, CommandId};
use turbo_vision::core::event::{EventType, KB_CTRL_N, KB_F10};
use turbo_vision::core::geometry::Rect;
use turbo_vision::core::menu_data::{Menu, MenuItem};
use turbo_vision::views::menu_bar::{MenuBar, SubMenu};
use turbo_vision::views::status_line::{StatusItem, StatusLine};
use crate::ui::flow::{Flow, FlowState};
use crate::ui::output_view::{OutputView, CM_OPEN_PROMPT};
use crate::ui::progress_dialog::ProgressDialog;
use crate::ui::prompt_dialog::PromptDialog;

const CM_NEW_FLOW: CommandId = 200;

fn main() -> turbo_vision::core::error::Result<()> {
    let mut app = Application::new()?;
    let (width, height) = app.terminal.size();

    // Menu bar
    let mut menu_bar = MenuBar::new(Rect::new(0, 0, width, 1));
    let flow_menu = Menu::from_items(vec![
        MenuItem::with_shortcut("~N~ew Flow", CM_NEW_FLOW, KB_CTRL_N, "Ctrl+N", 0),
        MenuItem::separator(),
        MenuItem::with_shortcut("E~x~it", CM_QUIT, KB_F10, "F10", 0),
    ]);
    menu_bar.add_submenu(SubMenu::new("~F~low", flow_menu));
    app.set_menu_bar(menu_bar);

    // Status line
    let status_line = StatusLine::new(
        Rect::new(0, height - 1, width, height),
        vec![
            StatusItem::new("~Ctrl+N~ New Flow", KB_CTRL_N, CM_NEW_FLOW),
            StatusItem::new("~F10~ Quit", KB_F10, CM_QUIT),
        ],
    );
    app.set_status_line(status_line);

    // Active flow (at most one for now)
    let mut active_flow: Option<Box<Flow>> = None;

    // Custom event loop
    app.running = true;
    while app.running {
        // Draw — desktop draws all its children (including OutputView if added)
        app.draw();
        let _ = app.terminal.flush();

        // Poll flow during idle
        if let Some(ref mut flow) = active_flow {
            flow.poll();
        }

        // Get event with 50ms timeout for responsive polling
        match app
            .terminal
            .poll_event(Duration::from_millis(50))
            .ok()
            .flatten()
        {
            Some(mut event) => {
                // 1. Let turbo-vision handle it first (menu bar, desktop, status line)
                //    StatusLine converts KB_CTRL_N → Command(CM_NEW_FLOW)
                //    OutputView converts KB_ENTER → Command(CM_OPEN_PROMPT)
                app.handle_event(&mut event);

                // 2. Check for our custom commands (after conversion)
                if event.what == EventType::Command {
                    match event.command {
                        CM_NEW_FLOW => {
                            if claude::binary::find_claude_binary().is_err() {
                                app.beep();
                                continue;
                            }

                            let desktop_bounds = app.desktop.get_bounds();
                            let output_view = OutputView::new(desktop_bounds, "Flow");
                            let widget = output_view.widget_ref();
                            // Add output view to desktop — it stays visible during modal dialogs
                            app.desktop.add(Box::new(output_view));

                            let mut flow = Box::new(Flow::new(widget));

                            // Open prompt dialog (exec_view — output stays visible behind it)
                            let (sw, sh) = app.terminal.size();
                            let prompt_dialog = PromptDialog::new(sw as u16, sh as u16);
                            let memo_ref = prompt_dialog.memo_ref();
                            let result = app.exec_view(prompt_dialog);

                            if result == CM_OK {
                                let prompt_text = memo_ref.borrow().get_text();
                                if !prompt_text.trim().is_empty()
                                    && flow.start_prompt(&prompt_text).is_ok()
                                {
                                    run_progress_loop(&mut app, &mut flow);
                                }
                            }

                            active_flow = Some(flow);
                        }
                        CM_OPEN_PROMPT => {
                            if let Some(ref mut flow) = active_flow
                                && (flow.state == FlowState::Done
                                    || flow.state == FlowState::Idle)
                            {
                                let (sw, sh) = app.terminal.size();
                                let prompt_dialog = PromptDialog::new(sw as u16, sh as u16);
                                let memo_ref = prompt_dialog.memo_ref();
                                let result = app.exec_view(prompt_dialog);

                                if result == CM_OK {
                                    let prompt_text = memo_ref.borrow().get_text();
                                    if !prompt_text.trim().is_empty()
                                        && flow.start_prompt(&prompt_text).is_ok()
                                    {
                                        run_progress_loop(&mut app, flow);
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            None => {
                // Idle — just poll flow
                if let Some(ref mut flow) = active_flow {
                    flow.poll();
                }
            }
        }
    }

    Ok(())
}

/// Run a progress dialog while Claude is executing.
/// Uses a custom modal loop so the flow can be polled concurrently.
/// The OutputView is already on the desktop (child 0), so it stays visible.
/// The ProgressDialog is added as a later child (drawn on top).
fn run_progress_loop(app: &mut Application, flow: &mut Flow) {
    let (sw, sh) = app.terminal.size();
    let progress = ProgressDialog::new(sw as u16, sh as u16);

    let view_index = app.desktop.child_count();
    app.desktop.add(progress);

    loop {
        // Desktop draws OutputView first, then ProgressDialog on top
        app.draw();
        let _ = app.terminal.flush();

        // Poll flow
        let completed = flow.poll();
        if completed {
            if view_index < app.desktop.child_count() {
                app.desktop.remove_child(view_index);
            }
            return;
        }

        // Check for events
        if let Some(mut event) = app
            .terminal
            .poll_event(Duration::from_millis(50))
            .ok()
            .flatten()
        {
            app.handle_event(&mut event);
            if !app.running {
                flow.cancel();
                if view_index < app.desktop.child_count() {
                    app.desktop.remove_child(view_index);
                }
                return;
            }
            // Check if progress dialog was cancelled (button press → end_state)
            if view_index < app.desktop.child_count() {
                let end_state = app.desktop.child_at(view_index).get_end_state();
                if end_state != 0 {
                    flow.cancel();
                    app.desktop.remove_child(view_index);
                    return;
                }
            }
        }
    }
}
