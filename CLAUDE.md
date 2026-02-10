# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

A real-time system monitor TUI (like htop) built in Rust with ratatui 0.29, sysinfo 0.38, and crossterm 0.28. Displays CPU per-core bars, RAM/Swap gauges, network sparklines, disk I/O, and a sortable process table.

## Build & Run

```bash
cargo build            # debug build
cargo build --release  # release build
cargo run --release    # run the TUI
cargo check            # type-check only (fastest feedback loop)
cargo clippy           # lint
cargo fmt              # format
```

No tests exist yet. No feature flags.

## Architecture

Everything lives in `src/main.rs` with inline modules:

| Module | Purpose |
|--------|---------|
| `main` | Terminal setup (raw mode + alternate screen), event loop, teardown |
| `errors` | `SysmonError` enum (currently just `Io` variant) |
| `event` | `AppEvent` enum + `EventHandler` (spawns thread, polls crossterm) |
| `types` | Data structs: `CpuCoreUsage`, `RamSwapUsage`, `NetworkStats`, `DiskIOStats`, `ProcessInfo`, `SystemMetrics`, `SortOrder` |
| `collector` | `Collector` wraps `sysinfo::System` + `Networks`, produces `SystemMetrics` snapshots |
| `widgets` | `Renderable` trait + widget structs (CPU bars, RAM gauge, network sparkline, disk I/O, process table) |
| `layout` | `LayoutManager` splits screen into 5 areas (top: CPU+RAM, bottom: Net+Disk+Process) |
| `app` | `AppState` owns `Collector` + metrics + sort order, drives the update/render cycle |

**Data flow:** `EventHandler` thread → mpsc channel → main loop → `AppState::update_metrics()` (via `Collector`) → `AppState::render()` (via `LayoutManager` + widgets)

**Concurrency model:** Single MPSC channel decouples input polling from rendering. The event thread sends `Tick` (every 250ms) or `Input` events. Main thread owns all mutable state — no `Arc<Mutex<>>` needed.

## Key Bindings

- `q` — quit
- `c` — sort processes by CPU
- `m` — sort processes by memory

## sysinfo 0.38 API Notes

These were pain points during initial development. Keep in mind when modifying the collector:

- `refresh_all()` returns `()`, not `Result` — it's infallible
- CPU: `sys.cpus()` (not `processors()`)
- Memory: `used_memory()` / `total_memory()` return **bytes** — do NOT multiply by 1024
- Networks: separate `Networks` type, `Networks::new_with_refreshed_list()`, `refresh(bool)` where bool = remove disappeared interfaces
- Disk I/O: `Disk` has no `read_bytes`/`write_bytes` — aggregated from `Process::disk_usage()` instead
- Process: `name()` returns `&OsStr`, use `.to_string_lossy()`
- `memory()` returns bytes directly

## ratatui 0.29 API Notes

- `Frame::area()` not `Frame::size()` (deprecated)
- `Table::new(rows, widths)` — widths is second argument, not a builder method
- `Sparkline::data()` takes `&[u64]`, not `&[u16]`
- `Layout::split()` returns `Rc<[Rect]>` — use indexing, not array destructuring

## Known Limitations & Future Work

Per the code review report (`Rust TUI System Monitor Code Review.txt`):

1. **No sparkline history** — network widget only shows 2 current data points (RX/TX) instead of a time-series. Fix: add `VecDeque<u64>` ring buffers to `AppState` for RX/TX history.
2. **`refresh_all()` is heavyweight** — refreshes process list, temps, users every tick. Fix: use `refresh_specifics()` with `RefreshKind` for granular control; refresh processes less often (every 4th tick).
3. **Sorting in render loop** — `ProcessTableWidget` re-sorts every frame even if data hasn't changed. Fix: pre-sort in `update_metrics()`.
4. **No panic hook** — a panic in the render loop leaves the terminal in raw mode. Fix: install a panic hook that restores the terminal.
5. **No search** — the process list is sortable but not searchable. Fix: add `search_query: String` to `AppState`, filter in `handle_input`.
6. **Custom `Renderable` trait** — works but prevents interop with ratatui's `Widget`/`WidgetRef` ecosystem.

## Reference

- `WORKING_MEMORY.md` — cross-module notes on type signatures and API contracts used during generation
- `Rust TUI System Monitor Code Review.txt` — detailed architectural analysis and optimization roadmap
