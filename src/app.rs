use crate::system::{ActionSignal, ProcessEntry, SortColumn, SystemMonitor};
use ratatui::layout::Rect;
use ratatui::widgets::TableState;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActiveTab {
    Processes,
    Performance,
    DisksAndNet,
}

impl ActiveTab {
    pub fn all() -> &'static [ActiveTab] {
        &[
            ActiveTab::Processes,
            ActiveTab::Performance,
            ActiveTab::DisksAndNet,
        ]
    }

    pub fn title(self) -> &'static str {
        match self {
            ActiveTab::Processes => " 1: Процессы и Приложения ",
            ActiveTab::Performance => " 2: ЦП и Память (Графики) ",
            ActiveTab::DisksAndNet => " 3: Диски и Сеть ",
        }
    }
}

#[derive(Debug, Clone)]
pub enum ModalState {
    None,
    SignalConfirm {
        pids: Vec<u32>,
        proc_name: String,
        selected_signal: ActionSignal,
        include_children: bool,
    },
    ProcessDetails {
        pid: u32,
    },
    Help,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HitTarget {
    Tab(ActiveTab),
    RefreshFaster,
    RefreshSlower,
    TogglePause,
    ToggleTree,
    ToggleGroupApps,
    ToggleMyUser,
    ToggleHideKernel,
    SearchBox,
    ClearSearch,
    SortHeader(SortColumn),
    ProcessRow(usize),
    ProcessTableBody,
    ActionTerminate,
    ActionKill,
    ActionSignalMenu,
    ActionStopResume,
    ActionDetails,
    ActionHelp,
    ActionQuit,
    // Modal targets
    ModalClose,
    ModalConfirmExecute,
    ModalToggleChildren,
    ModalSelectSignal(ActionSignal),
    ModalQuickSignal(ActionSignal),
}

#[derive(Debug, Clone)]
pub struct HitBox {
    pub rect: Rect,
    pub target: HitTarget,
}

#[derive(Debug, Clone)]
pub struct ToastMessage {
    pub text: String,
    pub is_error: bool,
    pub created_at: Instant,
}

pub struct App {
    pub monitor: SystemMonitor,
    pub running: bool,
    pub paused: bool,
    pub active_tab: ActiveTab,
    pub refresh_ms: u64,
    pub last_tick: Instant,

    // Process table view settings
    pub sort_col: SortColumn,
    pub sort_asc: bool,
    pub tree_mode: bool,
    pub group_apps: bool,
    pub only_my_user: bool,
    pub hide_kernel_threads: bool,

    // Search / Filter
    pub search_query: String,
    pub search_focused: bool,

    // Process list & selection
    pub view_processes: Vec<ProcessEntry>,
    pub table_state: TableState,
    pub selected_pid: Option<u32>,

    // Modal state
    pub modal: ModalState,

    // Status / Toast notification
    pub toast: Option<ToastMessage>,

    // Mouse interactivity state
    pub hitboxes: Vec<HitBox>,
    pub mouse_pos: (u16, u16),
    pub hovered_target: Option<HitTarget>,
    pub last_click: Option<(Instant, HitTarget)>,
}

impl App {
    pub fn new() -> Self {
        let monitor = SystemMonitor::new();
        let mut app = Self {
            monitor,
            running: true,
            paused: false,
            active_tab: ActiveTab::Processes,
            refresh_ms: 1000,
            last_tick: Instant::now(),
            sort_col: SortColumn::Cpu,
            sort_asc: false,
            tree_mode: false,
            group_apps: false,
            only_my_user: false,
            hide_kernel_threads: true,
            search_query: String::new(),
            search_focused: false,
            view_processes: Vec::new(),
            table_state: TableState::default(),
            selected_pid: None,
            modal: ModalState::None,
            toast: Some(ToastMessage {
                text: "Мышь активна: кликайте по вкладкам, заголовкам таблиц, процессам и кнопкам действий!".to_string(),
                is_error: false,
                created_at: Instant::now(),
            }),
            hitboxes: Vec::new(),
            mouse_pos: (0, 0),
            hovered_target: None,
            last_click: None,
        };
        app.rebuild_process_list();
        if !app.view_processes.is_empty() {
            app.table_state.select(Some(0));
            app.selected_pid = Some(app.view_processes[0].pid);
        }
        app
    }

    pub fn set_toast(&mut self, text: impl Into<String>, is_error: bool) {
        self.toast = Some(ToastMessage {
            text: text.into(),
            is_error,
            created_at: Instant::now(),
        });
    }

    pub fn on_tick(&mut self) {
        if let Some(t) = &self.toast {
            if t.created_at.elapsed() > Duration::from_secs(6) {
                self.toast = None;
            }
        }
        if !self.paused {
            self.monitor.refresh();
            self.rebuild_process_list();
        }
        self.last_tick = Instant::now();
    }

    pub fn force_refresh(&mut self) {
        self.monitor.refresh();
        self.rebuild_process_list();
        self.last_tick = Instant::now();
    }

    pub fn rebuild_process_list(&mut self) {
        self.view_processes = self.monitor.build_view_processes(
            &self.search_query,
            self.only_my_user,
            self.hide_kernel_threads,
            self.tree_mode,
            self.group_apps,
            self.sort_col,
            self.sort_asc,
        );

        if self.view_processes.is_empty() {
            self.table_state.select(None);
            self.selected_pid = None;
            return;
        }

        // Preserve selection by PID if possible
        if let Some(pid) = self.selected_pid {
            if let Some(pos) = self
                .view_processes
                .iter()
                .position(|p| p.pid == pid || p.group_pids.contains(&pid))
            {
                self.table_state.select(Some(pos));
                self.selected_pid = Some(self.view_processes[pos].pid);
                return;
            }
        }

        // Otherwise clamp previous index
        let idx = self
            .table_state
            .selected()
            .unwrap_or(0)
            .min(self.view_processes.len().saturating_sub(1));
        self.table_state.select(Some(idx));
        self.selected_pid = Some(self.view_processes[idx].pid);
    }

    pub fn selected_process(&self) -> Option<&ProcessEntry> {
        let idx = self.table_state.selected()?;
        self.view_processes.get(idx)
    }

    pub fn select_index(&mut self, idx: usize) {
        if self.view_processes.is_empty() {
            self.table_state.select(None);
            self.selected_pid = None;
            return;
        }
        let clamped = idx.min(self.view_processes.len() - 1);
        self.table_state.select(Some(clamped));
        self.selected_pid = Some(self.view_processes[clamped].pid);
    }

    pub fn select_next(&mut self, step: usize) {
        if self.view_processes.is_empty() {
            return;
        }
        let cur = self.table_state.selected().unwrap_or(0);
        self.select_index(cur.saturating_add(step));
    }

    pub fn select_prev(&mut self, step: usize) {
        if self.view_processes.is_empty() {
            return;
        }
        let cur = self.table_state.selected().unwrap_or(0);
        self.select_index(cur.saturating_sub(step));
    }

    pub fn toggle_sort(&mut self, col: SortColumn) {
        if self.sort_col == col {
            self.sort_asc = !self.sort_asc;
        } else {
            self.sort_col = col;
            // Default direction: ascending for text/PID, descending for resource usage
            self.sort_asc = matches!(
                col,
                SortColumn::Pid | SortColumn::Name | SortColumn::User | SortColumn::Command
            );
        }
        self.rebuild_process_list();
        self.select_index(0);
        *self.table_state.offset_mut() = 0;
    }

    pub fn open_signal_modal(&mut self, default_signal: ActionSignal) {
        if let Some(proc_entry) = self.selected_process().cloned() {
            let include_children = self.tree_mode || proc_entry.group_count > 1;
            self.modal = ModalState::SignalConfirm {
                pids: proc_entry.group_pids.clone(),
                proc_name: proc_entry.name.clone(),
                selected_signal: default_signal,
                include_children,
            };
        } else {
            self.set_toast("Сначала выберите процесс в таблице", true);
        }
    }

    pub fn open_details_modal(&mut self) {
        if let Some(proc_entry) = self.selected_process() {
            self.modal = ModalState::ProcessDetails {
                pid: proc_entry.pid,
            };
        } else {
            self.set_toast("Сначала выберите процесс в таблице", true);
        }
    }

    pub fn execute_quick_signal(&mut self, signal: ActionSignal) {
        if let Some(proc_entry) = self.selected_process().cloned() {
            let res = self
                .monitor
                .send_signal_to_pids(&proc_entry.group_pids, signal, false);
            match res {
                Ok(msg) => self.set_toast(msg, false),
                Err(err) => self.set_toast(err, true),
            }
            self.force_refresh();
        } else {
            self.set_toast("Сначала выберите процесс", true);
        }
    }

    pub fn toggle_stop_resume_selected(&mut self) {
        if let Some(proc_entry) = self.selected_process().cloned() {
            let is_stopped = proc_entry.status.to_lowercase().contains("stop")
                || proc_entry.status.to_lowercase().contains("park");
            let sig = if is_stopped {
                ActionSignal::Continue
            } else {
                ActionSignal::Stop
            };
            self.execute_quick_signal(sig);
        } else {
            self.set_toast("Сначала выберите процесс", true);
        }
    }

    pub fn confirm_modal_action(&mut self) {
        if let ModalState::SignalConfirm {
            pids,
            selected_signal,
            include_children,
            ..
        } = self.modal.clone()
        {
            let res = self
                .monitor
                .send_signal_to_pids(&pids, selected_signal, include_children);
            match res {
                Ok(msg) => self.set_toast(msg, false),
                Err(err) => self.set_toast(err, true),
            }
            self.modal = ModalState::None;
            self.force_refresh();
        }
    }

    pub fn clear_hitboxes(&mut self) {
        self.hitboxes.clear();
    }

    pub fn register_hitbox(&mut self, rect: Rect, target: HitTarget) {
        self.hitboxes.push(HitBox { rect, target });
    }

    pub fn hit_test(&self, col: u16, row: u16) -> Option<HitTarget> {
        // Iterate in reverse so top-most (e.g. modal) hitboxes win over background ones
        for hb in self.hitboxes.iter().rev() {
            if col >= hb.rect.x
                && col < hb.rect.x.saturating_add(hb.rect.width)
                && row >= hb.rect.y
                && row < hb.rect.y.saturating_add(hb.rect.height)
            {
                return Some(hb.target);
            }
        }
        None
    }

    pub fn update_hover(&mut self, col: u16, row: u16) {
        self.mouse_pos = (col, row);
        self.hovered_target = self.hit_test(col, row);
    }
}
