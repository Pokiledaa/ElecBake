# ElecBake

An AI copilot for electronic circuit design, starting with Altium Designer.

This repository is at an early stage. What works today is the schematic engine:
it opens an Altium project or schematic, derives the netlist, and renders sheets.

## Build

Requires Rust 1.85 or newer.

    cargo build --release

## Credits

Altium file reading is provided by
[altium-format](https://github.com/akiselev/altium-cli) (Apache-2.0).
Sample schematics under `fixtures/altiumsharp/` come from
[AltiumSharp](https://github.com/issus/AltiumSharp) (Apache-2.0).

The license for ElecBake itself has not been chosen yet.
