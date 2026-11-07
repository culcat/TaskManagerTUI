use crate::app::{ActiveTab, App, HitTarget, ModalState};
use crate::system::{
    format_bytes, format_duration, format_speed, ActionSignal, SortColumn,
};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Margin, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, Borders, Cell, Clear, Gauge, Paragraph, Row, Scrollbar, ScrollbarOrientation,
        ScrollbarState, Sparkline, Table, Wrap,
    },
    Frame,
};

pub fn draw(frame: &mut Frame, app: &mut App) {
    app.clear_hitboxes();
    let area = frame.area();

    // Background fill
    let bg_block = Block::default().style(Style::default().bg(Color::Rgb(16, 18, 24)));
    frame.render_widget(bg_block, area);

    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Top bar: Tabs & global controls
            Constraint::Length(6), // System overview dashboard (CPU, Mem, Swap, Net/Uptime)
            Constraint::Min(10),   // Active tab content
            Constraint::Length(3), // Action toolbar (clickable buttons + toast)
        ])
        .split(area);

    draw_top_bar(frame, app, main_chunks[0]);
    draw_system_summary(frame, app, main_chunks[1]);

    match app.active_tab {
        ActiveTab::Processes => draw_processes_tab(frame, app, main_chunks[2]),
        ActiveTab::Performance => draw_performance_tab(frame, app, main_chunks[2]),
        ActiveTab::DisksAndNet => draw_disks_and_net_tab(frame, app, main_chunks[2]),
    }

    draw_bottom_bar(frame, app, main_chunks[3]);

    // If modal is open, draw it on top and register its hitboxes last so they take priority
    match app.modal.clone() {
        ModalState::None => {}
        ModalState::SignalConfirm {
            pids,
            proc_name,
            selected_signal,
            include_children,
        } => {
            draw_signal_modal(
                frame,
                app,
                area,
                &pids,
                &proc_name,
                selected_signal,
                include_children,
            );
        }
        ModalState::ProcessDetails { pid } => {
            draw_details_modal(frame, app, area, pid);
        }
        ModalState::Help => {
            draw_help_modal(frame, app, area);
        }
    }
}

fn button_style(hovered: bool, active: bool, fg: Color, active_bg: Color) -> Style {
    if hovered {
        Style::default()
            .fg(Color::Black)
            .bg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else if active {
        Style::default()
            .fg(Color::White)
            .bg(active_bg)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(fg).bg(Color::Rgb(30, 34, 46))
    }
}

fn draw_top_bar(frame: &mut Frame, app: &mut App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Rgb(65, 72, 104)))
        .title(Span::styled(
            format!(
                " ⚡ Rust TUI Task Monitor — {} ({}) ",
                app.monitor.hostname, app.monitor.os_name
            ),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(28), // Tab 1
            Constraint::Length(1),
            Constraint::Length(28), // Tab 2
            Constraint::Length(1),
            Constraint::Length(20), // Tab 3
            Constraint::Min(2),     // Spacer
            Constraint::Length(15), // Refresh rate [-] 1000ms [+]
            Constraint::Length(1),
            Constraint::Length(13), // Pause button
            Constraint::Length(1),
            Constraint::Length(12), // Help button
            Constraint::Length(1),
            Constraint::Length(11), // Quit button
        ])
        .split(inner);

    let tabs = [
        (ActiveTab::Processes, cols[0]),
        (ActiveTab::Performance, cols[2]),
        (ActiveTab::DisksAndNet, cols[4]),
    ];

    for (tab, rect) in tabs {
        let target = HitTarget::Tab(tab);
        let is_active = app.active_tab == tab;
        let is_hovered = app.hovered_target == Some(target);
        let style = button_style(is_hovered, is_active, Color::Gray, Color::Rgb(61, 89, 161));
        let p = Paragraph::new(tab.title())
            .alignment(Alignment::Center)
            .style(style);
        frame.render_widget(p, rect);
        app.register_hitbox(rect, target);
    }

    // Refresh rate controls: [-] 1.0s [+]
    let ref_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(5),
            Constraint::Length(3),
        ])
        .split(cols[6]);

    let faster_hov = app.hovered_target == Some(HitTarget::RefreshFaster);
    frame.render_widget(
        Paragraph::new("[-]")
            .alignment(Alignment::Center)
            .style(button_style(faster_hov, false, Color::Yellow, Color::Reset)),
        ref_chunks[0],
    );
    app.register_hitbox(ref_chunks[0], HitTarget::RefreshFaster);

    frame.render_widget(
        Paragraph::new(format!("{}мс", app.refresh_ms))
            .alignment(Alignment::Center)
            .style(Style::default().fg(Color::White)),
        ref_chunks[1],
    );

    let slower_hov = app.hovered_target == Some(HitTarget::RefreshSlower);
    frame.render_widget(
        Paragraph::new("[+]")
            .alignment(Alignment::Center)
            .style(button_style(slower_hov, false, Color::Yellow, Color::Reset)),
        ref_chunks[2],
    );
    app.register_hitbox(ref_chunks[2], HitTarget::RefreshSlower);

    // Pause button
    let pause_target = HitTarget::TogglePause;
    let pause_hov = app.hovered_target == Some(pause_target);
    let pause_label = if app.paused {
        " ▶ Пуск (P) "
    } else {
        " ⏸ Пауза(P) "
    };
    let pause_style = button_style(
        pause_hov,
        app.paused,
        Color::Green,
        Color::Rgb(180, 100, 30),
    );
    frame.render_widget(
        Paragraph::new(pause_label)
            .alignment(Alignment::Center)
            .style(pause_style),
        cols[8],
    );
    app.register_hitbox(cols[8], pause_target);

    // Help button
    let help_target = HitTarget::ActionHelp;
    let help_hov = app.hovered_target == Some(help_target);
    frame.render_widget(
        Paragraph::new(" ? Справка ")
            .alignment(Alignment::Center)
            .style(button_style(help_hov, false, Color::Cyan, Color::Reset)),
        cols[10],
    );
    app.register_hitbox(cols[10], help_target);

    // Quit button
    let quit_target = HitTarget::ActionQuit;
    let quit_hov = app.hovered_target == Some(quit_target);
    frame.render_widget(
        Paragraph::new(" ✖ Выход ")
            .alignment(Alignment::Center)
            .style(button_style(
                quit_hov,
                false,
                Color::LightRed,
                Color::Rgb(140, 40, 40),
            )),
        cols[12],
    );
    app.register_hitbox(cols[12], quit_target);
}

fn cpu_color(pct: f32) -> Color {
    if pct >= 80.0 {
        Color::LightRed
    } else if pct >= 50.0 {
        Color::Yellow
    } else {
        Color::LightGreen
    }
}

fn draw_system_summary(frame: &mut Frame, app: &mut App, area: Rect) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(34), // CPU gauge + core summary
            Constraint::Percentage(34), // RAM + Swap gauges
            Constraint::Percentage(32), // System info: Tasks, LoadAvg, Uptime, Net
        ])
        .split(area);

    // 1. CPU Panel
    let cpu_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Rgb(65, 72, 104)))
        .title(Span::styled(
            format!(" Процессор ({} потоков) ", app.monitor.cores.len()),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ));
    let cpu_inner = cpu_block.inner(cols[0]);
    frame.render_widget(cpu_block, cols[0]);

    let cpu_rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(1),
        ])
        .split(cpu_inner);

    let global_cpu = app.monitor.global_cpu.clamp(0.0, 100.0);
    let cpu_gauge = Gauge::default()
        .gauge_style(
            Style::default()
                .fg(cpu_color(global_cpu))
                .bg(Color::Rgb(35, 39, 52)),
        )
        .percent(global_cpu.round() as u16)
        .label(Span::styled(
            format!("Общая загрузка ЦП: {:.1}%", global_cpu),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ));
    frame.render_widget(cpu_gauge, cpu_rows[0]);

    // Mini per-core usages line
    let mut core_spans = Vec::new();
    for (idx, c) in app.monitor.cores.iter().take(12).enumerate() {
        core_spans.push(Span::styled(
            format!("C{}:{:>3.0}% ", idx, c.usage),
            Style::default().fg(cpu_color(c.usage)),
        ));
    }
    frame.render_widget(Paragraph::new(Line::from(core_spans)), cpu_rows[2]);

    // 2. Memory & Swap Panel
    let mem_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Rgb(65, 72, 104)))
        .title(Span::styled(
            " Память (RAM & Swap) ",
            Style::default()
                .fg(Color::Magenta)
                .add_modifier(Modifier::BOLD),
        ));
    let mem_inner = mem_block.inner(cols[1]);
    frame.render_widget(mem_block, cols[1]);

    let mem_rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(0),
        ])
        .split(mem_inner);

    let mem_pct = if app.monitor.total_mem > 0 {
        ((app.monitor.used_mem as f64 / app.monitor.total_mem as f64) * 100.0).clamp(0.0, 100.0)
    } else {
        0.0
    };
    let mem_gauge = Gauge::default()
        .gauge_style(
            Style::default()
                .fg(cpu_color(mem_pct as f32))
                .bg(Color::Rgb(35, 39, 52)),
        )
        .percent(mem_pct.round() as u16)
        .label(Span::styled(
            format!(
                "RAM: {} / {} ({:.1}%)",
                format_bytes(app.monitor.used_mem),
                format_bytes(app.monitor.total_mem),
                mem_pct
            ),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ));
    frame.render_widget(mem_gauge, mem_rows[0]);

    let swap_pct = if app.monitor.total_swap > 0 {
        ((app.monitor.used_swap as f64 / app.monitor.total_swap as f64) * 100.0).clamp(0.0, 100.0)
    } else {
        0.0
    };
    let swap_gauge = Gauge::default()
        .gauge_style(
            Style::default()
                .fg(Color::Cyan)
                .bg(Color::Rgb(35, 39, 52)),
        )
        .percent(swap_pct.round() as u16)
        .label(Span::styled(
            format!(
                "Swap: {} / {} ({:.1}%)",
                format_bytes(app.monitor.used_swap),
                format_bytes(app.monitor.total_swap),
                swap_pct
            ),
            Style::default().fg(Color::White),
        ));
    frame.render_widget(swap_gauge, mem_rows[2]);

    // 3. System Status Panel
    let stat_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Rgb(65, 72, 104)))
        .title(Span::styled(
            " Сводка Системы и Сеть ",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ));
    let stat_inner = stat_block.inner(cols[2]);
    frame.render_widget(stat_block, cols[2]);

    let (l1, l5, l15) = app.monitor.load_avg;
    let lines = vec![
        Line::from(vec![
            Span::styled("Процессов: ", Style::default().fg(Color::Gray)),
            Span::styled(
                format!("{}", app.monitor.raw_processes.len()),
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" (показано: ", Style::default().fg(Color::Gray)),
            Span::styled(
                format!("{}", app.view_processes.len()),
                Style::default().fg(Color::Cyan),
            ),
            Span::styled(")", Style::default().fg(Color::Gray)),
        ]),
        Line::from(vec![
            Span::styled("Load Avg:  ", Style::default().fg(Color::Gray)),
            Span::styled(
                format!("{:.2}  {:.2}  {:.2}", l1, l5, l15),
                Style::default().fg(Color::Yellow),
            ),
            Span::styled(" | Uptime: ", Style::default().fg(Color::Gray)),
            Span::styled(
                format_duration(app.monitor.uptime),
                Style::default().fg(Color::Green),
            ),
        ]),
        Line::from(vec![
            Span::styled("Сеть ↓RX:  ", Style::default().fg(Color::Gray)),
            Span::styled(
                format!("{:<12}", format_speed(app.monitor.total_rx_speed)),
                Style::default().fg(Color::LightGreen),
            ),
            Span::styled("↑TX: ", Style::default().fg(Color::Gray)),
            Span::styled(
                format_speed(app.monitor.total_tx_speed),
                Style::default().fg(Color::LightCyan),
            ),
        ]),
    ];
    frame.render_widget(Paragraph::new(lines), stat_inner);
}

fn draw_processes_tab(frame: &mut Frame, app: &mut App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Filter & View mode toolbar
            Constraint::Min(5),    // Process Table
        ])
        .split(area);

    draw_process_toolbar(frame, app, chunks[0]);
    draw_process_table(frame, app, chunks[1]);
}

fn draw_process_toolbar(frame: &mut Frame, app: &mut App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Rgb(65, 72, 104)))
        .title(Span::styled(
            " Фильтр и Режимы Отображения (Кликните мышью для переключения) ",
            Style::default().fg(Color::Gray),
        ));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Min(24),    // Search input box
            Constraint::Length(5),  // Clear search [×]
            Constraint::Length(1),  // Spacer
            Constraint::Length(16), // Tree toggle [Дерево (t)]
            Constraint::Length(1),
            Constraint::Length(20), // Group apps [Приложения (g)]
            Constraint::Length(1),
            Constraint::Length(17), // Only my user [Мои: user (u)]
            Constraint::Length(1),
            Constraint::Length(18), // Hide kernel threads
        ])
        .split(inner);

    // Search box
    let search_hov = app.hovered_target == Some(HitTarget::SearchBox);
    let search_style = if app.search_focused {
        Style::default()
            .fg(Color::Yellow)
            .bg(Color::Rgb(40, 46, 66))
            .add_modifier(Modifier::BOLD)
    } else if search_hov {
        Style::default().fg(Color::Cyan).bg(Color::Rgb(35, 40, 56))
    } else {
        Style::default().fg(Color::White).bg(Color::Rgb(26, 29, 40))
    };

    let cursor_char = if app.search_focused { "█" } else { "" };
    let search_text = if app.search_query.is_empty() && !app.search_focused {
        " 🔍 Поиск по имени, команде или PID (нажмите / или кликните)...".to_string()
    } else {
        format!(" 🔍 Фильтр: {}{}", app.search_query, cursor_char)
    };
    frame.render_widget(Paragraph::new(search_text).style(search_style), cols[0]);
    app.register_hitbox(cols[0], HitTarget::SearchBox);

    // Clear search button
    let clear_hov = app.hovered_target == Some(HitTarget::ClearSearch);
    frame.render_widget(
        Paragraph::new(" [×] ")
            .alignment(Alignment::Center)
            .style(button_style(
                clear_hov,
                !app.search_query.is_empty(),
                Color::LightRed,
                Color::Rgb(110, 40, 40),
            )),
        cols[1],
    );
    app.register_hitbox(cols[1], HitTarget::ClearSearch);

    // Tree mode button
    let tree_hov = app.hovered_target == Some(HitTarget::ToggleTree);
    let tree_label = if app.tree_mode {
        " ☑ Дерево (t) "
    } else {
        " ☐ Дерево (t) "
    };
    frame.render_widget(
        Paragraph::new(tree_label)
            .alignment(Alignment::Center)
            .style(button_style(
                tree_hov,
                app.tree_mode,
                Color::Cyan,
                Color::Rgb(40, 100, 120),
            )),
        cols[3],
    );
    app.register_hitbox(cols[3], HitTarget::ToggleTree);

    // Group apps button
    let grp_hov = app.hovered_target == Some(HitTarget::ToggleGroupApps);
    let grp_label = if app.group_apps {
        " ☑ Приложения (g) "
    } else {
        " ☐ Приложения (g) "
    };
    frame.render_widget(
        Paragraph::new(grp_label)
            .alignment(Alignment::Center)
            .style(button_style(
                grp_hov,
                app.group_apps,
                Color::Magenta,
                Color::Rgb(100, 50, 130),
            )),
        cols[5],
    );
    app.register_hitbox(cols[5], HitTarget::ToggleGroupApps);

    // Only my user button
    let usr_hov = app.hovered_target == Some(HitTarget::ToggleMyUser);
    let usr_label = if app.only_my_user {
        " ☑ Только мои(u) "
    } else {
        " ☐ Только мои(u) "
    };
    frame.render_widget(
        Paragraph::new(usr_label)
            .alignment(Alignment::Center)
            .style(button_style(
                usr_hov,
                app.only_my_user,
                Color::Green,
                Color::Rgb(40, 110, 60),
            )),
        cols[7],
    );
    app.register_hitbox(cols[7], HitTarget::ToggleMyUser);

    // Hide kernel threads button
    let krn_hov = app.hovered_target == Some(HitTarget::ToggleHideKernel);
    let krn_label = if app.hide_kernel_threads {
        " ☑ Без ядра (K) "
    } else {
        " ☐ Без ядра (K) "
    };
    frame.render_widget(
        Paragraph::new(krn_label)
            .alignment(Alignment::Center)
            .style(button_style(
                krn_hov,
                app.hide_kernel_threads,
                Color::Yellow,
                Color::Rgb(95, 85, 30),
            )),
        cols[9],
    );
    app.register_hitbox(cols[9], HitTarget::ToggleHideKernel);
}

fn draw_process_table(frame: &mut Frame, app: &mut App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Rgb(65, 72, 104)))
        .title(Span::styled(
            " Список Процессов (Клик по заголовку — сортировка | Клик по строке — выбор | Двойной клик — управление) ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let columns = [
        (SortColumn::Pid, Constraint::Length(8)),
        (SortColumn::Name, Constraint::Length(26)),
        (SortColumn::User, Constraint::Length(12)),
        (SortColumn::Status, Constraint::Length(9)),
        (SortColumn::Cpu, Constraint::Length(9)),
        (SortColumn::MemPercent, Constraint::Length(8)),
        (SortColumn::MemBytes, Constraint::Length(11)),
        (SortColumn::DiskRead, Constraint::Length(11)),
        (SortColumn::DiskWrite, Constraint::Length(11)),
        (SortColumn::Command, Constraint::Min(16)),
    ];

    let constraints: Vec<Constraint> = columns.iter().map(|(_, c)| *c).collect();

    // Calculate column rects on the header line (inner.y) so mouse clicks sort columns
    let header_rect = Rect {
        x: inner.x,
        y: inner.y,
        width: inner.width,
        height: 1,
    };
    let col_rects = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(constraints.clone())
        .spacing(1)
        .split(header_rect);

    let mut header_cells = Vec::with_capacity(columns.len());
    for (idx, (col, _)) in columns.iter().enumerate() {
        let target = HitTarget::SortHeader(*col);
        app.register_hitbox(col_rects[idx], target);

        let is_sorted = app.sort_col == *col;
        let is_hovered = app.hovered_target == Some(target);
        let arrow = if is_sorted {
            if app.sort_asc {
                " ▲"
            } else {
                " ▼"
            }
        } else {
            ""
        };
        let style = if is_hovered {
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else if is_sorted {
            Style::default()
                .fg(Color::Yellow)
                .bg(Color::Rgb(40, 48, 72))
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
                .fg(Color::Cyan)
                .bg(Color::Rgb(26, 30, 44))
                .add_modifier(Modifier::BOLD)
        };
        header_cells.push(Cell::from(format!("{}{}", col.title(), arrow)).style(style));
    }

    let header = Row::new(header_cells).height(1);

    // Table body area (below header)
    let body_rect = Rect {
        x: inner.x,
        y: inner.y.saturating_add(1),
        width: inner.width,
        height: inner.height.saturating_sub(1),
    };
    app.register_hitbox(body_rect, HitTarget::ProcessTableBody);

    let rows: Vec<Row> = app
        .view_processes
        .iter()
        .enumerate()
        .map(|(idx, p)| {
            let is_hovered = app.hovered_target == Some(HitTarget::ProcessRow(idx));
            let base_bg = if is_hovered {
                Color::Rgb(42, 50, 74)
            } else if idx % 2 == 0 {
                Color::Rgb(18, 20, 28)
            } else {
                Color::Rgb(23, 26, 36)
            };

            let cpu_fg = cpu_color(p.cpu_usage);
            let mem_fg = cpu_color(p.mem_percent);

            Row::new(vec![
                Cell::from(format!("{}", p.pid)).style(Style::default().fg(Color::Yellow)),
                Cell::from(p.display_name.clone()).style(
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ),
                Cell::from(p.user.clone()).style(Style::default().fg(Color::Gray)),
                Cell::from(p.status.clone()).style(Style::default().fg(Color::Cyan)),
                Cell::from(format!("{:>6.1}%", p.cpu_usage)).style(
                    Style::default()
                        .fg(cpu_fg)
                        .add_modifier(if p.cpu_usage >= 25.0 {
                            Modifier::BOLD
                        } else {
                            Modifier::empty()
                        }),
                ),
                Cell::from(format!("{:>5.1}%", p.mem_percent)).style(Style::default().fg(mem_fg)),
                Cell::from(format_bytes(p.mem_bytes)).style(Style::default().fg(Color::Magenta)),
                Cell::from(format_speed(p.disk_read_per_sec))
                    .style(Style::default().fg(Color::Green)),
                Cell::from(format_speed(p.disk_write_per_sec))
                    .style(Style::default().fg(Color::LightBlue)),
                Cell::from(p.cmd.clone()).style(Style::default().fg(Color::DarkGray)),
            ])
            .style(Style::default().bg(base_bg))
            .height(1)
        })
        .collect();

    let table = Table::new(rows, constraints)
        .header(header)
        .column_spacing(1)
        .row_highlight_style(
            Style::default()
                .bg(Color::Rgb(61, 89, 161))
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        );

    frame.render_stateful_widget(table, inner, &mut app.table_state);

    // Register hitbox for each visible row using updated table_state.offset()
    let offset = app.table_state.offset();
    let max_rows = body_rect.height as usize;
    for r in 0..max_rows {
        let proc_idx = offset + r;
        if proc_idx < app.view_processes.len() {
            let row_rect = Rect {
                x: body_rect.x,
                y: body_rect.y + r as u16,
                width: body_rect.width,
                height: 1,
            };
            app.register_hitbox(row_rect, HitTarget::ProcessRow(proc_idx));
        }
    }

    // Render scrollbar
    if app.view_processes.len() > max_rows {
        let mut sb_state = ScrollbarState::new(app.view_processes.len())
            .position(app.table_state.selected().unwrap_or(0));
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(Some("▲"))
            .end_symbol(Some("▼"));
        frame.render_stateful_widget(
            scrollbar,
            area.inner(Margin {
                vertical: 1,
                horizontal: 0,
            }),
            &mut sb_state,
        );
    }
}

fn draw_performance_tab(frame: &mut Frame, app: &mut App, area: Rect) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(45), // Historical Sparklines (CPU & Memory)
            Constraint::Percentage(55), // Per-core CPU usage bars
        ])
        .split(area);

    let top_cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(rows[0]);

    // CPU History Sparkline
    let cpu_data: Vec<u64> = app.monitor.cpu_history.iter().copied().collect();
    let cpu_spark = Sparkline::default()
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Rgb(65, 72, 104)))
                .title(Span::styled(
                    format!(
                        " История загрузки ЦП (Текущая: {:.1}%) ",
                        app.monitor.global_cpu
                    ),
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                )),
        )
        .data(&cpu_data)
        .max(100)
        .style(Style::default().fg(Color::LightGreen));
    frame.render_widget(cpu_spark, top_cols[0]);

    // Memory History Sparkline
    let mem_data: Vec<u64> = app.monitor.mem_history.iter().copied().collect();
    let mem_spark = Sparkline::default()
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Rgb(65, 72, 104)))
                .title(Span::styled(
                    format!(
                        " История использования Памяти (Доступно: {}) ",
                        format_bytes(app.monitor.available_mem)
                    ),
                    Style::default()
                        .fg(Color::Magenta)
                        .add_modifier(Modifier::BOLD),
                )),
        )
        .data(&mem_data)
        .max(100)
        .style(Style::default().fg(Color::LightMagenta));
    frame.render_widget(mem_spark, top_cols[1]);

    // Per-core CPU Grid
    let cores_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Rgb(65, 72, 104)))
        .title(Span::styled(
            " Загрузка Ядер / Потоков Процессора ",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ));
    let cores_inner = cores_block.inner(rows[1]);
    frame.render_widget(cores_block, rows[1]);

    let core_cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(cores_inner);

    let mid = (app.monitor.cores.len() + 1) / 2;
    for (col_idx, slice) in [
        &app.monitor.cores[..mid.min(app.monitor.cores.len())],
        &app.monitor.cores[mid.min(app.monitor.cores.len())..],
    ]
    .into_iter()
    .enumerate()
    {
        let mut lines = Vec::new();
        for core in slice {
            let bar_width = 22usize;
            let filled = ((core.usage / 100.0) * bar_width as f32).round() as usize;
            let filled = filled.min(bar_width);
            let bar_str = format!(
                "[{}{}]",
                "█".repeat(filled),
                "░".repeat(bar_width.saturating_sub(filled))
            );
            lines.push(Line::from(vec![
                Span::styled(
                    format!(" {:<7} ", core.name),
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(bar_str, Style::default().fg(cpu_color(core.usage))),
                Span::styled(
                    format!(" {:>5.1}% ", core.usage),
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("({} МГц)", core.frequency),
                    Style::default().fg(Color::Gray),
                ),
            ]));
        }
        frame.render_widget(Paragraph::new(lines), core_cols[col_idx]);
    }
}

fn draw_disks_and_net_tab(frame: &mut Frame, app: &mut App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(52), Constraint::Percentage(48)])
        .split(area);

    // Disks Table
    let disk_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Rgb(65, 72, 104)))
        .title(Span::styled(
            " Дисковые Разделы и Накопители ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ));
    let disk_inner = disk_block.inner(chunks[0]);
    frame.render_widget(disk_block, chunks[0]);

    let disk_header = Row::new(vec![
        "Точка монтирования",
        "Устройство",
        "ФС",
        "Использовано",
        "Свободно",
        "Всего",
        "Заполненность",
    ])
    .style(
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
    )
    .height(1);

    let disk_rows: Vec<Row> = app
        .monitor
        .disk_entries
        .iter()
        .map(|d| {
            let bar_w = 16usize;
            let fill = ((d.usage_percent / 100.0) * bar_w as f32).round() as usize;
            let fill = fill.min(bar_w);
            let bar = format!(
                "[{}{}] {:>5.1}%",
                "■".repeat(fill),
                "·".repeat(bar_w.saturating_sub(fill)),
                d.usage_percent
            );
            Row::new(vec![
                Cell::from(d.mount_point.clone()).style(
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ),
                Cell::from(d.name.clone()).style(Style::default().fg(Color::Gray)),
                Cell::from(d.file_system.clone()).style(Style::default().fg(Color::Cyan)),
                Cell::from(format_bytes(d.used_space)).style(Style::default().fg(Color::Magenta)),
                Cell::from(format_bytes(d.available_space))
                    .style(Style::default().fg(Color::Green)),
                Cell::from(format_bytes(d.total_space)).style(Style::default().fg(Color::White)),
                Cell::from(bar).style(Style::default().fg(cpu_color(d.usage_percent))),
            ])
        })
        .collect();

    let disk_table = Table::new(
        disk_rows,
        [
            Constraint::Length(22),
            Constraint::Length(18),
            Constraint::Length(10),
            Constraint::Length(14),
            Constraint::Length(14),
            Constraint::Length(14),
            Constraint::Min(24),
        ],
    )
    .header(disk_header)
    .column_spacing(1);
    frame.render_widget(disk_table, disk_inner);

    // Network Interfaces Table
    let net_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Rgb(65, 72, 104)))
        .title(Span::styled(
            " Сетевые Интерфейсы (В реальном времени) ",
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ));
    let net_inner = net_block.inner(chunks[1]);
    frame.render_widget(net_block, chunks[1]);

    let net_header = Row::new(vec![
        "Интерфейс",
        "Приём (↓ RX/с)",
        "Отправка (↑ TX/с)",
        "Всего принято",
        "Всего отправлено",
    ])
    .style(
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
    )
    .height(1);

    let net_rows: Vec<Row> = app
        .monitor
        .net_entries
        .iter()
        .map(|n| {
            Row::new(vec![
                Cell::from(n.interface.clone()).style(
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ),
                Cell::from(format_speed(n.rx_bytes_per_sec))
                    .style(Style::default().fg(Color::LightGreen)),
                Cell::from(format_speed(n.tx_bytes_per_sec))
                    .style(Style::default().fg(Color::LightCyan)),
                Cell::from(format_bytes(n.total_rx)).style(Style::default().fg(Color::Gray)),
                Cell::from(format_bytes(n.total_tx)).style(Style::default().fg(Color::Gray)),
            ])
        })
        .collect();

    let net_table = Table::new(
        net_rows,
        [
            Constraint::Length(20),
            Constraint::Length(20),
            Constraint::Length(20),
            Constraint::Length(20),
            Constraint::Min(20),
        ],
    )
    .header(net_header)
    .column_spacing(1);
    frame.render_widget(net_table, net_inner);
}

fn draw_bottom_bar(frame: &mut Frame, app: &mut App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Rgb(65, 72, 104)));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(22), // Terminate (SIGTERM)
            Constraint::Length(1),
            Constraint::Length(19), // Force Kill (SIGKILL)
            Constraint::Length(1),
            Constraint::Length(16), // Signal menu
            Constraint::Length(1),
            Constraint::Length(18), // Stop/Resume
            Constraint::Length(1),
            Constraint::Length(19), // Details
            Constraint::Length(2),
            Constraint::Min(10),    // Status / Toast notification
        ])
        .split(inner);

    let actions = [
        (
            HitTarget::ActionTerminate,
            " ⏹ Завершить (Del/x) ",
            Color::Yellow,
            Color::Rgb(130, 90, 20),
            cols[0],
        ),
        (
            HitTarget::ActionKill,
            " 💀 Убить (F9 / k) ",
            Color::LightRed,
            Color::Rgb(150, 35, 35),
            cols[2],
        ),
        (
            HitTarget::ActionSignalMenu,
            " ⚡ Сигнал (S) ",
            Color::Magenta,
            Color::Rgb(100, 40, 120),
            cols[4],
        ),
        (
            HitTarget::ActionStopResume,
            " ⏯ Стоп/Пуск (s) ",
            Color::Cyan,
            Color::Rgb(30, 90, 110),
            cols[6],
        ),
        (
            HitTarget::ActionDetails,
            " ℹ Инфо (Enter/i) ",
            Color::Green,
            Color::Rgb(35, 100, 60),
            cols[8],
        ),
    ];

    for (target, label, fg, active_bg, rect) in actions {
        let hov = app.hovered_target == Some(target);
        let style = button_style(hov, true, fg, active_bg);
        frame.render_widget(
            Paragraph::new(label)
                .alignment(Alignment::Center)
                .style(style),
            rect,
        );
        app.register_hitbox(rect, target);
    }

    // Status or Toast message on the right
    if let Some(toast) = &app.toast {
        let color = if toast.is_error {
            Color::LightRed
        } else {
            Color::LightGreen
        };
        let prefix = if toast.is_error { "⚠ " } else { "✔ " };
        frame.render_widget(
            Paragraph::new(format!("{}{}", prefix, toast.text))
                .style(Style::default().fg(color).add_modifier(Modifier::BOLD)),
            cols[10],
        );
    } else if let Some(sel) = app.selected_process() {
        frame.render_widget(
            Paragraph::new(format!(
                "Выбран: {} (PID: {}) — CPU {:.1}%, RAM {}",
                sel.name,
                sel.pid,
                sel.cpu_usage,
                format_bytes(sel.mem_bytes)
            ))
            .style(Style::default().fg(Color::Gray)),
            cols[10],
        );
    }
}

fn centered_rect(width: u16, height: u16, r: Rect) -> Rect {
    let w = width.min(r.width.saturating_sub(2));
    let h = height.min(r.height.saturating_sub(2));
    Rect {
        x: r.x + (r.width.saturating_sub(w)) / 2,
        y: r.y + (r.height.saturating_sub(h)) / 2,
        width: w,
        height: h,
    }
}

fn draw_signal_modal(
    frame: &mut Frame,
    app: &mut App,
    area: Rect,
    pids: &[u32],
    proc_name: &str,
    selected_signal: ActionSignal,
    include_children: bool,
) {
    let modal_rect = centered_rect(64, 18, area);
    frame.render_widget(Clear, modal_rect);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(
            Style::default()
                .fg(Color::LightRed)
                .add_modifier(Modifier::BOLD),
        )
        .style(Style::default().bg(Color::Rgb(22, 24, 34)))
        .title(Span::styled(
            " ⚡ Управление Приложением / Отправка Сигнала ",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(modal_rect);
    frame.render_widget(block, modal_rect);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2), // Target description
            Constraint::Length(1), // Spacer
            Constraint::Length(6), // Signal list (6 options)
            Constraint::Length(1), // Spacer
            Constraint::Length(1), // Toggle children checkbox
            Constraint::Length(2), // Spacer
            Constraint::Length(1), // Execute / Cancel buttons
        ])
        .split(inner);

    let pid_desc = if pids.len() == 1 {
        format!("Процесс: {} (PID: {})", proc_name, pids[0])
    } else {
        format!(
            "Группа приложения: {} ({} процессов)",
            proc_name,
            pids.len()
        )
    };
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled(
                pid_desc,
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(Span::styled(
                "Выберите действие мышью или клавишами ↑/↓ и нажмите Enter:",
                Style::default().fg(Color::Gray),
            )),
        ]),
        rows[0],
    );

    // 6 Signal choices
    for (i, &sig) in ActionSignal::all().iter().enumerate() {
        let sig_rect = Rect {
            x: rows[2].x + 1,
            y: rows[2].y + i as u16,
            width: rows[2].width.saturating_sub(2),
            height: 1,
        };
        let target = HitTarget::ModalSelectSignal(sig);
        let is_sel = selected_signal == sig;
        let is_hov = app.hovered_target == Some(target);
        let radio = if is_sel { "(●)" } else { "( )" };
        let style = if is_hov {
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else if is_sel {
            Style::default()
                .fg(Color::Yellow)
                .bg(Color::Rgb(50, 60, 95))
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White)
        };
        frame.render_widget(
            Paragraph::new(format!(" {} {} ", radio, sig.label())).style(style),
            sig_rect,
        );
        app.register_hitbox(sig_rect, target);
    }

    // Include children toggle
    let child_target = HitTarget::ModalToggleChildren;
    let child_hov = app.hovered_target == Some(child_target);
    let check = if include_children { "[☑]" } else { "[☐]" };
    let child_style = button_style(
        child_hov,
        include_children,
        Color::Cyan,
        Color::Rgb(40, 80, 100),
    );
    frame.render_widget(
        Paragraph::new(format!(
            " {} Завершить вместе со всеми дочерними процессами (Пробел) ",
            check
        ))
        .style(child_style),
        rows[4],
    );
    app.register_hitbox(rows[4], child_target);

    // Buttons row
    let btn_cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(10),
            Constraint::Percentage(35),
            Constraint::Percentage(10),
            Constraint::Percentage(35),
            Constraint::Percentage(10),
        ])
        .split(rows[6]);

    let exec_target = HitTarget::ModalConfirmExecute;
    let exec_hov = app.hovered_target == Some(exec_target);
    frame.render_widget(
        Paragraph::new(" ✔ Выполнить (Enter) ")
            .alignment(Alignment::Center)
            .style(button_style(
                exec_hov,
                true,
                Color::White,
                Color::Rgb(160, 40, 40),
            )),
        btn_cols[1],
    );
    app.register_hitbox(btn_cols[1], exec_target);

    let cancel_target = HitTarget::ModalClose;
    let cancel_hov = app.hovered_target == Some(cancel_target);
    frame.render_widget(
        Paragraph::new(" ✖ Отмена (Esc) ")
            .alignment(Alignment::Center)
            .style(button_style(
                cancel_hov,
                true,
                Color::White,
                Color::Rgb(60, 68, 90),
            )),
        btn_cols[3],
    );
    app.register_hitbox(btn_cols[3], cancel_target);
}

fn draw_details_modal(frame: &mut Frame, app: &mut App, area: Rect, pid: u32) {
    let modal_rect = centered_rect(76, 22, area);
    frame.render_widget(Clear, modal_rect);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .style(Style::default().bg(Color::Rgb(20, 23, 33)))
        .title(Span::styled(
            format!(" ℹ Подробная Информация о Процессе (PID: {}) ", pid),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(modal_rect);
    frame.render_widget(block, modal_rect);

    let proc_opt = app
        .monitor
        .raw_processes
        .iter()
        .find(|p| p.pid == pid)
        .cloned();

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(12),   // Info lines
            Constraint::Length(2), // Action buttons
        ])
        .split(inner);

    if let Some(p) = proc_opt {
        let descendants = app.monitor.collect_descendants(&[p.pid]);
        let children_count = descendants.len().saturating_sub(1);

        let lines = vec![
            Line::from(vec![
                Span::styled("Имя процесса:   ", Style::default().fg(Color::Gray)),
                Span::styled(
                    &p.name,
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled("   | Статус: ", Style::default().fg(Color::Gray)),
                Span::styled(&p.status, Style::default().fg(Color::Cyan)),
            ]),
            Line::from(vec![
                Span::styled("PID / PPID:     ", Style::default().fg(Color::Gray)),
                Span::styled(
                    format!("{} / {}", p.pid, p.ppid.unwrap_or(0)),
                    Style::default().fg(Color::Yellow),
                ),
                Span::styled("   | Дочерних процессов: ", Style::default().fg(Color::Gray)),
                Span::styled(
                    format!("{}", children_count),
                    Style::default().fg(Color::Green),
                ),
            ]),
            Line::from(vec![
                Span::styled("Пользователь:   ", Style::default().fg(Color::Gray)),
                Span::styled(&p.user, Style::default().fg(Color::White)),
                Span::styled("   | Время работы: ", Style::default().fg(Color::Gray)),
                Span::styled(
                    format_duration(p.run_time),
                    Style::default().fg(Color::Green),
                ),
            ]),
            Line::from(vec![
                Span::styled("Загрузка ЦП:    ", Style::default().fg(Color::Gray)),
                Span::styled(
                    format!("{:.2}%", p.cpu_usage),
                    Style::default().fg(cpu_color(p.cpu_usage)),
                ),
            ]),
            Line::from(vec![
                Span::styled("Память (RSS):   ", Style::default().fg(Color::Gray)),
                Span::styled(
                    format!("{} ({:.2}%)", format_bytes(p.mem_bytes), p.mem_percent),
                    Style::default().fg(Color::Magenta),
                ),
                Span::styled("   | Виртуальная: ", Style::default().fg(Color::Gray)),
                Span::styled(
                    format_bytes(p.virt_bytes),
                    Style::default().fg(Color::DarkGray),
                ),
            ]),
            Line::from(vec![
                Span::styled("Диск (Чтение):  ", Style::default().fg(Color::Gray)),
                Span::styled(
                    format!(
                        "{} (Всего: {})",
                        format_speed(p.disk_read_per_sec),
                        format_bytes(p.disk_total_read)
                    ),
                    Style::default().fg(Color::LightGreen),
                ),
            ]),
            Line::from(vec![
                Span::styled("Диск (Запись):  ", Style::default().fg(Color::Gray)),
                Span::styled(
                    format!(
                        "{} (Всего: {})",
                        format_speed(p.disk_write_per_sec),
                        format_bytes(p.disk_total_write)
                    ),
                    Style::default().fg(Color::LightBlue),
                ),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled("Исполняемый:    ", Style::default().fg(Color::Gray)),
                Span::styled(&p.exe_path, Style::default().fg(Color::Cyan)),
            ]),
            Line::from(vec![
                Span::styled("Рабочая папка:  ", Style::default().fg(Color::Gray)),
                Span::styled(&p.cwd_path, Style::default().fg(Color::White)),
            ]),
            Line::from(vec![
                Span::styled("Команда:        ", Style::default().fg(Color::Gray)),
                Span::styled(&p.cmd, Style::default().fg(Color::Yellow)),
            ]),
        ];
        frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), rows[0]);
    } else {
        frame.render_widget(
            Paragraph::new("Процесс уже завершён.")
                .style(Style::default().fg(Color::LightRed)),
            rows[0],
        );
    }

    let btn_cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(2),
            Constraint::Length(22), // SIGTERM
            Constraint::Length(2),
            Constraint::Length(22), // SIGKILL
            Constraint::Length(2),
            Constraint::Min(16),    // Close
            Constraint::Length(2),
        ])
        .split(rows[1]);

    let term_target = HitTarget::ModalQuickSignal(ActionSignal::Terminate);
    frame.render_widget(
        Paragraph::new(" ⏹ Завершить (SIGTERM) ")
            .alignment(Alignment::Center)
            .style(button_style(
                app.hovered_target == Some(term_target),
                true,
                Color::White,
                Color::Rgb(140, 90, 20),
            )),
        btn_cols[1],
    );
    app.register_hitbox(btn_cols[1], term_target);

    let kill_target = HitTarget::ModalQuickSignal(ActionSignal::Kill);
    frame.render_widget(
        Paragraph::new(" 💀 Убить (SIGKILL) ")
            .alignment(Alignment::Center)
            .style(button_style(
                app.hovered_target == Some(kill_target),
                true,
                Color::White,
                Color::Rgb(160, 35, 35),
            )),
        btn_cols[3],
    );
    app.register_hitbox(btn_cols[3], kill_target);

    let close_target = HitTarget::ModalClose;
    frame.render_widget(
        Paragraph::new(" ✖ Закрыть (Esc) ")
            .alignment(Alignment::Center)
            .style(button_style(
                app.hovered_target == Some(close_target),
                true,
                Color::White,
                Color::Rgb(60, 68, 90),
            )),
        btn_cols[5],
    );
    app.register_hitbox(btn_cols[5], close_target);
}

fn draw_help_modal(frame: &mut Frame, app: &mut App, area: Rect) {
    let modal_rect = centered_rect(70, 22, area);
    frame.render_widget(Clear, modal_rect);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )
        .style(Style::default().bg(Color::Rgb(20, 23, 33)))
        .title(Span::styled(
            " ❓ Справка: Управление Мышью и Клавиатурой ",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(modal_rect);
    frame.render_widget(block, modal_rect);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(14), Constraint::Length(1)])
        .split(inner);

    let help_lines = vec![
        Line::from(Span::styled(
            "🖱 Управление Мышью:",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from("  • Клик ЛКМ по процессу — выбрать процесс в списке"),
        Line::from("  • Двойной клик ЛКМ по процессу — открыть окно закрытия / сигналов"),
        Line::from("  • Правый клик (ПКМ) по процессу — подробная информация о процессе"),
        Line::from("  • Колёсико мыши (вверх/вниз) — плавная прокрутка таблицы процессов"),
        Line::from("  • Клик по заголовкам столбцов — сортировка по столбцу (повторный клик меняет ▲/▼)"),
        Line::from("  • Клик по любым кнопкам на верхней/нижней панели — мгновенное действие"),
        Line::from(""),
        Line::from(Span::styled(
            "⌨ Горячие Клавиши:",
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from("  • x / Delete       — Завершить приложение (открыть диалог SIGTERM)"),
        Line::from("  • k / F9           — Принудительно закрыть (диалог SIGKILL)"),
        Line::from("  • s                — Приостановить / Возобновить процесс (SIGSTOP / SIGCONT)"),
        Line::from("  • Enter / i        — Подробная инспекция выбранного процесса"),
        Line::from("  • /                — Поиск / фильтрация процессов по имени или PID"),
        Line::from("  • t / g / u / K    — Режим дерева (t), Группировка (g), Мои (u), Без ядра (K)"),
        Line::from("  • 1 / 2 / 3 / Tab  — Переключение вкладок (Процессы / Графики / Диски и Сеть)"),
        Line::from("  • Space / p        — Заморозить / возобновить обновление экрана"),
        Line::from("  • q / Esc          — Выход из программы / закрытие модального окна"),
    ];
    frame.render_widget(Paragraph::new(help_lines), rows[0]);

    let close_rect = centered_rect(26, 1, rows[1]);
    let close_target = HitTarget::ModalClose;
    frame.render_widget(
        Paragraph::new(" ✖ Закрыть окно (Esc) ")
            .alignment(Alignment::Center)
            .style(button_style(
                app.hovered_target == Some(close_target),
                true,
                Color::White,
                Color::Rgb(60, 68, 90),
            )),
        close_rect,
    );
    app.register_hitbox(close_rect, close_target);
}
