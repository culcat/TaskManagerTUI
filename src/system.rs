use std::collections::{HashMap, HashSet, VecDeque};
use std::time::Instant;
use sysinfo::{Disks, Networks, Pid, ProcessRefreshKind, ProcessesToUpdate, Signal, System, Users};

pub const HISTORY_LEN: usize = 120;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortColumn {
    Pid,
    Name,
    User,
    Status,
    Cpu,
    MemPercent,
    MemBytes,
    DiskRead,
    DiskWrite,
    Command,
}

impl SortColumn {
    pub fn title(&self) -> &'static str {
        match self {
            SortColumn::Pid => "PID",
            SortColumn::Name => "Приложение / Процесс",
            SortColumn::User => "Юзер",
            SortColumn::Status => "Статус",
            SortColumn::Cpu => "CPU %",
            SortColumn::MemPercent => "MEM %",
            SortColumn::MemBytes => "Память",
            SortColumn::DiskRead => "Чтение/с",
            SortColumn::DiskWrite => "Запись/с",
            SortColumn::Command => "Команда",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionSignal {
    Terminate, // SIGTERM (15)
    Kill,      // SIGKILL (9)
    Interrupt, // SIGINT (2)
    Hangup,    // SIGHUP (1)
    Stop,      // SIGSTOP (19)
    Continue,  // SIGCONT (18)
}

impl ActionSignal {
    pub fn all() -> &'static [ActionSignal] {
        &[
            ActionSignal::Terminate,
            ActionSignal::Kill,
            ActionSignal::Interrupt,
            ActionSignal::Stop,
            ActionSignal::Continue,
            ActionSignal::Hangup,
        ]
    }

    pub fn to_sysinfo(self) -> Signal {
        match self {
            ActionSignal::Terminate => Signal::Term,
            ActionSignal::Kill => Signal::Kill,
            ActionSignal::Interrupt => Signal::Interrupt,
            ActionSignal::Hangup => Signal::Hangup,
            ActionSignal::Stop => Signal::Stop,
            ActionSignal::Continue => Signal::Continue,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            ActionSignal::Terminate => "SIGTERM (15) — Мягкое закрытие",
            ActionSignal::Kill => "SIGKILL (9)  — Принудительно убить",
            ActionSignal::Interrupt => "SIGINT  (2)  — Прерывание (Ctrl+C)",
            ActionSignal::Stop => "SIGSTOP (19) — Приостановить (Заморозка)",
            ActionSignal::Continue => "SIGCONT (18) — Возобновить работу",
            ActionSignal::Hangup => "SIGHUP  (1)  — Закрытие терминала / Reload",
        }
    }

    pub fn short_name(self) -> &'static str {
        match self {
            ActionSignal::Terminate => "SIGTERM",
            ActionSignal::Kill => "SIGKILL",
            ActionSignal::Interrupt => "SIGINT",
            ActionSignal::Stop => "SIGSTOP",
            ActionSignal::Continue => "SIGCONT",
            ActionSignal::Hangup => "SIGHUP",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ProcessEntry {
    pub pid: u32,
    pub ppid: Option<u32>,
    pub name: String,
    pub display_name: String,
    pub user: String,
    pub status: String,
    pub cpu_usage: f32,
    pub mem_bytes: u64,
    pub virt_bytes: u64,
    pub mem_percent: f32,
    pub disk_read_per_sec: u64,
    pub disk_write_per_sec: u64,
    pub disk_total_read: u64,
    pub disk_total_write: u64,
    pub run_time: u64,
    pub cmd: String,
    pub exe_path: String,
    pub cwd_path: String,
    pub group_count: usize,
    pub group_pids: Vec<u32>,
}

#[derive(Debug, Clone)]
pub struct DiskEntry {
    pub name: String,
    pub mount_point: String,
    pub file_system: String,
    pub total_space: u64,
    pub available_space: u64,
    pub used_space: u64,
    pub usage_percent: f32,
    pub is_removable: bool,
}

#[derive(Debug, Clone)]
pub struct NetEntry {
    pub interface: String,
    pub rx_bytes_per_sec: u64,
    pub tx_bytes_per_sec: u64,
    pub total_rx: u64,
    pub total_tx: u64,
}

#[derive(Debug, Clone)]
pub struct CoreInfo {
    pub name: String,
    pub usage: f32,
    pub frequency: u64,
}

pub struct SystemMonitor {
    pub sys: System,
    pub disks: Disks,
    pub networks: Networks,
    pub users: Users,
    pub last_refresh: Instant,
    pub cpu_history: VecDeque<u64>,
    pub mem_history: VecDeque<u64>,
    pub net_rx_history: VecDeque<u64>,
    pub net_tx_history: VecDeque<u64>,
    pub cores: Vec<CoreInfo>,
    pub global_cpu: f32,
    pub total_mem: u64,
    pub used_mem: u64,
    pub available_mem: u64,
    pub total_swap: u64,
    pub used_swap: u64,
    pub uptime: u64,
    pub load_avg: (f64, f64, f64),
    pub hostname: String,
    pub os_name: String,
    pub current_user: String,
    pub raw_processes: Vec<ProcessEntry>,
    pub disk_entries: Vec<DiskEntry>,
    pub net_entries: Vec<NetEntry>,
    pub total_rx_speed: u64,
    pub total_tx_speed: u64,
}

impl SystemMonitor {
    pub fn new() -> Self {
        let mut sys = System::new_all().expect("Failed to initialize sysinfo::System");
        let _ = sys.refresh_all();
        let disks = Disks::new_with_refreshed_list().expect("Failed to initialize Disks");
        let networks = Networks::new_with_refreshed_list().expect("Failed to initialize Networks");
        let users = Users::new_with_refreshed_list().expect("Failed to initialize Users");

        let hostname = System::host_name().unwrap_or_else(|_| "localhost".to_string());
        let os_name = format!(
            "{} {}",
            System::name().unwrap_or_else(|_| "Linux".to_string()),
            System::os_version().unwrap_or_default()
        )
        .trim()
        .to_string();

        let current_user = std::env::var("USER")
            .or_else(|_| std::env::var("USERNAME"))
            .unwrap_or_else(|_| "root".to_string());

        let mut monitor = Self {
            sys,
            disks,
            networks,
            users,
            last_refresh: Instant::now(),
            cpu_history: VecDeque::from(vec![0; HISTORY_LEN]),
            mem_history: VecDeque::from(vec![0; HISTORY_LEN]),
            net_rx_history: VecDeque::from(vec![0; HISTORY_LEN]),
            net_tx_history: VecDeque::from(vec![0; HISTORY_LEN]),
            cores: Vec::new(),
            global_cpu: 0.0,
            total_mem: 0,
            used_mem: 0,
            available_mem: 0,
            total_swap: 0,
            used_swap: 0,
            uptime: 0,
            load_avg: (0.0, 0.0, 0.0),
            hostname,
            os_name,
            current_user,
            raw_processes: Vec::new(),
            disk_entries: Vec::new(),
            net_entries: Vec::new(),
            total_rx_speed: 0,
            total_tx_speed: 0,
        };
        monitor.refresh();
        monitor
    }

    pub fn refresh(&mut self) {
        let elapsed = self.last_refresh.elapsed().as_secs_f64().max(0.05);
        self.last_refresh = Instant::now();

        let _ = self.sys.refresh_cpu_all();
        let _ = self.sys.refresh_memory();
        let _ = self.sys.refresh_processes_specifics(
            ProcessesToUpdate::All,
            true,
            ProcessRefreshKind::everything(),
        );
        let _ = self.disks.refresh(true);
        let _ = self.networks.refresh(true);

        self.global_cpu = self.sys.global_cpu_usage();
        self.total_mem = self.sys.total_memory();
        self.used_mem = self.sys.used_memory();
        self.available_mem = self.sys.available_memory();
        self.total_swap = self.sys.total_swap();
        self.used_swap = self.sys.used_swap();
        self.uptime = System::uptime().unwrap_or(0);
        if let Ok(l) = System::load_average() {
            self.load_avg = (l.one, l.five, l.fifteen);
        }

        // Push histories
        if self.cpu_history.len() >= HISTORY_LEN {
            self.cpu_history.pop_front();
        }
        self.cpu_history.push_back(self.global_cpu.round() as u64);

        let mem_pct = if self.total_mem > 0 {
            ((self.used_mem as f64 / self.total_mem as f64) * 100.0).round() as u64
        } else {
            0
        };
        if self.mem_history.len() >= HISTORY_LEN {
            self.mem_history.pop_front();
        }
        self.mem_history.push_back(mem_pct);

        self.cores = self
            .sys
            .cpus()
            .iter()
            .enumerate()
            .map(|(idx, cpu)| CoreInfo {
                name: format!("CPU {:02}", idx),
                usage: cpu.usage(),
                frequency: cpu.frequency(),
            })
            .collect();

        // Map users
        let mut user_map: HashMap<sysinfo::Uid, String> = HashMap::new();
        for u in self.users.iter() {
            user_map.insert(u.id().clone(), u.name().to_string());
        }

        let total_mem_f = self.total_mem.max(1) as f32;
        let mut procs = Vec::with_capacity(self.sys.processes().len());

        for (pid, proc_info) in self.sys.processes() {
            // Skip kernel threads that have empty exe and minimal memory if user wants real processes,
            // Actually keep all processes, or format command nicely!
            let pid_u32 = pid.as_u32();
            let ppid_u32 = proc_info.parent().map(|p| p.as_u32());
            let name = proc_info.name().to_string_lossy().to_string();

            let user = proc_info
                .user_id()
                .and_then(|uid| user_map.get(uid).cloned())
                .unwrap_or_else(|| "-".to_string());

            let status = format!("{:?}", proc_info.status());
            let cpu_usage = proc_info.cpu_usage();
            let mem_bytes = proc_info.memory();
            let virt_bytes = proc_info.virtual_memory();
            let mem_percent = (mem_bytes as f32 / total_mem_f) * 100.0;

            let du = proc_info.disk_usage();
            let disk_read_per_sec = (du.read_bytes as f64 / elapsed) as u64;
            let disk_write_per_sec = (du.written_bytes as f64 / elapsed) as u64;

            let cmd_parts: Vec<String> = proc_info
                .cmd()
                .iter()
                .map(|s| s.to_string_lossy().to_string())
                .collect();
            let cmd = if cmd_parts.is_empty() {
                format!("[{}]", name)
            } else {
                cmd_parts.join(" ")
            };

            let exe_path = proc_info
                .exe()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|| "-".to_string());

            let cwd_path = proc_info
                .cwd()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|| "-".to_string());

            procs.push(ProcessEntry {
                pid: pid_u32,
                ppid: ppid_u32,
                name: name.clone(),
                display_name: name,
                user,
                status,
                cpu_usage,
                mem_bytes,
                virt_bytes,
                mem_percent,
                disk_read_per_sec,
                disk_write_per_sec,
                disk_total_read: du.total_read_bytes,
                disk_total_write: du.total_written_bytes,
                run_time: proc_info.run_time(),
                cmd,
                exe_path,
                cwd_path,
                group_count: 1,
                group_pids: vec![pid_u32],
            });
        }

        self.raw_processes = procs;

        // Disks
        let mut disk_list = Vec::new();
        for d in self.disks.iter() {
            let total = d.total_space();
            if total == 0 {
                continue;
            }
            let avail = d.available_space();
            let used = total.saturating_sub(avail);
            let pct = (used as f32 / total as f32) * 100.0;
            disk_list.push(DiskEntry {
                name: d.name().to_string_lossy().to_string(),
                mount_point: d.mount_point().to_string_lossy().to_string(),
                file_system: d.file_system().to_string_lossy().to_string(),
                total_space: total,
                available_space: avail,
                used_space: used,
                usage_percent: pct,
                is_removable: d.is_removable(),
            });
        }
        disk_list.sort_by(|a, b| a.mount_point.cmp(&b.mount_point));
        self.disk_entries = disk_list;

        // Networks
        let mut net_list = Vec::new();
        let mut rx_sum = 0u64;
        let mut tx_sum = 0u64;
        for (iface, data) in &self.networks {
            let rx_s = (data.received() as f64 / elapsed) as u64;
            let tx_s = (data.transmitted() as f64 / elapsed) as u64;
            rx_sum += rx_s;
            tx_sum += tx_s;
            net_list.push(NetEntry {
                interface: iface.to_string_lossy().to_string(),
                rx_bytes_per_sec: rx_s,
                tx_bytes_per_sec: tx_s,
                total_rx: data.total_received(),
                total_tx: data.total_transmitted(),
            });
        }
        net_list.sort_by(|a, b| b.total_rx.cmp(&a.total_rx));
        self.net_entries = net_list;
        self.total_rx_speed = rx_sum;
        self.total_tx_speed = tx_sum;

        if self.net_rx_history.len() >= HISTORY_LEN {
            self.net_rx_history.pop_front();
        }
        self.net_rx_history.push_back(rx_sum / 1024);

        if self.net_tx_history.len() >= HISTORY_LEN {
            self.net_tx_history.pop_front();
        }
        self.net_tx_history.push_back(tx_sum / 1024);
    }

    pub fn build_view_processes(
        &self,
        filter: &str,
        only_my_user: bool,
        hide_kernel_threads: bool,
        tree_mode: bool,
        group_apps: bool,
        sort_col: SortColumn,
        sort_asc: bool,
    ) -> Vec<ProcessEntry> {
        let filter_lower = filter.trim().to_lowercase();

        let filtered: Vec<ProcessEntry> = self
            .raw_processes
            .iter()
            .filter(|p| {
                if only_my_user && p.user != self.current_user {
                    return false;
                }
                if hide_kernel_threads && p.exe_path == "-" && p.mem_bytes == 0 {
                    return false;
                }
                if !filter_lower.is_empty() {
                    let match_name = p.name.to_lowercase().contains(&filter_lower);
                    let match_cmd = p.cmd.to_lowercase().contains(&filter_lower);
                    let match_pid = p.pid.to_string().contains(&filter_lower);
                    let match_user = p.user.to_lowercase().contains(&filter_lower);
                    return match_name || match_cmd || match_pid || match_user;
                }
                true
            })
            .cloned()
            .collect();

        if group_apps {
            let mut groups: HashMap<String, ProcessEntry> = HashMap::new();
            for p in filtered {
                let entry = groups.entry(p.name.clone()).or_insert_with(|| {
                    let mut base = p.clone();
                    base.cpu_usage = 0.0;
                    base.mem_bytes = 0;
                    base.virt_bytes = 0;
                    base.mem_percent = 0.0;
                    base.disk_read_per_sec = 0;
                    base.disk_write_per_sec = 0;
                    base.disk_total_read = 0;
                    base.disk_total_write = 0;
                    base.group_count = 0;
                    base.group_pids.clear();
                    base
                });
                entry.cpu_usage += p.cpu_usage;
                entry.mem_bytes += p.mem_bytes;
                entry.virt_bytes += p.virt_bytes;
                entry.mem_percent += p.mem_percent;
                entry.disk_read_per_sec += p.disk_read_per_sec;
                entry.disk_write_per_sec += p.disk_write_per_sec;
                entry.disk_total_read += p.disk_total_read;
                entry.disk_total_write += p.disk_total_write;
                entry.group_count += 1;
                entry.group_pids.push(p.pid);
                if p.pid < entry.pid {
                    entry.pid = p.pid;
                }
            }
            let mut grouped: Vec<ProcessEntry> = groups
                .into_values()
                .map(|mut g| {
                    if g.group_count > 1 {
                        g.display_name = format!("📦 {} ({})", g.name, g.group_count);
                    }
                    g
                })
                .collect();
            Self::sort_slice(&mut grouped, sort_col, sort_asc);
            return grouped;
        }

        if !tree_mode {
            let mut list = filtered;
            Self::sort_slice(&mut list, sort_col, sort_asc);
            return list;
        }

        // Build process tree
        let pid_set: HashSet<u32> = filtered.iter().map(|p| p.pid).collect();
        let mut children_map: HashMap<u32, Vec<ProcessEntry>> = HashMap::new();
        let mut roots: Vec<ProcessEntry> = Vec::new();

        for p in filtered {
            if let Some(ppid) = p.ppid {
                if ppid != p.pid && pid_set.contains(&ppid) {
                    children_map.entry(ppid).or_default().push(p);
                    continue;
                }
            }
            roots.push(p);
        }

        Self::sort_slice(&mut roots, sort_col, sort_asc);
        for children in children_map.values_mut() {
            Self::sort_slice(children, sort_col, sort_asc);
        }

        let mut out = Vec::new();
        let mut visited = HashSet::new();
        for root in roots {
            Self::flatten_tree(&root, &children_map, "", true, true, &mut out, &mut visited);
        }
        out
    }

    fn flatten_tree(
        node: &ProcessEntry,
        children_map: &HashMap<u32, Vec<ProcessEntry>>,
        prefix: &str,
        is_last: bool,
        is_root: bool,
        out: &mut Vec<ProcessEntry>,
        visited: &mut HashSet<u32>,
    ) {
        if !visited.insert(node.pid) {
            return;
        }
        let mut item = node.clone();
        if !is_root {
            let branch = if is_last { "└─ " } else { "├─ " };
            item.display_name = format!("{}{}{}", prefix, branch, node.name);
        }
        out.push(item);

        if let Some(children) = children_map.get(&node.pid) {
            let next_prefix = if is_root {
                String::new()
            } else if is_last {
                format!("{}   ", prefix)
            } else {
                format!("{}│  ", prefix)
            };
            for (idx, child) in children.iter().enumerate() {
                let last_child = idx + 1 == children.len();
                Self::flatten_tree(
                    child,
                    children_map,
                    &next_prefix,
                    last_child,
                    false,
                    out,
                    visited,
                );
            }
        }
    }

    fn sort_slice(list: &mut [ProcessEntry], col: SortColumn, asc: bool) {
        list.sort_by(|a, b| {
            let ord = match col {
                SortColumn::Pid => a.pid.cmp(&b.pid),
                SortColumn::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
                SortColumn::User => a.user.to_lowercase().cmp(&b.user.to_lowercase()),
                SortColumn::Status => a.status.cmp(&b.status),
                SortColumn::Cpu => a
                    .cpu_usage
                    .partial_cmp(&b.cpu_usage)
                    .unwrap_or(std::cmp::Ordering::Equal),
                SortColumn::MemPercent | SortColumn::MemBytes => a.mem_bytes.cmp(&b.mem_bytes),
                SortColumn::DiskRead => a.disk_read_per_sec.cmp(&b.disk_read_per_sec),
                SortColumn::DiskWrite => a.disk_write_per_sec.cmp(&b.disk_write_per_sec),
                SortColumn::Command => a.cmd.to_lowercase().cmp(&b.cmd.to_lowercase()),
            };
            let ord = if ord == std::cmp::Ordering::Equal {
                a.pid.cmp(&b.pid)
            } else {
                ord
            };
            if asc {
                ord
            } else {
                ord.reverse()
            }
        });
    }

    pub fn collect_descendants(&self, root_pids: &[u32]) -> Vec<u32> {
        let mut children_map: HashMap<u32, Vec<u32>> = HashMap::new();
        for p in &self.raw_processes {
            if let Some(ppid) = p.ppid {
                children_map.entry(ppid).or_default().push(p.pid);
            }
        }
        let mut result = Vec::new();
        let mut visited = HashSet::new();
        let mut queue: VecDeque<u32> = root_pids.iter().copied().collect();
        while let Some(pid) = queue.pop_front() {
            if visited.insert(pid) {
                result.push(pid);
                if let Some(ch) = children_map.get(&pid) {
                    for &c in ch {
                        queue.push_back(c);
                    }
                }
            }
        }
        result
    }

    pub fn send_signal_to_pids(
        &mut self,
        pids: &[u32],
        signal: ActionSignal,
        include_children: bool,
    ) -> Result<String, String> {
        let target_pids = if include_children {
            self.collect_descendants(pids)
        } else {
            pids.to_vec()
        };

        if target_pids.is_empty() {
            return Err("Не выбрано ни одного процесса".to_string());
        }

        let sys_sig = signal.to_sysinfo();
        let mut ok_count = 0usize;
        let mut fail_count = 0usize;
        let mut first_name = String::new();

        for &pid_u32 in &target_pids {
            let pid = Pid::from_u32(pid_u32);
            if let Some(proc_info) = self.sys.process(pid) {
                if first_name.is_empty() {
                    first_name = proc_info.name().to_string_lossy().to_string();
                }
                if proc_info.kill_with(sys_sig).unwrap_or(false) {
                    ok_count += 1;
                } else {
                    fail_count += 1;
                }
            }
        }

        if ok_count > 0 {
            let msg = if target_pids.len() == 1 {
                format!(
                    "Отправлен {} процессу {} ({})",
                    signal.short_name(),
                    target_pids[0],
                    first_name
                )
            } else {
                format!(
                    "Отправлен {} → {} процессам ({}) [ошибок: {}]",
                    signal.short_name(),
                    ok_count,
                    first_name,
                    fail_count
                )
            };
            Ok(msg)
        } else {
            Err(format!(
                "Не удалось отправить {} (возможно, недостаточно прав или процесс уже завершён)",
                signal.short_name()
            ))
        }
    }
}

pub fn format_bytes(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    const TB: f64 = GB * 1024.0;

    let b = bytes as f64;
    if b >= TB {
        format!("{:.2} ТБ", b / TB)
    } else if b >= GB {
        format!("{:.2} ГБ", b / GB)
    } else if b >= MB {
        format!("{:.1} МБ", b / MB)
    } else if b >= KB {
        format!("{:.0} КБ", b / KB)
    } else {
        format!("{} Б", bytes)
    }
}

pub fn format_speed(bytes_per_sec: u64) -> String {
    if bytes_per_sec == 0 {
        return "0 Б/с".to_string();
    }
    format!("{}/с", format_bytes(bytes_per_sec))
}

pub fn format_duration(secs: u64) -> String {
    let days = secs / 86400;
    let hours = (secs % 86400) / 3600;
    let mins = (secs % 3600) / 60;
    let s = secs % 60;
    if days > 0 {
        format!("{}д {:02}:{:02}:{:02}", days, hours, mins, s)
    } else {
        format!("{:02}:{:02}:{:02}", hours, mins, s)
    }
}
