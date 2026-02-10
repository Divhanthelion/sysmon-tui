# WORKING MEMORY

Cross-module knowledge base. Each module leaves notes for modules that depend on it.

## How to Read This File
When implementing a module, find the sections for your dependencies and pay attention to:
- Method signatures (especially return types: Option vs Result, &T vs T)
- Trait implementations you can rely on (FromStr, Clone, etc.)
- Gotchas and non-obvious patterns

## How Notes Are Structured
Each module section contains:
- **Key Types**: The main structs/enums and their purpose
- **Critical Signatures**: Method signatures that are easy to get wrong
- **Trait Impls**: What traits are implemented (use these!)
- **Gotchas**: Things that will break your code if you assume wrong

---

## ui::event

**Trait implementations:**
- `none`

**Key signatures:**
- ``pub fn new(tick_rate: std::time::Duration) -> Self``
- ``pub fn run(&self`
- `tx: std::sync::mpsc::Sender<AppEvent>)``

## core::types

**Notes for dependents:**
- `CpuCoreUsage` contains a core identifier and its usage percentage.
- `RamSwapUsage` tracks used and total bytes for RAM or swap.
- `NetworkStats` aggregates received and transmitted byte counts.
- `DiskIOStats` aggregates read and written byte counts.
- `ProcessInfo` holds a process ID, name, CPU usage percentage, and memory usage in bytes.
- `SortOrder` enum provides two variants: `Cpu` and `Mem`.
- `SystemMetrics` aggregates all the above metrics into a single snapshot.

**Key signatures:**
- `None`

## ui::layout

**Notes for dependents:**
- `LayoutManager` contains five public `Rect` fields for widget placement.
- The layout splits the screen into a top half (CPU & RAM) and bottom half (Network, Disk, Process).
- `new` takes a `ratatui::layout::Rect` and returns a fully populated `LayoutManager`.

**Key signatures:**
- ``pub fn new(size: Rect) -> Self``

