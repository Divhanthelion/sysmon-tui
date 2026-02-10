//! A high‑performance resource monitor TUI application using ratatui, sysinfo, and crossterm. Features real‑time CPU core usage bars, RAM/Swap gauges, network throughput sparklines, disk I/O monitoring, and a searchable/sortable process list.

pub mod main {
    //! Entry point: initializes terminal, runs the event loop.
    todo!()
}

fn main() {
    println!("Starting application...");
    todo!("Wire up application entry point")
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
            /// A periodic tick event.
            Tick,
            /// A key press event from the user.
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
