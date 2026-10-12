# Gaps in altium-format, and where ElecBake fills them

`altium-format` is ElecBake's file engine for now. This list records what it does not
provide, so that it can serve as the requirements for ElecBake's own engine later.
Everything ElecBake adds lives in `crates/elecbake-core/src/altium/` unless noted.

| Gap | What ElecBake does |
|---|---|
| No schematic netlist: the file gives drawn objects, not connections | Built in `elecbake-core::netlist` |
| The pin's electrical end is not given, only its body-side point, length and orientation | Computed in `altium::component` |
| Pins of every part of a multi-part component are listed on each placement | Filtered by `owner_part_id` against `current_part_id` |
| Pins are not filtered by display mode (alternate symbol views) | Not handled yet |
| A "Comment" such as `=Part Number` is returned unresolved | Resolved against the component's parameters |
| `footprints()` is empty for every component in the AltiumSharp sample files | `footprint` stays empty; not yet investigated |
| `part_count` is one more than the real number of parts | Not used; parts are told apart by `current_part_id` |
| Port and sheet-entry connection points are not given | Computed from location, width, style, side and distance |
| Document paths in `.PrjPcb` are Windows-style and case-insensitive | `altium::resolve` |
| Rendered SVG carries no component or net identifiers | To be solved by an overlay (plan 2) |
| Buses, bus entries and signal harnesses carry no expanded connectivity | Not expanded; reported as diagnostics |
| `AltiumProject::open` reads the project file as UTF-8, but Altium writes the Windows code page | A non-UTF-8 file is decoded as Windows-1252 into a scratch copy and opened from there |
| One unknown option value (for example `HierarchyMode=7`) makes the typed project view fail | The plain document list is used with the automatic net scope, and a diagnostic is raised |
| Off-sheet connectors are exposed as power objects with a flag | Kept apart from power nets; joined across sheets in flat and global scope only, diagnosed in hierarchical scope |
| `Repeat(...)` channel sheet symbols are not expanded | Placed once and diagnosed |
