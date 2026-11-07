pub mod app;
pub mod event;
pub mod system;
pub mod ui;

use anyhow::Result;
use app::App;
use crossterm::{
    event::{
        self as ct_event, DisableMouseCapture, EnableMouseCapture, Event, KeyEventKind,
    },
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::{
    io,
    time::Duration,
};

fn restore_terminal() {
    let _ = disable_raw_mode();
    let _ = execute!(io::stdout(), LeaveAlternateScreen, DisableMouseCapture);
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!("tui-task-manager v0.1.0 — Полноценный TUI монитор задач на Rust с поддержкой мыши");
        println!("\nИспользование: tui-task-manager [ОПЦИИ]");
        println!("\nОпции:");
        println!("  -h, --help       Показать эту справку");
        println!("  -v, --version    Показать версию программы");
        return Ok(());
    }
    if args.iter().any(|a| a == "--version" || a == "-v") {
        println!("tui-task-manager {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

    // Install panic hook so terminal & mouse mode are always restored cleanly
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        restore_terminal();
        original_hook(panic_info);
    }));

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let res = run_app(&mut terminal);

    restore_terminal();
    terminal.show_cursor()?;

    res
}

fn run_app(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
    let mut app = App::new();

    while app.running {
        terminal.draw(|frame| ui::draw(frame, &mut app))?;

        let tick_rate = Duration::from_millis(app.refresh_ms);
        let timeout = tick_rate.saturating_sub(app.last_tick.elapsed());

        if ct_event::poll(timeout)? {
            match ct_event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => {
                    event::handle_key_event(&mut app, key);
                }
                Event::Mouse(mouse) => {
                    event::handle_mouse_event(&mut app, mouse);
                }
                Event::Resize(_, _) => {}
                _ => {}
            }
        }

        if app.last_tick.elapsed() >= tick_rate {
            app.on_tick();
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{ActiveTab, HitTarget, ModalState};
    use crate::system::{ActionSignal, SortColumn};
    use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
    use ratatui::backend::TestBackend;
    use std::process::Command;

    #[test]
    fn test_ui_render_and_mouse_hitboxes() {
        let backend = TestBackend::new(140, 40);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut app = App::new();

        terminal.draw(|frame| ui::draw(frame, &mut app)).unwrap();

        // Ensure hitboxes are registered for tabs, headers, rows, and action buttons
        assert!(
            !app.hitboxes.is_empty(),
            "Hitboxes should be populated after rendering"
        );
        assert!(app
            .hitboxes
            .iter()
            .any(|h| h.target == HitTarget::Tab(ActiveTab::Performance)));
        assert!(app
            .hitboxes
            .iter()
            .any(|h| h.target == HitTarget::SortHeader(SortColumn::Pid)));
        assert!(app
            .hitboxes
            .iter()
            .any(|h| h.target == HitTarget::ActionTerminate));
        assert!(app
            .hitboxes
            .iter()
            .any(|h| h.target == HitTarget::ActionKill));

        // Simulate clicking Performance tab with mouse
        let perf_hb = app
            .hitboxes
            .iter()
            .find(|h| h.target == HitTarget::Tab(ActiveTab::Performance))
            .unwrap()
            .rect;

        event::handle_mouse_event(
            &mut app,
            MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: perf_hb.x + 1,
                row: perf_hb.y,
                modifiers: KeyModifiers::empty(),
            },
        );
        assert_eq!(app.active_tab, ActiveTab::Performance);

        // Render Performance tab and DisksAndNet tab to verify no panics
        terminal.draw(|frame| ui::draw(frame, &mut app)).unwrap();
        app.active_tab = ActiveTab::DisksAndNet;
        terminal.draw(|frame| ui::draw(frame, &mut app)).unwrap();
    }

    #[test]
    fn test_mouse_column_sort_and_mode_toggles() {
        let backend = TestBackend::new(140, 40);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut app = App::new();

        terminal.draw(|frame| ui::draw(frame, &mut app)).unwrap();

        // Click on MemBytes column header to sort by memory
        let mem_header_rect = app
            .hitboxes
            .iter()
            .find(|h| h.target == HitTarget::SortHeader(SortColumn::MemBytes))
            .unwrap()
            .rect;

        event::handle_mouse_event(
            &mut app,
            MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: mem_header_rect.x + 1,
                row: mem_header_rect.y,
                modifiers: KeyModifiers::empty(),
            },
        );
        assert_eq!(app.sort_col, SortColumn::MemBytes);
        assert!(!app.sort_asc);

        // Click again to flip sort order
        event::handle_mouse_event(
            &mut app,
            MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: mem_header_rect.x + 1,
                row: mem_header_rect.y,
                modifiers: KeyModifiers::empty(),
            },
        );
        assert!(app.sort_asc);

        // Click Group Apps toggle button
        let grp_rect = app
            .hitboxes
            .iter()
            .find(|h| h.target == HitTarget::ToggleGroupApps)
            .unwrap()
            .rect;
        event::handle_mouse_event(
            &mut app,
            MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: grp_rect.x + 1,
                row: grp_rect.y,
                modifiers: KeyModifiers::empty(),
            },
        );
        assert!(app.group_apps);

        // Right-click on the first visible process row to open ProcessDetails modal
        terminal.draw(|frame| ui::draw(frame, &mut app)).unwrap();
        let row_rect = app
            .hitboxes
            .iter()
            .find(|h| matches!(h.target, HitTarget::ProcessRow(_)))
            .unwrap()
            .rect;
        event::handle_mouse_event(
            &mut app,
            MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Right),
                column: row_rect.x + 2,
                row: row_rect.y,
                modifiers: KeyModifiers::empty(),
            },
        );
        assert!(matches!(app.modal, ModalState::ProcessDetails { .. }));
    }

    #[test]
    fn test_closing_spawned_application_via_mouse_modal() {
        // Spawn a dummy sleep process and verify we can close it via the task manager
        let mut child = Command::new("sleep")
            .arg("60")
            .spawn()
            .expect("Failed to spawn sleep process");
        let child_pid = child.id();

        let backend = TestBackend::new(140, 40);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut app = App::new();
        app.hide_kernel_threads = false;
        app.force_refresh();

        // Filter by child PID
        app.search_query = child_pid.to_string();
        app.rebuild_process_list();
        assert!(
            app.view_processes.iter().any(|p| p.pid == child_pid),
            "Spawned process should be visible in task manager"
        );
        let pos = app
            .view_processes
            .iter()
            .position(|p| p.pid == child_pid)
            .unwrap();
        app.select_index(pos);

        terminal.draw(|frame| ui::draw(frame, &mut app)).unwrap();

        // Click the "Убить (F9 / k)" button with the mouse
        let kill_rect = app
            .hitboxes
            .iter()
            .find(|h| h.target == HitTarget::ActionKill)
            .unwrap()
            .rect;

        event::handle_mouse_event(
            &mut app,
            MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: kill_rect.x + 1,
                row: kill_rect.y,
                modifiers: KeyModifiers::empty(),
            },
        );

        assert!(matches!(
            app.modal,
            ModalState::SignalConfirm {
                selected_signal: ActionSignal::Kill,
                ..
            }
        ));

        // Render modal so modal hitboxes are registered
        terminal.draw(|frame| ui::draw(frame, &mut app)).unwrap();

        let confirm_rect = app
            .hitboxes
            .iter()
            .find(|h| h.target == HitTarget::ModalConfirmExecute)
            .unwrap()
            .rect;

        // Click "Выполнить" in the modal with the mouse
        event::handle_mouse_event(
            &mut app,
            MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: confirm_rect.x + 1,
                row: confirm_rect.y,
                modifiers: KeyModifiers::empty(),
            },
        );

        // Wait on child to verify it was terminated
        let status = child.wait().expect("Failed to wait on child");
        assert!(!status.success(), "Child process should have been killed");
    }
}
