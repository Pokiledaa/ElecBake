# ElecBake

An AI copilot for electronic circuit design, starting with Altium Designer.

This repository is at an early stage. What works today is the schematic engine:
it opens an Altium project or schematic, derives the netlist, and renders sheets.
Altium Designer is not needed.

## Build

Requires Rust 1.85 or newer.

    cargo build --release

## Usage

Print the netlist of a project or a schematic (child sheets are followed):

    elecbake netlist MyBoard.PrjPcb
    elecbake netlist Top.SchDoc --format json

Render every sheet to SVG:

    elecbake render MyBoard.PrjPcb -o out/

Net names come from power ports, net labels and ports. A net with no name is
called `N$<designator>_<pin>`. Buses and signal harnesses are not followed yet;
affected nets are listed under "Diagnostics".

## Credits

Altium file reading is provided by
[altium-format](https://github.com/akiselev/altium-cli) (Apache-2.0).
Sample schematics under `fixtures/altiumsharp/` come from
[AltiumSharp](https://github.com/issus/AltiumSharp) (Apache-2.0).

The license for ElecBake itself has not been chosen yet.
