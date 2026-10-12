# ElecBake

An AI copilot for electronic circuit design, starting with Altium Designer. Written in Rust.
Scope is circuit design: reading and reviewing schematics, part selection, calculations,
simulation, and board advice. PCB routing is out of scope.

The design spec and implementation plans are kept locally under `docs/superpowers/`,
which git ignores. Read them there if present.

## Commits

- Commit as the repository owner only. Never add `Co-Authored-By` lines, "Generated with"
  lines, or any other AI or model attribution to commit messages or pull requests.
- `docs/superpowers/` (specs and plans) is ignored by git. Those files stay local; do not
  force-add them.
- Commit or push only when asked. Nothing is pushed to GitHub without an explicit request.

## Layout

```
crates/elecbake-core/   library: Altium loading, netlist, design model
  src/altium/           the only code that imports altium-format
  src/schematic.rs      ElecBake's own drawn-schematic types
  src/netlist/          connectivity inside a sheet, across sheets, diagnostics
  src/model.rs          Design: what the viewer, MCP tools and chat will read
  src/design.rs         load_design(path) for .PrjPcb and .SchDoc
  src/compare.rs        check against a Protel netlist exported from Altium
crates/elecbake/        the `elecbake` command-line program
fixtures/altiumsharp/   open sample schematics used by tests
docs/                   engine gaps, third-party record
```

## Commands

```
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all
cargo run -p elecbake -- netlist "fixtures/altiumsharp/Overview.SchDoc"
cargo run -p elecbake -- render "fixtures/altiumsharp/Overview.SchDoc" -o out/
```

## Rules for the code

- Only `crates/elecbake-core/src/altium/` may import `altium_format`, `altium_format_types`
  or `altium_format_render_svg`. Everything else uses ElecBake's own types. `altium-format`
  is the file engine for now and will be replaced by ElecBake's own engine later.
- When `altium-format` lacks something, implement it in ElecBake and add a line to
  `docs/altium-engine-gaps.md`.
- `altium-format` is pinned to a git commit in `Cargo.toml`. Its enums are
  `#[non_exhaustive]`, so every `match` on one needs a `_` arm.
- Altium files are read-only for now. Never modify a user's Altium file.
- Coordinates are Altium raw units as `i64` (10,000 units = 1 mil).
- Net names are matched ignoring letter case (`netlist::net_key`).
- Write the test first and watch it fail before writing the code.
- The AI connection is built on MCP (Model Context Protocol); later features are added as
  MCP tools or servers rather than hard-wired into the chat.

## Licensing and third-party code

The license for ElecBake has not been chosen; this is deliberate. Code from other projects,
including GPL ones, may be copied and adapted. Every time that happens, record the source,
commit and license in `docs/third-party-code.md`. Do not add a LICENSE file or `license`
fields until the owner decides.

## Testing with real designs

Private Altium designs must not be committed. Only openly licensed files go in `fixtures/`.
The real correctness check is `elecbake netlist <project> --compare <file.NET>` against a
Protel netlist exported from Altium (Design » Netlist For Project » Protel).
