# sysmon-tui

A lightweight system monitor TUI built in Rust. Designed for monitoring GPU inference workloads on NVIDIA Jetson and similar Linux machines over SSH.

<img width="1282" height="673" alt="Screenshot 2026-02-10 at 12 27 23 PM" src="https://github.com/user-attachments/assets/c9cad83c-cf7b-4668-8a23-2c22b0fa4e36" />

## Features

- **CPU** — average gauge + per-core breakdown with color coding
- **RAM** — usage gauge with GiB readout
- **Thermals** — reads Linux sysfs thermal zones (GPU, CPU, SoC) and hwmon sensors, color-coded by severity
- **Network** — RX/TX sparklines with rolling history
- **Disk I/O** — read/write sparklines with rolling history
- **Processes** — sortable table (CPU or memory), updates at configurable rate
- **Snapshots** — press `l` to dump a CSV snapshot of all processes
- **Status bar** — shows current scan rate, snapshot path, and key hints

## Install

Requires Rust 1.70+.

```bash
cargo build --release
```

### Cross-compile for aarch64 Linux (e.g., Jetson)

If you're on an Apple Silicon Mac with Docker/OrbStack:

```bash
docker run --rm -v "$(pwd)":/app -w /app rust:latest cargo build --release
scp target/release/sysmon-tui user@host:~
ssh -t user@host ./sysmon-tui
```

The `-t` flag is required — the TUI needs a real terminal.

## Usage

```bash
./sysmon-tui
```

### Key Bindings

| Key | Action |
|-----|--------|
| `q` | Quit |
| `c` | Sort processes by CPU |
| `m` | Sort processes by memory |
| `[` | Scan processes faster (250ms/500ms/1s/2s/5s) |
| `]` | Scan processes slower |
| `l` | Save CSV snapshot of current processes |

### Process Snapshots

Press `l` to dump a snapshot. Creates a timestamped CSV file:

```
/tmp/sysmon-tui/sysmon-2026-02-10_05-15-30.csv
```

Format:
```csv
timestamp,pid,name,cpu_percent,mem_bytes
2026-02-10T05:15:30.123,150627,python3,407.5,26755072000
```

Override the log directory:
```bash
SYSMON_LOG_DIR=~/logs ./sysmon-tui
```

## Dependencies

| Crate | Purpose |
|-------|---------|
| ratatui 0.29 | TUI rendering |
| crossterm 0.28 | Terminal control |
| sysinfo 0.38 | CPU, memory, process, network metrics |
| chrono 0.4 | Log file timestamps |

## License

MIT
