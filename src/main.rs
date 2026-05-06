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
use turbo_vision::views::view::View;

use crate::ui::flow::{Flow, FlowState};
use crate::ui::output_view::{CM_OPEN_PROMPT, OutputView};
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
        // Draw everything
        app.desktop.draw(&mut app.terminal);

        // Draw the flow's output view on top of the desktop background
        if let Some(ref mut flow) = active_flow {
            flow.output_view.draw(&mut app.terminal);
        }

        // Draw menu bar and status line
        if let Some(ref mut mb) = app.menu_bar {
            mb.draw(&mut app.terminal);
        }
        if let Some(ref mut sl) = app.status_line {
            sl.draw(&mut app.terminal);
        }
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
                // Handle custom commands
                if event.what == EventType::Command {
                    match event.command {
                        CM_NEW_FLOW => {
                            // Check binary exists
                            if claude::binary::find_claude_binary().is_err() {
                                app.beep();
                                event.clear();
                                continue;
                            }

                            let desktop_bounds = app.desktop.get_bounds();
                            let output_view = OutputView::new(desktop_bounds, "Flow");
                            let mut flow = Box::new(Flow::new(output_view));

                            // Open prompt dialog
                            let (sw, sh) = app.terminal.size();
                            let prompt_dialog = PromptDialog::new(sw as u16, sh as u16);
                            let memo_ref = prompt_dialog.memo_ref();
                            let result = app.exec_view(prompt_dialog);

                            if result == CM_OK {
                                let prompt_text = memo_ref.borrow().get_text();
                                if !prompt_text.trim().is_empty()
                                    && let Ok(()) = flow.start_prompt(&prompt_text)
                                {
                                    // Run progress loop
                                    run_progress_loop(&mut app, &mut flow);
                                }
                            }

                            active_flow = Some(flow);
                            event.clear();
                            continue;
                        }
                        CM_OPEN_PROMPT => {
                            if let Some(ref mut flow) = active_flow
                                && (flow.state == FlowState::Done || flow.state == FlowState::Idle)
                            {
                                let (sw, sh) = app.terminal.size();
                                let prompt_dialog = PromptDialog::new(sw as u16, sh as u16);
                                let memo_ref = prompt_dialog.memo_ref();
                                let result = app.exec_view(prompt_dialog);

                                if result == CM_OK {
                                    let prompt_text = memo_ref.borrow().get_text();
                                    if !prompt_text.trim().is_empty()
                                        && let Ok(()) = flow.start_prompt(&prompt_text)
                                    {
                                        run_progress_loop(&mut app, flow);
                                    }
                                }
                            }
                            event.clear();
                            continue;
                        }
                        _ => {}
                    }
                }

                // Let the flow's output view handle keyboard events (scrolling, Enter)
                if let Some(ref mut flow) = active_flow
                    && event.what != EventType::Nothing
                {
                    flow.output_view.handle_event(&mut event);
                }

                // Pass remaining events to app
                if event.what != EventType::Nothing {
                    app.handle_event(&mut event);
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

/// Run the progress dialog loop while Claude is executing.
/// Polls the flow for events, closes when done or cancelled.
fn run_progress_loop(app: &mut Application, flow: &mut Flow) {
    let (sw, sh) = app.terminal.size();
    let progress = ProgressDialog::new(sw as u16, sh as u16);

    // We manually manage the modal loop because we need to poll the flow
    // during the dialog. Using exec_view would block without polling.
    let view_index = app.desktop.child_count();
    app.desktop.add(progress);

    loop {
        // Draw
        app.desktop.draw(&mut app.terminal);
        // Also draw the flow's output view (behind the progress dialog)
        flow.output_view.draw(&mut app.terminal);
        // Redraw menu/status on top
        if let Some(ref mut mb) = app.menu_bar {
            mb.draw(&mut app.terminal);
        }
        if let Some(ref mut sl) = app.status_line {
            sl.draw(&mut app.terminal);
        }
        let _ = app.terminal.flush();

        // Poll flow
        let completed = flow.poll();
        if completed {
            // Remove progress dialog
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
            // Let app handle the event (dispatches to desktop, menu, status)
            app.handle_event(&mut event);
            if !app.running {
                flow.cancel();
                if view_index < app.desktop.child_count() {
                    app.desktop.remove_child(view_index);
                }
                return;
            }
            // Check if progress dialog was cancelled
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
