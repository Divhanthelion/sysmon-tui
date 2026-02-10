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
