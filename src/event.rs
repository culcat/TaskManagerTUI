use crate::app::{ActiveTab, App, HitTarget, ModalState};
use crate::system::{ActionSignal, SortColumn};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use std::time::{Duration, Instant};

pub fn handle_key_event(app: &mut App, key: KeyEvent) {
    // Ctrl+C always exits cleanly
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
        app.running = false;
        return;
    }

    // 1. Handle active modal first
    match &mut app.modal {
        ModalState::SignalConfirm {
            selected_signal,
            include_children,
            ..
        } => {
            match key.code {
                KeyCode::Esc | KeyCode::Char('q') => {
                    app.modal = ModalState::None;
                }
                KeyCode::Enter => {
                    app.confirm_modal_action();
                }
                KeyCode::Char(' ') => {
                    *include_children = !*include_children;
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    let all = ActionSignal::all();
                    if let Some(pos) = all.iter().position(|s| s == selected_signal) {
                        let prev = if pos == 0 { all.len() - 1 } else { pos - 1 };
                        *selected_signal = all[prev];
                    }
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    let all = ActionSignal::all();
                    if let Some(pos) = all.iter().position(|s| s == selected_signal) {
                        let next = (pos + 1) % all.len();
                        *selected_signal = all[next];
                    }
                }
                _ => {}
            }
            return;
        }
        ModalState::ProcessDetails { pid } => {
            let pid_val = *pid;
            match key.code {
                KeyCode::Esc | KeyCode::Char('q') | KeyCode::Enter | KeyCode::Char('i') => {
                    app.modal = ModalState::None;
                }
                KeyCode::Char('x') | KeyCode::Delete => {
                    let res =
                        app.monitor
                            .send_signal_to_pids(&[pid_val], ActionSignal::Terminate, false);
                    match res {
                        Ok(msg) => app.set_toast(msg, false),
                        Err(err) => app.set_toast(err, true),
                    }
                    app.modal = ModalState::None;
                    app.force_refresh();
                }
                KeyCode::Char('k') | KeyCode::F(9) => {
                    let res =
                        app.monitor
                            .send_signal_to_pids(&[pid_val], ActionSignal::Kill, false);
                    match res {
                        Ok(msg) => app.set_toast(msg, false),
                        Err(err) => app.set_toast(err, true),
                    }
                    app.modal = ModalState::None;
                    app.force_refresh();
                }
                _ => {}
            }
            return;
        }
        ModalState::Help => {
            if matches!(
                key.code,
                KeyCode::Esc | KeyCode::Char('q') | KeyCode::Enter | KeyCode::Char('?') | KeyCode::F(1)
            ) {
                app.modal = ModalState::None;
            }
            return;
        }
        ModalState::None => {}
    }

    // 2. Handle search input mode when focused
    if app.search_focused {
        match key.code {
            KeyCode::Esc | KeyCode::Enter => {
                app.search_focused = false;
            }
            KeyCode::Backspace => {
                app.search_query.pop();
                app.rebuild_process_list();
            }
            KeyCode::Up => {
                app.select_prev(1);
            }
            KeyCode::Down => {
                app.select_next(1);
            }
            KeyCode::Char(c) => {
                app.search_query.push(c);
                app.rebuild_process_list();
            }
            _ => {}
        }
        return;
    }

    // 3. Normal keyboard shortcuts
    match key.code {
        KeyCode::Char('q') | KeyCode::F(10) => {
            app.running = false;
        }
        KeyCode::Esc => {
            if !app.search_query.is_empty() {
                app.search_query.clear();
                app.rebuild_process_list();
            }
        }
        KeyCode::Char('1') => app.active_tab = ActiveTab::Processes,
        KeyCode::Char('2') => app.active_tab = ActiveTab::Performance,
        KeyCode::Char('3') => app.active_tab = ActiveTab::DisksAndNet,
        KeyCode::Tab => {
            let tabs = ActiveTab::all();
            let pos = tabs.iter().position(|t| *t == app.active_tab).unwrap_or(0);
            app.active_tab = tabs[(pos + 1) % tabs.len()];
        }
        KeyCode::BackTab => {
            let tabs = ActiveTab::all();
            let pos = tabs.iter().position(|t| *t == app.active_tab).unwrap_or(0);
            let prev = if pos == 0 { tabs.len() - 1 } else { pos - 1 };
            app.active_tab = tabs[prev];
        }
        KeyCode::Char('/') | KeyCode::Char('f') | KeyCode::F(3) => {
            app.active_tab = ActiveTab::Processes;
            app.search_focused = true;
        }
        KeyCode::Char(' ') | KeyCode::Char('p') => {
            app.paused = !app.paused;
            let state_msg = if app.paused {
                "Обновление приостановлено (Пауза)"
            } else {
                "Обновление возобновлено"
            };
            app.set_toast(state_msg, false);
        }
        KeyCode::Char('r') | KeyCode::F(5) => {
            app.force_refresh();
            app.set_toast("Данные принудительно обновлены", false);
        }
        KeyCode::Char('+') | KeyCode::Char('=') => {
            app.refresh_ms = (app.refresh_ms + 250).min(5000);
        }
        KeyCode::Char('-') | KeyCode::Char('_') => {
            app.refresh_ms = app.refresh_ms.saturating_sub(250).max(250);
        }
        KeyCode::Char('t') => {
            app.tree_mode = !app.tree_mode;
            if app.tree_mode {
                app.group_apps = false;
            }
            app.rebuild_process_list();
        }
        KeyCode::Char('g') => {
            app.group_apps = !app.group_apps;
            if app.group_apps {
                app.tree_mode = false;
            }
            app.rebuild_process_list();
        }
        KeyCode::Char('u') => {
            app.only_my_user = !app.only_my_user;
            app.rebuild_process_list();
        }
        KeyCode::Char('K') => {
            app.hide_kernel_threads = !app.hide_kernel_threads;
            app.rebuild_process_list();
        }
        KeyCode::Up => app.select_prev(1),
        KeyCode::Down => app.select_next(1),
        KeyCode::PageUp => app.select_prev(15),
        KeyCode::PageDown => app.select_next(15),
        KeyCode::Home => app.select_index(0),
        KeyCode::End => {
            if !app.view_processes.is_empty() {
                app.select_index(app.view_processes.len() - 1);
            }
        }
        // Sort shortcuts
        KeyCode::Char('c') | KeyCode::F(6) => app.toggle_sort(SortColumn::Cpu),
        KeyCode::Char('m') => app.toggle_sort(SortColumn::MemBytes),
        KeyCode::Char('n') => app.toggle_sort(SortColumn::Name),
        // Process control / Closing applications
        KeyCode::Char('x') | KeyCode::Delete => {
            app.open_signal_modal(ActionSignal::Terminate);
        }
        KeyCode::Char('k') | KeyCode::F(9) => {
            app.open_signal_modal(ActionSignal::Kill);
        }
        KeyCode::Char('S') => {
            app.open_signal_modal(ActionSignal::Terminate);
        }
        KeyCode::Char('s') => {
            app.toggle_stop_resume_selected();
        }
        KeyCode::Enter | KeyCode::Char('i') => {
            app.open_details_modal();
        }
        KeyCode::Char('?') | KeyCode::F(1) | KeyCode::Char('h') => {
            app.modal = ModalState::Help;
        }
        _ => {}
    }
}

pub fn handle_mouse_event(app: &mut App, mouse: MouseEvent) {
    let col = mouse.column;
    let row = mouse.row;
    app.update_hover(col, row);

    match mouse.kind {
        MouseEventKind::Moved => {
            // Hover state already updated above
        }
        MouseEventKind::ScrollUp => {
            if matches!(app.modal, ModalState::None) {
                app.select_prev(3);
            }
        }
        MouseEventKind::ScrollDown => {
            if matches!(app.modal, ModalState::None) {
                app.select_next(3);
            }
        }
        MouseEventKind::Down(MouseButton::Right) => {
            // Right click on a process row selects it and opens Process Details
            if let Some(HitTarget::ProcessRow(idx)) = app.hit_test(col, row) {
                app.search_focused = false;
                app.select_index(idx);
                app.open_details_modal();
            } else if !matches!(app.modal, ModalState::None) {
                app.modal = ModalState::None;
            }
        }
        MouseEventKind::Down(MouseButton::Left) => {
            let hit = app.hit_test(col, row);

            // Unfocus search box if user clicked anywhere outside SearchBox
            if hit != Some(HitTarget::SearchBox) {
                app.search_focused = false;
            }

            let Some(target) = hit else {
                return;
            };

            // Detect double click on the same target within 400ms
            let now = Instant::now();
            let is_double_click = app
                .last_click
                .map(|(t, prev_target)| {
                    prev_target == target && now.duration_since(t) <= Duration::from_millis(400)
                })
                .unwrap_or(false);
            app.last_click = Some((now, target));

            // If a modal is open, only allow modal targets
            if !matches!(app.modal, ModalState::None) {
                match target {
                    HitTarget::ModalClose => {
                        app.modal = ModalState::None;
                    }
                    HitTarget::ModalConfirmExecute => {
                        app.confirm_modal_action();
                    }
                    HitTarget::ModalToggleChildren => {
                        if let ModalState::SignalConfirm {
                            include_children, ..
                        } = &mut app.modal
                        {
                            *include_children = !*include_children;
                        }
                    }
                    HitTarget::ModalSelectSignal(sig) => {
                        if let ModalState::SignalConfirm {
                            selected_signal, ..
                        } = &mut app.modal
                        {
                            *selected_signal = sig;
                        }
                        if is_double_click {
                            app.confirm_modal_action();
                        }
                    }
                    HitTarget::ModalQuickSignal(sig) => {
                        if let ModalState::ProcessDetails { pid } = app.modal {
                            let res = app.monitor.send_signal_to_pids(&[pid], sig, false);
                            match res {
                                Ok(msg) => app.set_toast(msg, false),
                                Err(err) => app.set_toast(err, true),
                            }
                            app.modal = ModalState::None;
                            app.force_refresh();
                        }
                    }
                    _ => {}
                }
                return;
            }

            // Main UI targets
            match target {
                HitTarget::Tab(tab) => {
                    app.active_tab = tab;
                }
                HitTarget::RefreshFaster => {
                    app.refresh_ms = app.refresh_ms.saturating_sub(250).max(250);
                }
                HitTarget::RefreshSlower => {
                    app.refresh_ms = (app.refresh_ms + 250).min(5000);
                }
                HitTarget::TogglePause => {
                    app.paused = !app.paused;
                    let msg = if app.paused {
                        "Обновление приостановлено"
                    } else {
                        "Обновление возобновлено"
                    };
                    app.set_toast(msg, false);
                }
                HitTarget::ToggleTree => {
                    app.tree_mode = !app.tree_mode;
                    if app.tree_mode {
                        app.group_apps = false;
                    }
                    app.rebuild_process_list();
                }
                HitTarget::ToggleGroupApps => {
                    app.group_apps = !app.group_apps;
                    if app.group_apps {
                        app.tree_mode = false;
                    }
                    app.rebuild_process_list();
                }
                HitTarget::ToggleMyUser => {
                    app.only_my_user = !app.only_my_user;
                    app.rebuild_process_list();
                }
                HitTarget::ToggleHideKernel => {
                    app.hide_kernel_threads = !app.hide_kernel_threads;
                    app.rebuild_process_list();
                }
                HitTarget::SearchBox => {
                    app.search_focused = true;
                }
                HitTarget::ClearSearch => {
                    app.search_query.clear();
                    app.search_focused = false;
                    app.rebuild_process_list();
                }
                HitTarget::SortHeader(col) => {
                    app.toggle_sort(col);
                }
                HitTarget::ProcessRow(idx) => {
                    app.select_index(idx);
                    if is_double_click {
                        app.open_signal_modal(ActionSignal::Terminate);
                    }
                }
                HitTarget::ActionTerminate => {
                    app.open_signal_modal(ActionSignal::Terminate);
                }
                HitTarget::ActionKill => {
                    app.open_signal_modal(ActionSignal::Kill);
                }
                HitTarget::ActionSignalMenu => {
                    app.open_signal_modal(ActionSignal::Terminate);
                }
                HitTarget::ActionStopResume => {
                    app.toggle_stop_resume_selected();
                }
                HitTarget::ActionDetails => {
                    app.open_details_modal();
                }
                HitTarget::ActionHelp => {
                    app.modal = ModalState::Help;
                }
                HitTarget::ActionQuit => {
                    app.running = false;
                }
                _ => {}
            }
        }
        _ => {}
    }
}
