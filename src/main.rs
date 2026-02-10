//! A high-performance resource monitor TUI application using ratatui, sysinfo, and crossterm.
//! Features real-time CPU usage, RAM gauge, thermal sensors, network sparklines,
//! disk I/O monitoring, and a sortable process list with configurable scan rate and logging.

pub mod errors {
        use std::fmt;
        use std::io;

        #[derive(Debug)]
        pub enum SysmonError {
            Io(io::Error),
        }

        impl fmt::Display for SysmonError {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                match self {
                    SysmonError::Io(e) => write!(f, "I/O error: {}", e),
                }
            }
        }

        impl std::error::Error for SysmonError {}

        impl From<io::Error> for SysmonError {
            fn from(e: io::Error) -> Self {
                SysmonError::Io(e)
            }
        }
}

pub mod event {
        use std::sync::mpsc::Sender;
        use std::time::Duration;

        use crossterm::event::{self, Event as CEvent, KeyEvent};

        pub enum AppEvent {
            Tick,
            Input(KeyEvent),
        }

        pub struct EventHandler {
            pub tick_rate: Duration,
        }

        impl EventHandler {
            pub fn new(tick_rate: Duration) -> Self {
                EventHandler { tick_rate }
            }

            pub fn run(&self, tx: Sender<AppEvent>) {
                let tick_rate = self.tick_rate;
                std::thread::spawn(move || {
                    loop {
                        if event::poll(tick_rate).unwrap_or(false) {
                            if let Ok(CEvent::Key(key)) = event::read() {
                                let _ = tx.send(AppEvent::Input(key));
                            }
                        } else {
                            let _ = tx.send(AppEvent::Tick);
                        }
                    }
                });
            }
        }
}

pub mod types {
        #[derive(Clone)]
        pub struct CpuCoreUsage {
            pub core_id: usize,
            pub usage_percent: f32,
        }

        #[derive(Clone)]
        pub struct RamSwapUsage {
            pub used: u64,
            pub total: u64,
        }

        #[derive(Clone)]
        pub struct NetworkStats {
            pub received_bytes: u64,
            pub transmitted_bytes: u64,
        }

        #[derive(Clone)]
        pub struct DiskIOStats {
            pub read_bytes: u64,
            pub write_bytes: u64,
        }

        #[derive(Clone)]
        pub struct ProcessInfo {
            pub pid: i32,
            pub name: String,
            pub cpu_percent: f32,
            pub mem_bytes: u64,
        }

        #[derive(Clone)]
        pub struct ThermalInfo {
            pub label: String,
            pub temp_celsius: f32,
            pub critical_celsius: Option<f32>,
        }

        #[derive(Clone, Copy)]
        pub enum SortOrder {
            Cpu,
            Mem,
        }

        #[derive(Clone)]
        pub struct SystemMetrics {
            pub cpu: Vec<CpuCoreUsage>,
            pub ram: RamSwapUsage,
            pub swap: RamSwapUsage,
            pub network: NetworkStats,
            pub disk_io: DiskIOStats,
            pub processes: Vec<ProcessInfo>,
            pub thermals: Vec<ThermalInfo>,
        }

        /// Rolling history for sparkline widgets.
        pub struct SparklineHistory {
            pub net_rx: std::collections::VecDeque<u64>,
            pub net_tx: std::collections::VecDeque<u64>,
            pub disk_read: std::collections::VecDeque<u64>,
            pub disk_write: std::collections::VecDeque<u64>,
            capacity: usize,
        }

        impl SparklineHistory {
            pub fn new(capacity: usize) -> Self {
                Self {
                    net_rx: std::collections::VecDeque::with_capacity(capacity),
                    net_tx: std::collections::VecDeque::with_capacity(capacity),
                    disk_read: std::collections::VecDeque::with_capacity(capacity),
                    disk_write: std::collections::VecDeque::with_capacity(capacity),
                    capacity,
                }
            }

            pub fn push(&mut self, net: &NetworkStats, disk: &DiskIOStats) {
                if self.net_rx.len() >= self.capacity {
                    self.net_rx.pop_front();
                    self.net_tx.pop_front();
                    self.disk_read.pop_front();
                    self.disk_write.pop_front();
                }
                self.net_rx.push_back(net.received_bytes);
                self.net_tx.push_back(net.transmitted_bytes);
                self.disk_read.push_back(disk.read_bytes);
                self.disk_write.push_back(disk.write_bytes);
            }
        }
}

pub mod collector {
        use std::cmp::Ordering;
        use sysinfo::{System, Networks, Components};

        pub struct Collector {
            sys: System,
            networks: Networks,
            components: Components,
            tick: u32,
            pub process_every: u32,
            last_disk_io: crate::types::DiskIOStats,
            last_processes: Vec<crate::types::ProcessInfo>,
            last_thermals: Vec<crate::types::ThermalInfo>,
        }

        impl Collector {
            pub fn new() -> Self {
                let mut sys = System::new_all();
                sys.refresh_all();
                let networks = Networks::new_with_refreshed_list();
                let components = Components::new_with_refreshed_list();
                Self {
                    sys, networks, components,
                    tick: 0,
                    process_every: 4, // default: every 4th tick = 1/s
                    last_disk_io: crate::types::DiskIOStats { read_bytes: 0, write_bytes: 0 },
                    last_processes: Vec::new(),
                    last_thermals: Vec::new(),
                }
            }

            pub fn collect(&mut self) -> crate::types::SystemMetrics {
                // Cheap — every tick (250ms)
                self.sys.refresh_cpu_usage();
                self.sys.refresh_memory();
                self.networks.refresh(false);

                // Expensive — every Nth tick (configurable)
                let full = self.tick % self.process_every == 0;
                if full {
                    self.sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
                    self.components.refresh(false);
                }
                self.tick = self.tick.wrapping_add(1);

                // CPU
                let cpu: Vec<crate::types::CpuCoreUsage> = self
                    .sys
                    .cpus()
                    .iter()
                    .enumerate()
                    .map(|(idx, cpu)| crate::types::CpuCoreUsage {
                        core_id: idx,
                        usage_percent: cpu.cpu_usage(),
                    })
                    .collect();

                // RAM
                let ram = crate::types::RamSwapUsage {
                    used: self.sys.used_memory(),
                    total: self.sys.total_memory(),
                };

                let swap = crate::types::RamSwapUsage {
                    used: self.sys.used_swap(),
                    total: self.sys.total_swap(),
                };

                // Network
                let mut net_recv = 0u64;
                let mut net_trans = 0u64;
                for (_name, data) in &self.networks {
                    net_recv += data.received();
                    net_trans += data.transmitted();
                }
                let network = crate::types::NetworkStats {
                    received_bytes: net_recv,
                    transmitted_bytes: net_trans,
                };

                // Disk I/O, Thermals, Processes — only on full refresh
                let disk_io = if full {
                    let mut disk_read = 0u64;
                    let mut disk_write = 0u64;
                    for (_pid, process) in self.sys.processes() {
                        let usage = process.disk_usage();
                        disk_read += usage.read_bytes;
                        disk_write += usage.written_bytes;
                    }
                    self.last_disk_io = crate::types::DiskIOStats {
                        read_bytes: disk_read,
                        write_bytes: disk_write,
                    };
                    self.last_disk_io.clone()
                } else {
                    self.last_disk_io.clone()
                };

                let (thermals, processes) = if full {
                    // Sysfs thermal zones first (GPU, CPU, SoC)
                    let mut thermals: Vec<crate::types::ThermalInfo> = Vec::new();
                    if let Ok(entries) = std::fs::read_dir("/sys/devices/virtual/thermal") {
                        for entry in entries.flatten() {
                            let path = entry.path();
                            if !path.file_name()
                                .and_then(|n| n.to_str())
                                .map_or(false, |n| n.starts_with("thermal_zone"))
                            {
                                continue;
                            }
                            let label = std::fs::read_to_string(path.join("type"))
                                .unwrap_or_default()
                                .trim()
                                .to_string();
                            let temp = std::fs::read_to_string(path.join("temp"))
                                .ok()
                                .and_then(|s| s.trim().parse::<f32>().ok())
                                .map(|t| t / 1000.0);
                            if let Some(temp_celsius) = temp {
                                thermals.push(crate::types::ThermalInfo {
                                    label,
                                    temp_celsius,
                                    critical_celsius: None,
                                });
                            }
                        }
                    }
                    // hwmon sensors via sysinfo
                    thermals.extend(self.components.iter().filter_map(|c| {
                        Some(crate::types::ThermalInfo {
                            label: c.label().to_string(),
                            temp_celsius: c.temperature()?,
                            critical_celsius: c.critical(),
                        })
                    }));

                    // Processes
                    let mut processes: Vec<crate::types::ProcessInfo> = self
                        .sys
                        .processes()
                        .iter()
                        .map(|(pid, process)| crate::types::ProcessInfo {
                            pid: pid.as_u32() as i32,
                            name: process.name().to_string_lossy().to_string(),
                            cpu_percent: process.cpu_usage(),
                            mem_bytes: process.memory(),
                        })
                        .collect();
                    processes.sort_by(|a, b| {
                        b.cpu_percent
                            .partial_cmp(&a.cpu_percent)
                            .unwrap_or(Ordering::Equal)
                    });

                    self.last_thermals = thermals.clone();
                    self.last_processes = processes.clone();
                    (thermals, processes)
                } else {
                    (self.last_thermals.clone(), self.last_processes.clone())
                };

                crate::types::SystemMetrics {
                    cpu,
                    ram,
                    swap,
                    network,
                    disk_io,
                    processes,
                    thermals,
                }
            }
        }
}

pub mod widgets {
        use ratatui::{
            Frame,
            layout::{Rect, Layout, Constraint},
            style::{Style, Color, Modifier},
            text::{Line, Span},
            widgets::{
                Block, Borders, Gauge, Paragraph, Row, Table, Cell, Sparkline,
            },
        };

        use crate::types::{
            CpuCoreUsage,
            RamSwapUsage,
            ProcessInfo,
            SortOrder,
            ThermalInfo,
        };

        pub trait Renderable {
            fn render(&self, area: Rect, f: &mut Frame);
        }

        /// Compact CPU widget: single average gauge + per-core summary text.
        pub struct CpuWidget {
            pub data: Vec<CpuCoreUsage>,
        }

        impl CpuWidget {
            pub fn new(data: Vec<CpuCoreUsage>) -> Self {
                Self { data }
            }
        }

        impl Renderable for CpuWidget {
            fn render(&self, area: Rect, f: &mut Frame) {
                if self.data.is_empty() {
                    return;
                }

                let avg = self.data.iter().map(|c| c.usage_percent).sum::<f32>()
                    / self.data.len() as f32;

                let chunks = Layout::vertical([
                    Constraint::Length(3), // gauge
                    Constraint::Min(1),    // per-core text
                ])
                .split(area);

                let gauge = Gauge::default()
                    .block(Block::default().borders(Borders::ALL).title(
                        format!("CPU ({} cores) avg {:.0}%", self.data.len(), avg),
                    ))
                    .gauge_style(Style::default().fg(Color::Green))
                    .percent(avg.min(100.0) as u16);
                f.render_widget(gauge, chunks[0]);

                let mut lines: Vec<Line> = Vec::new();
                let mut spans: Vec<Span> = Vec::new();
                for (i, core) in self.data.iter().enumerate() {
                    if i > 0 {
                        spans.push(Span::raw(" | "));
                    }
                    let color = if core.usage_percent > 80.0 {
                        Color::Red
                    } else if core.usage_percent > 40.0 {
                        Color::Yellow
                    } else {
                        Color::Green
                    };
                    spans.push(Span::styled(
                        format!("{:>2}:{:>3.0}%", core.core_id, core.usage_percent),
                        Style::default().fg(color),
                    ));
                    if (i + 1) % 4 == 0 {
                        lines.push(Line::from(std::mem::take(&mut spans)));
                    }
                }
                if !spans.is_empty() {
                    lines.push(Line::from(spans));
                }

                let para = Paragraph::new(lines)
                    .block(Block::default().borders(Borders::ALL));
                f.render_widget(para, chunks[1]);
            }
        }

        /// RAM usage gauge.
        pub struct RamGaugeWidget {
            pub data: RamSwapUsage,
        }

        impl RamGaugeWidget {
            pub fn new(data: RamSwapUsage) -> Self {
                Self { data }
            }
        }

        impl Renderable for RamGaugeWidget {
            fn render(&self, area: Rect, f: &mut Frame) {
                let percent = if self.data.total > 0 {
                    (self.data.used as f64 / self.data.total as f64 * 100.0) as u16
                } else {
                    0
                };
                let used_gib = self.data.used as f64 / (1024.0 * 1024.0 * 1024.0);
                let total_gib = self.data.total as f64 / (1024.0 * 1024.0 * 1024.0);
                let gauge = Gauge::default()
                    .block(Block::default().borders(Borders::ALL).title(
                        format!("RAM {:.1}/{:.1} GiB", used_gib, total_gib),
                    ))
                    .gauge_style(Style::default().fg(Color::Cyan))
                    .percent(percent);
                f.render_widget(gauge, area);
            }
        }

        /// Thermal sensors table with color-coded temperatures.
        pub struct ThermalWidget {
            pub data: Vec<ThermalInfo>,
        }

        impl ThermalWidget {
            pub fn new(data: Vec<ThermalInfo>) -> Self {
                Self { data }
            }
        }

        impl Renderable for ThermalWidget {
            fn render(&self, area: Rect, f: &mut Frame) {
                if self.data.is_empty() {
                    let block = Block::default().borders(Borders::ALL).title("Thermals");
                    let para = Paragraph::new("No sensors found")
                        .block(block);
                    f.render_widget(para, area);
                    return;
                }

                let rows: Vec<Row> = self
                    .data
                    .iter()
                    .map(|t| {
                        let color = if t.temp_celsius > 85.0 {
                            Color::Red
                        } else if t.temp_celsius > 65.0 {
                            Color::Yellow
                        } else {
                            Color::Green
                        };
                        let crit_str = match t.critical_celsius {
                            Some(c) => format!("/{:.0}°C", c),
                            None => String::new(),
                        };
                        Row::new(vec![
                            Cell::from(t.label.clone()),
                            Cell::from(format!("{:.1}°C{}", t.temp_celsius, crit_str))
                                .style(Style::default().fg(color)),
                        ])
                    })
                    .collect();

                let widths = [Constraint::Min(12), Constraint::Length(16)];
                let table = Table::new(rows, widths)
                    .block(Block::default().borders(Borders::ALL).title("Thermals"));

                f.render_widget(table, area);
            }
        }

        /// Network sparkline with RX/TX history.
        pub struct NetworkSparklineWidget {
            pub rx: Vec<u64>,
            pub tx: Vec<u64>,
        }

        impl NetworkSparklineWidget {
            pub fn new(rx: Vec<u64>, tx: Vec<u64>) -> Self {
                Self { rx, tx }
            }
        }

        impl Renderable for NetworkSparklineWidget {
            fn render(&self, area: Rect, f: &mut Frame) {
                let chunks = Layout::vertical([
                    Constraint::Percentage(50),
                    Constraint::Percentage(50),
                ]).split(area);

                let rx_spark = Sparkline::default()
                    .block(Block::default().borders(Borders::ALL).title("RX"))
                    .data(&self.rx)
                    .style(Style::default().fg(Color::Green));
                f.render_widget(rx_spark, chunks[0]);

                let tx_spark = Sparkline::default()
                    .block(Block::default().borders(Borders::ALL).title("TX"))
                    .data(&self.tx)
                    .style(Style::default().fg(Color::Yellow));
                f.render_widget(tx_spark, chunks[1]);
            }
        }

        /// Disk I/O sparklines with history.
        pub struct DiskIOSparkWidget {
            pub read: Vec<u64>,
            pub write: Vec<u64>,
        }

        impl DiskIOSparkWidget {
            pub fn new(read: Vec<u64>, write: Vec<u64>) -> Self {
                Self { read, write }
            }
        }

        impl Renderable for DiskIOSparkWidget {
            fn render(&self, area: Rect, f: &mut Frame) {
                let chunks = Layout::vertical([
                    Constraint::Percentage(50),
                    Constraint::Percentage(50),
                ]).split(area);

                let read_spark = Sparkline::default()
                    .block(Block::default().borders(Borders::ALL).title("Read"))
                    .data(&self.read)
                    .style(Style::default().fg(Color::Blue));
                f.render_widget(read_spark, chunks[0]);

                let write_spark = Sparkline::default()
                    .block(Block::default().borders(Borders::ALL).title("Write"))
                    .data(&self.write)
                    .style(Style::default().fg(Color::Magenta));
                f.render_widget(write_spark, chunks[1]);
            }
        }

        /// Process table.
        pub struct ProcessTableWidget {
            pub data: Vec<ProcessInfo>,
            pub sort_order: SortOrder,
        }

        impl ProcessTableWidget {
            pub fn new(data: Vec<ProcessInfo>, sort_order: SortOrder) -> Self {
                Self { data, sort_order }
            }
        }

        impl Renderable for ProcessTableWidget {
            fn render(&self, area: Rect, f: &mut Frame) {
                let mut sorted: Vec<&ProcessInfo> = self.data.iter().collect();
                match self.sort_order {
                    SortOrder::Cpu => sorted.sort_by(|a, b| {
                        b.cpu_percent
                            .partial_cmp(&a.cpu_percent)
                            .unwrap_or(std::cmp::Ordering::Equal)
                    }),
                    SortOrder::Mem => sorted.sort_by(|a, b| b.mem_bytes.cmp(&a.mem_bytes)),
                }

                let rows: Vec<Row> = sorted
                    .iter()
                    .map(|p| {
                        Row::new(vec![
                            Cell::from(p.pid.to_string()),
                            Cell::from(p.name.clone()),
                            Cell::from(format!("{:.1}%", p.cpu_percent)),
                            Cell::from(format!("{} MiB", p.mem_bytes / (1024 * 1024))),
                        ])
                    })
                    .collect();

                let widths = [
                    Constraint::Length(8),
                    Constraint::Min(20),
                    Constraint::Length(8),
                    Constraint::Length(10),
                ];
                let table = Table::new(rows, widths)
                    .header(
                        Row::new(vec!["PID", "Name", "CPU%", "MEM"])
                            .style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                    )
                    .block(Block::default().borders(Borders::ALL).title("Processes"));

                f.render_widget(table, area);
            }
        }

        /// Status bar showing scan rate, log status, and key hints.
        pub struct StatusBarWidget {
            pub process_every: u32,
            pub tick_ms: u32,
            pub snap_path: Option<String>,
            pub log_path: Option<String>,
        }

        impl StatusBarWidget {
            pub fn new(process_every: u32, tick_ms: u32, snap_path: Option<String>, log_path: Option<String>) -> Self {
                Self { process_every, tick_ms, snap_path, log_path }
            }
        }

        impl Renderable for StatusBarWidget {
            fn render(&self, area: Rect, f: &mut Frame) {
                let scan_ms = self.process_every * self.tick_ms;
                let scan_str = if scan_ms >= 1000 {
                    format!("{:.1}s", scan_ms as f32 / 1000.0)
                } else {
                    format!("{}ms", scan_ms)
                };

                let mut spans = vec![
                    Span::styled(" Proc scan: ", Style::default().fg(Color::DarkGray)),
                    Span::styled(scan_str, Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
                ];

                if let Some(ref p) = self.log_path {
                    spans.push(Span::styled(" | ", Style::default().fg(Color::DarkGray)));
                    spans.push(Span::styled(format!("REC: {}", p), Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)));
                }

                if let Some(ref p) = self.snap_path {
                    spans.push(Span::styled(" | ", Style::default().fg(Color::DarkGray)));
                    spans.push(Span::styled(format!("SNAP: {}", p), Style::default().fg(Color::Green)));
                }

                spans.push(Span::styled(" | ", Style::default().fg(Color::DarkGray)));
                spans.push(Span::styled("[/] scan rate  l:snap  Alt+l:log  c/m:sort  q:quit", Style::default().fg(Color::DarkGray)));

                let para = Paragraph::new(Line::from(spans));
                f.render_widget(para, area);
            }
        }
}

pub mod layout {
        use ratatui::layout::{Constraint, Direction, Layout, Rect};

        pub struct LayoutManager {
            pub cpu_area: Rect,
            pub ram_area: Rect,
            pub thermal_area: Rect,
            pub net_area: Rect,
            pub disk_area: Rect,
            pub proc_area: Rect,
            pub status_area: Rect,
        }

        impl LayoutManager {
            /// Layout:
            /// Top 35%:    [CPU 40% | RAM 25% | Thermals 35%]
            /// Middle 64%: [Network 20% | Disk 20% | Processes 60%]
            /// Bottom 1:   [Status bar]
            pub fn new(size: Rect) -> Self {
                let main_chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([
                        Constraint::Percentage(35),
                        Constraint::Min(1),
                        Constraint::Length(1),
                    ])
                    .split(size);

                let top_chunks = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([
                        Constraint::Percentage(40),
                        Constraint::Percentage(25),
                        Constraint::Percentage(35),
                    ])
                    .split(main_chunks[0]);

                let bottom_chunks = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([
                        Constraint::Percentage(20),
                        Constraint::Percentage(20),
                        Constraint::Percentage(60),
                    ])
                    .split(main_chunks[1]);

                Self {
                    cpu_area: top_chunks[0],
                    ram_area: top_chunks[1],
                    thermal_area: top_chunks[2],
                    net_area: bottom_chunks[0],
                    disk_area: bottom_chunks[1],
                    proc_area: bottom_chunks[2],
                    status_area: main_chunks[2],
                }
            }
        }
}

pub mod app {
        use std::io::Write;
        use crate::types::{
            DiskIOStats, NetworkStats, RamSwapUsage, SortOrder,
            SparklineHistory, SystemMetrics,
        };
        use crate::collector::Collector;
        use crate::layout::LayoutManager;
        use crate::widgets::{
            CpuWidget, DiskIOSparkWidget, NetworkSparklineWidget, ProcessTableWidget,
            RamGaugeWidget, ThermalWidget, StatusBarWidget, Renderable,
        };
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        use ratatui::Frame;

        pub struct AppState {
            pub metrics: SystemMetrics,
            pub sort_order: SortOrder,
            collector: Collector,
            history: SparklineHistory,
            log_dir: String,
            /// Briefly shows the last snapshot path, cleared after a few ticks.
            snap_path: Option<String>,
            snap_ttl: u32,
            /// Continuous logging (Alt+L toggle)
            log_writer: Option<std::io::BufWriter<std::fs::File>>,
            log_path: Option<String>,
        }

        /// Scan rate presets: ticks between process refreshes.
        /// With 250ms tick: 1=4/s, 2=2/s, 4=1/s, 8=0.5/s, 20=once per 5s
        const SCAN_PRESETS: &[u32] = &[1, 2, 4, 8, 20];

        impl AppState {
            pub fn new() -> Self {
                let log_dir = std::env::var("SYSMON_LOG_DIR")
                    .unwrap_or_else(|_| "/tmp/sysmon-tui".to_string());

                Self {
                    metrics: SystemMetrics {
                        cpu: Vec::new(),
                        ram: RamSwapUsage { used: 0, total: 0 },
                        swap: RamSwapUsage { used: 0, total: 0 },
                        network: NetworkStats {
                            received_bytes: 0,
                            transmitted_bytes: 0,
                        },
                        disk_io: DiskIOStats {
                            read_bytes: 0,
                            write_bytes: 0,
                        },
                        processes: Vec::new(),
                        thermals: Vec::new(),
                    },
                    sort_order: SortOrder::Cpu,
                    collector: Collector::new(),
                    history: SparklineHistory::new(120),
                    log_dir,
                    snap_path: None,
                    snap_ttl: 0,
                    log_writer: None,
                    log_path: None,
                }
            }

            pub fn update_metrics(&mut self) {
                self.metrics = self.collector.collect();
                self.history.push(&self.metrics.network, &self.metrics.disk_io);
                self.write_log();
                // Fade out snapshot notification
                if self.snap_ttl > 0 {
                    self.snap_ttl -= 1;
                    if self.snap_ttl == 0 {
                        self.snap_path = None;
                    }
                }
            }

            fn snapshot(&mut self) {
                let _ = std::fs::create_dir_all(&self.log_dir);
                let now = chrono::Local::now();
                let filename = format!("snap-{}.csv", now.format("%Y-%m-%d_%H-%M-%S"));
                let path = format!("{}/{}", self.log_dir, filename);
                if let Ok(file) = std::fs::File::create(&path) {
                    let mut w = std::io::BufWriter::new(file);
                    let _ = writeln!(w, "timestamp,pid,name,cpu_percent,mem_bytes");
                    let ts = now.format("%Y-%m-%dT%H:%M:%S%.3f");
                    for p in &self.metrics.processes {
                        let _ = writeln!(w, "{},{},{},{:.1},{}", ts, p.pid, p.name, p.cpu_percent, p.mem_bytes);
                    }
                    let _ = w.flush();
                    self.snap_path = Some(path);
                    self.snap_ttl = 12; // ~3 seconds at 250ms tick
                }
            }

            fn toggle_log(&mut self) {
                if self.log_writer.is_some() {
                    self.log_writer = None;
                    self.log_path = None;
                } else {
                    let _ = std::fs::create_dir_all(&self.log_dir);
                    let now = chrono::Local::now();
                    let filename = format!("sysmon-{}.csv", now.format("%Y-%m-%d_%H-%M-%S"));
                    let path = format!("{}/{}", self.log_dir, filename);
                    if let Ok(file) = std::fs::File::create(&path) {
                        let mut w = std::io::BufWriter::new(file);
                        let _ = writeln!(w, "timestamp,pid,name,cpu_percent,mem_bytes");
                        self.log_writer = Some(w);
                        self.log_path = Some(path);
                    }
                }
            }

            fn write_log(&mut self) {
                if let Some(ref mut writer) = self.log_writer {
                    let now = chrono::Local::now().format("%Y-%m-%dT%H:%M:%S%.3f");
                    for p in &self.metrics.processes {
                        let _ = writeln!(writer, "{},{},{},{:.1},{}", now, p.pid, p.name, p.cpu_percent, p.mem_bytes);
                    }
                    let _ = writer.flush();
                }
            }

            fn scan_faster(&mut self) {
                let cur = self.collector.process_every;
                for &p in SCAN_PRESETS.iter().rev() {
                    if p < cur {
                        self.collector.process_every = p;
                        return;
                    }
                }
            }

            fn scan_slower(&mut self) {
                let cur = self.collector.process_every;
                for &p in SCAN_PRESETS.iter() {
                    if p > cur {
                        self.collector.process_every = p;
                        return;
                    }
                }
            }

            pub fn handle_input(&mut self, key: KeyEvent) {
                match key.code {
                    KeyCode::Char('c') | KeyCode::Char('C') => self.sort_order = SortOrder::Cpu,
                    KeyCode::Char('m') | KeyCode::Char('M') => self.sort_order = SortOrder::Mem,
                    KeyCode::Char('l') if key.modifiers.contains(KeyModifiers::ALT) => self.toggle_log(),
                    KeyCode::Char('l') | KeyCode::Char('L') => self.snapshot(),
                    KeyCode::Char('[') => self.scan_faster(),
                    KeyCode::Char(']') => self.scan_slower(),
                    _ => {}
                }
            }

            pub fn render(&self, f: &mut Frame) {
                let size = f.area();
                let layout = LayoutManager::new(size);

                CpuWidget::new(self.metrics.cpu.clone())
                    .render(layout.cpu_area, f);

                RamGaugeWidget::new(self.metrics.ram.clone())
                    .render(layout.ram_area, f);

                ThermalWidget::new(self.metrics.thermals.clone())
                    .render(layout.thermal_area, f);

                NetworkSparklineWidget::new(
                    self.history.net_rx.iter().copied().collect(),
                    self.history.net_tx.iter().copied().collect(),
                ).render(layout.net_area, f);

                DiskIOSparkWidget::new(
                    self.history.disk_read.iter().copied().collect(),
                    self.history.disk_write.iter().copied().collect(),
                ).render(layout.disk_area, f);

                ProcessTableWidget::new(self.metrics.processes.clone(), self.sort_order)
                    .render(layout.proc_area, f);

                StatusBarWidget::new(
                    self.collector.process_every,
                    250,
                    self.snap_path.clone(),
                    self.log_path.clone(),
                ).render(layout.status_area, f);
            }
        }
}

pub mod main {
    use std::error::Error;
    use std::time::Duration;
    use std::sync::mpsc::channel;

    use crossterm::{
        execute,
        terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    };
    use ratatui::{backend::CrosstermBackend, Terminal};

    use crate::event::{AppEvent, EventHandler};
    use crate::app::AppState;

    pub fn main() -> Result<(), Box<dyn Error>> {
        enable_raw_mode()?;
        let mut stdout = std::io::stdout();
        execute!(stdout, EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;

        let (tx, rx) = channel::<AppEvent>();
        let event_handler = EventHandler::new(Duration::from_millis(250));
        event_handler.run(tx);

        let mut app = AppState::new();

        loop {
            terminal.draw(|f| {
                app.render(f);
            })?;

            match rx.recv() {
                Ok(event) => match event {
                    AppEvent::Tick => {
                        app.update_metrics();
                    }
                    AppEvent::Input(key) => {
                        if key.code == crossterm::event::KeyCode::Char('q') {
                            break;
                        } else {
                            app.handle_input(key);
                        }
                    }
                },
                Err(_) => break,
            }
        }

        disable_raw_mode()?;
        execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
        terminal.show_cursor()?;

        Ok(())
    }
}

fn main() {
    if let Err(e) = crate::main::main() {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}
