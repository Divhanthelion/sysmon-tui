//! A high‑performance resource monitor TUI application using ratatui, sysinfo, and crossterm. Features real‑time CPU core usage bars, RAM/Swap gauges, network throughput sparklines, disk I/O monitoring, and a searchable/sortable process list.

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

    /// Entry point of the application.
    ///
    /// Initializes the terminal, spawns an event handler thread,
    /// creates the UI state and runs a simple event loop that
    /// updates metrics on ticks, handles key input (q to quit)
    /// and renders the UI each iteration.
    ///
    /// # Errors
    ///
    /// Returns an error if terminal initialization or rendering fails.
    pub fn main() -> Result<(), Box<dyn Error>> {
        // ---------- Terminal setup ----------
        enable_raw_mode()?;
        let mut stdout = std::io::stdout();
        execute!(stdout, EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;

        // ---------- Event channel ----------
        let (tx, rx) = channel::<AppEvent>();

        // ---------- Event handler ----------
        let event_handler = EventHandler::new(Duration::from_millis(250));
        // The handler spawns its own thread internally.
        event_handler.run(tx);

        // ---------- Application state ----------
        let mut app = AppState::new();

        // ---------- Main loop ----------
        loop {
            // Render the UI
            terminal.draw(|f| {
                app.render(f);
            })?;

            // Handle incoming events
            match rx.recv() {
                Ok(event) => match event {
                    AppEvent::Tick => {
                        app.update_metrics();
                    }
                    AppEvent::Input(key) => {
                        // Quit on 'q'
                        if key.code == crossterm::event::KeyCode::Char('q') {
                            break;
                        } else {
                            app.handle_input(key);
                        }
                    }
                },
                Err(_) => break, // channel disconnected
            }
        }

        // ---------- Restore terminal ----------
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

pub mod errors {
        use std::io;
        use sysinfo;

        /// Unified error type for sysmon.
        #[derive(Debug)]
        pub enum SysmonError {
            Io(io::Error),
            SysInfo(sysinfo::SystemTimeError),
        }

        impl std::error::Error for SysmonError {}
}

pub mod event {
        use std::sync::mpsc::Sender;
        use std::time::Duration;

        use crossterm::event::{self, Event as CEvent, KeyEvent};

        /// Events that the application can receive.
        pub enum AppEvent {
            Tick,
            Input(KeyEvent),
        }

        /// Handles polling for user input and timer ticks.
        ///
        /// The handler repeatedly polls the terminal for key events with a timeout
        /// equal to `tick_rate`. When a key event is detected it sends an
        /// `AppEvent::Input` through the provided channel. When the poll times out
        /// it sends an `AppEvent::Tick`. The loop runs forever; callers should drop
        /// the channel or terminate the process to stop it.
        pub struct EventHandler {
            /// How often a tick event should be sent (in milliseconds).
            pub tick_rate: Duration,
        }

        impl EventHandler {
            /// Create a new `EventHandler` with the given tick rate.
            ///
            /// # Arguments
            ///
            /// * `tick_rate` - The duration between tick events.
            pub fn new(tick_rate: Duration) -> Self {
                EventHandler { tick_rate }
            }

            /// Run the event loop, sending events through `tx`.
            ///
            /// This method blocks forever. It polls for key events with a timeout
            /// of `self.tick_rate`. On a key event it sends `AppEvent::Input`,
            /// otherwise on timeout it sends `AppEvent::Tick`. Errors from the
            /// channel or terminal are ignored.
            pub fn run(&self, tx: Sender<AppEvent>) {
                loop {
                    // Poll for an event with the configured timeout.
                    if event::poll(self.tick_rate).unwrap_or(false) {
                        // An event is available; read it.
                        match event::read().unwrap_or(CEvent::Key(KeyEvent::from_raw(0, 0))) {
                            CEvent::Key(key) => {
                                let _ = tx.send(AppEvent::Input(key));
                            }
                            // Ignore other event types.
                            _ => {}
                        }
                    } else {
                        // Timeout elapsed; send a tick event.
                        let _ = tx.send(AppEvent::Tick);
                    }
                }
            }
        }
}

pub mod types {
        /// CPU usage for a single core.
        pub struct CpuCoreUsage {
            pub core_id: usize,
            pub usage_percent: f32,
        }

        /// RAM or swap usage in bytes.
        pub struct RamSwapUsage {
            pub used: u64,
            pub total: u64,
        }

        /// Cumulative network statistics.
        pub struct NetworkStats {
            pub received_bytes: u64,
            pub transmitted_bytes: u64,
        }

        /// Cumulative disk I/O statistics.
        pub struct DiskIOStats {
            pub read_bytes: u64,
            pub write_bytes: u64,
        }

        /// Snapshot of a single process.
        pub struct ProcessInfo {
            pub pid: i32,
            pub name: String,
            pub cpu_percent: f32,
            pub mem_bytes: u64,
        }

        /// Sorting criteria for the process list.
        pub enum SortOrder {
            Cpu,
            Mem,
        }

        /// Aggregated snapshot of all monitored metrics.
        pub struct SystemMetrics {
            pub cpu: Vec<CpuCoreUsage>,
            pub ram: RamSwapUsage,
            pub swap: RamSwapUsage,
            pub network: NetworkStats,
            pub disk_io: DiskIOStats,
            pub processes: Vec<ProcessInfo>,
        }
}

pub mod collector {
        use std::cmp::Ordering;
        use sysinfo::{
            DiskExt, NetworkExt, ProcessExt, ProcessorExt, System, SystemExt,
        };

        /// Collects metrics from the underlying OS.
        pub struct Collector {
            sys: System,
        }

        impl Collector {
            /// Creates a new collector instance.
            pub fn new() -> Self {
                let mut sys = System::new_all();
                // Initial refresh to populate data
                let _ = sys.refresh_all();
                Self { sys }
            }

            /// Collects a snapshot of system metrics.
            ///
            /// Returns `Ok(SystemMetrics)` on success or a `SysmonError` if the
            /// underlying sysinfo library fails to refresh data.
            pub fn collect(
                &mut self,
            ) -> Result<crate::core::types::SystemMetrics, crate::core::errors::SysmonError> {
                // Refresh all data; map sysinfo errors to our error type.
                match self.sys.refresh_all() {
                    Ok(_) => {}
                    Err(e) => return Err(crate::core::errors::SysmonError::SysInfo(e)),
                }

                // CPU core usage
                let cpu = self
                    .sys
                    .processors()
                    .iter()
                    .enumerate()
                    .map(|(idx, proc)| crate::core::types::CpuCoreUsage {
                        core_id: idx,
                        usage_percent: proc.cpu_usage(),
                    })
                    .collect::<Vec<_>>();

                // RAM usage
                let ram = crate::core::types::RamSwapUsage {
                    used: self.sys.used_memory() * 1024,
                    total: self.sys.total_memory() * 1024,
                };

                // Swap usage
                let swap = crate::core::types::RamSwapUsage {
                    used: self.sys.used_swap() * 1024,
                    total: self.sys.total_swap() * 1024,
                };

                // Network statistics
                let mut net_recv = 0u64;
                let mut net_trans = 0u64;
                for (_name, data) in self.sys.networks() {
                    net_recv += data.received();
                    net_trans += data.transmitted();
                }
                let network = crate::core::types::NetworkStats {
                    received_bytes: net_recv,
                    transmitted_bytes: net_trans,
                };

                // Disk I/O statistics
                let mut disk_read = 0u64;
                let mut disk_write = 0u64;
                for disk in self.sys.disks() {
                    disk_read += disk.read_bytes();
                    disk_write += disk.write_bytes();
                }
                let disk_io = crate::core::types::DiskIOStats {
                    read_bytes: disk_read,
                    write_bytes: disk_write,
                };

                // Process information
                let mut processes = Vec::new();
                for (pid, process) in self.sys.processes() {
                    processes.push(crate::core::types::ProcessInfo {
                        pid: pid.as_u32() as i32,
                        name: process.name().to_string(),
                        cpu_percent: process.cpu_usage(),
                        mem_bytes: process.memory() * 1024,
                    });
                }
                // Sort processes by CPU usage descending
                processes.sort_by(|a, b| {
                    b.cpu_percent
                        .partial_cmp(&a.cpu_percent)
                        .unwrap_or(Ordering::Equal)
                });

                Ok(crate::core::types::SystemMetrics {
                    cpu,
                    ram,
                    swap,
                    network,
                    disk_io,
                    processes,
                })
            }
        }
}

pub mod widgets {
        use ratatui::{
            Frame,
            layout::{Rect, Layout, Constraint},
            style::{Style, Color, Modifier},
            widgets::{
                Block, Borders, Gauge, Row, Table, Cell, Sparkline,
            },
        };

        use crate::core::types::{
            CpuCoreUsage, RamSwapUsage, DiskIOStats,
            ProcessInfo, SortOrder,
        };

        /// Trait for widgets that can render themselves into a `ratatui::Frame`.
        pub trait Renderable {
            fn render(&self, area: Rect, f: &mut Frame);
        }

        /// Widget that displays a bar per CPU core.
        pub struct CpuBarWidget {
            pub data: Vec<CpuCoreUsage>,
        }

        impl CpuBarWidget {
            /// Create a new `CpuBarWidget` with the given core usage data.
            pub fn new(data: Vec<CpuCoreUsage>) -> Self {
                Self { data }
            }
        }

        impl Renderable for CpuBarWidget {
            fn render(&self, area: Rect, f: &mut Frame) {
                if self.data.is_empty() {
                    return;
                }
                let chunks = Layout::vertical(
                    self.data.iter().map(|_| Constraint::Length(1)).collect::<Vec<_>>(),
                )
                .split(area);

                for (i, core) in self.data.iter().enumerate() {
                    let gauge = Gauge::default()
                        .block(
                            Block::default()
                                .borders(Borders::ALL)
                                .title(format!("Core {}", core.core_id)),
                        )
                        .gauge_style(Style::default().fg(Color::Green))
                        .percent(core.usage_percent as u16);
                    f.render_widget(gauge, chunks[i]);
                }
            }
        }

        /// Widget that displays RAM usage as a gauge.
        pub struct RamGaugeWidget {
            pub data: RamSwapUsage,
        }

        impl RamGaugeWidget {
            /// Create a new `RamGaugeWidget` with the given RAM usage data.
            pub fn new(data: RamSwapUsage) -> Self {
                Self { data }
            }
        }

        impl Renderable for RamGaugeWidget {
            fn render(&self, area: Rect, f: &mut Frame) {
                let percent = if self.data.total > 0 {
                    (self.data.used as f32 / self.data.total as f32 * 100.0) as u16
                } else {
                    0
                };
                let gauge = Gauge::default()
                    .block(Block::default().borders(Borders::ALL).title("RAM"))
                    .gauge_style(Style::default().fg(Color::Cyan))
                    .percent(percent);
                f.render_widget(gauge, area);
            }
        }

        /// Widget that displays a sparkline of network traffic.
        pub struct NetworkSparklineWidget {
            pub data: Vec<u64>,
        }

        impl NetworkSparklineWidget {
            /// Create a new `NetworkSparklineWidget` with the given data.
            pub fn new(data: Vec<u64>) -> Self {
                Self { data }
            }
        }

        impl Renderable for NetworkSparklineWidget {
            fn render(&self, area: Rect, f: &mut Frame) {
                let data_u16 = self
                    .data
                    .iter()
                    .map(|x| (*x as u16).min(u16::MAX))
                    .collect::<Vec<u16>>();
                let sparkline = Sparkline::default()
                    .block(Block::default().borders(Borders::ALL).title("Network"))
                    .data(&data_u16)
                    .style(Style::default().fg(Color::Yellow));
                f.render_widget(sparkline, area);
            }
        }

        /// Widget that displays disk I/O as two gauges.
        pub struct DiskIOBarWidget {
            pub data: DiskIOStats,
        }

        impl DiskIOBarWidget {
            /// Create a new `DiskIOBarWidget` with the given I/O statistics.
            pub fn new(data: DiskIOStats) -> Self {
                Self { data }
            }
        }

        impl Renderable for DiskIOBarWidget {
            fn render(&self, area: Rect, f: &mut Frame) {
                let total = self.data.read_bytes + self.data.write_bytes;
                let read_percent = if total > 0 {
                    (self.data.read_bytes as f32 / total as f32 * 100.0) as u16
                } else {
                    0
                };
                let write_percent = if total > 0 {
                    (self.data.write_bytes as f32 / total as f32 * 100.0) as u16
                } else {
                    0
                };

                let chunks = Layout::horizontal([
                    Constraint::Percentage(50),
                    Constraint::Percentage(50),
                ])
                .split(area);

                let read_gauge = Gauge::default()
                    .block(Block::default().borders(Borders::ALL).title("Read"))
                    .gauge_style(Style::default().fg(Color::Blue))
                    .percent(read_percent);
                f.render_widget(read_gauge, chunks[0]);

                let write_gauge = Gauge::default()
                    .block(Block::default().borders(Borders::ALL).title("Write"))
                    .gauge_style(Style::default().fg(Color::Magenta))
                    .percent(write_percent);
                f.render_widget(write_gauge, chunks[1]);
            }
        }

        /// Widget that displays a table of processes.
        pub struct ProcessTableWidget {
            pub data: Vec<ProcessInfo>,
            pub sort_order: SortOrder,
        }

        impl ProcessTableWidget {
            /// Create a new `ProcessTableWidget` with the given process data and sort order.
            pub fn new(data: Vec<ProcessInfo>, sort_order: SortOrder) -> Self {
                Self { data, sort_order }
            }
        }

        impl Renderable for ProcessTableWidget {
            fn render(&self, area: Rect, f: &mut Frame) {
                // Build a sorted slice of references to avoid cloning ProcessInfo.
                let mut sorted: Vec<&ProcessInfo> = self.data.iter().collect();
                match self.sort_order {
                    SortOrder::Cpu => sorted.sort_by(|a, b| {
                        b.cpu_percent
                            .partial_cmp(&a.cpu_percent)
                            .unwrap_or(std::cmp::Ordering::Equal)
                    }),
                    SortOrder::Mem => sorted.sort_by(|a, b| b.mem_bytes.cmp(&a.mem_bytes)),
                }

                let rows = sorted
                    .iter()
                    .map(|p| {
                        Row::new(vec![
                            Cell::from(p.pid.to_string()),
                            Cell::from(&p.name),
                            Cell::from(format!("{:.1}%", p.cpu_percent)),
                            Cell::from(format!("{} MiB", p.mem_bytes / (1024 * 1024))),
                        ])
                    })
                    .collect::<Vec<Row>>();

                let table = Table::new(rows)
                    .header(
                        Row::new(vec!["PID", "Name", "CPU%", "MEM"])
                            .style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                    )
                    .block(Block::default().borders(Borders::ALL).title("Processes"))
                    .widths(&[
                        Constraint::Length(6),
                        Constraint::Min(20),
                        Constraint::Length(8),
                        Constraint::Length(10),
                    ]);

                f.render_widget(table, area);
            }
        }
}

pub mod layout {
        use ratatui::layout::{Constraint, Direction, Layout, Rect};

        /// Holds the rectangular areas for each widget on the screen.
        pub struct LayoutManager {
            pub cpu_area: Rect,
            pub ram_area: Rect,
            pub net_area: Rect,
            pub disk_area: Rect,
            pub proc_area: Rect,
        }

        impl LayoutManager {
            /// Creates a new `LayoutManager` by dividing the given screen size into
            /// five widget areas:
            ///
            /// * The top half is split horizontally into `cpu_area` and `ram_area`.
            /// * The bottom half is split horizontally into `net_area`, `disk_area`,
            ///   and `proc_area` (roughly 33%/33%/34%).
            ///
            /// # Arguments
            ///
            /// * `size` – The total screen area to layout.
            pub fn new(size: Rect) -> Self {
                // Split the screen vertically into top and bottom halves.
                let [top, bottom] = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                    .split(size);

                // Split the top half horizontally into CPU and RAM areas.
                let [cpu_area, ram_area] = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                    .split(top);

                // Split the bottom half horizontally into Network, Disk, and Process areas.
                let [net_area, disk_area, proc_area] = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([
                        Constraint::Percentage(33),
                        Constraint::Percentage(33),
                        Constraint::Percentage(34),
                    ])
                    .split(bottom);

                Self {
                    cpu_area,
                    ram_area,
                    net_area,
                    disk_area,
                    proc_area,
                }
            }
        }
}

pub mod app {
        use crate::core::errors::SysmonError;
        use crate::core::types::{
            CpuCoreUsage, DiskIOStats, NetworkStats, ProcessInfo, RamSwapUsage, SortOrder,
            SystemMetrics,
        };
        use crate::data::collector::Collector;
        use crate::ui::layout::LayoutManager;
        use crate::ui::widgets::{
            CpuBarWidget, DiskIOBarWidget, NetworkSparklineWidget, ProcessTableWidget,
            RamGaugeWidget,
        };
        use crossterm::event::{KeyCode, KeyEvent};
        
        use ratatui::Frame;

        /// Main application state and rendering loop.
        pub struct AppState {
            pub metrics: SystemMetrics,
            pub sort_order: SortOrder,
        }

        impl AppState {
            /// Create a new application state with empty metrics and default sort order.
            pub fn new() -> Self {
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
                    },
                    sort_order: SortOrder::Cpu,
                }
            }

            /// Update the metrics snapshot using the provided collector.
            ///
            /// The collector is expected to expose a `collect` method that returns
            /// `Result<SystemMetrics, SysmonError>`.  The returned metrics replace the
            /// current snapshot.
            pub fn update_metrics(
                &mut self,
                collector: &mut Collector,
            ) -> Result<(), SysmonError> {
                // The exact signature of `Collector::collect` is not specified in the
                // dependency notes, but we assume it returns a Result with the same error
                // type as `SysmonError`.  If the signature differs, this method will need
                // to be adjusted accordingly.
                let new_metrics = collector.collect()?;
                self.metrics = new_metrics;
                Ok(())
            }

            /// Handle a key event from the user.
            ///
            /// Currently only two keys are recognised:
            /// * `c` – switch to CPU sort order
            /// * `m` – switch to memory sort order
            pub fn handle_input(&mut self, key: KeyEvent) {
                match key.code {
                    KeyCode::Char('c') | KeyCode::Char('C') => self.sort_order = SortOrder::Cpu,
                    KeyCode::Char('m') | KeyCode::Char('M') => self.sort_order = SortOrder::Mem,
                    _ => {}
                }
            }

            /// Render the current state to the provided frame.
            ///
            /// The layout is calculated from the frame's size, and each widget receives
            /// a clone of the relevant data slice.
            pub fn render(&self, f: &mut Frame) {
                let size = f.size();
                let layout = LayoutManager::new(size);

                // CPU usage bar
                CpuBarWidget::new(self.metrics.cpu.clone())
                    .render(layout.cpu_area, f);

                // RAM usage gauge
                RamGaugeWidget::new(self.metrics.ram.clone())
                    .render(layout.ram_area, f);

                // Network sparkline (received + transmitted)
                let net_data = vec![
                    self.metrics.network.received_bytes,
                    self.metrics.network.transmitted_bytes,
                ];
                NetworkSparklineWidget::new(net_data)
                    .render(layout.net_area, f);

                // Disk I/O bar
                DiskIOBarWidget::new(self.metrics.disk_io.clone())
                    .render(layout.disk_area, f);

                // Process table
                ProcessTableWidget::new(self.metrics.processes.clone(), self.sort_order)
                    .render(layout.proc_area, f);
            }
        }
}