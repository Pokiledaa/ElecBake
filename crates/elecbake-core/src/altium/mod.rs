//! The only module that imports `altium-format`. It converts that crate's types into
//! ElecBake's own, so the file engine can be replaced later without touching anything else.
//! Features `altium-format` lacks are implemented here and listed in docs/altium-engine-gaps.md.

use std::path::{Path, PathBuf};

use altium_format::api::{SchDocComponent, SheetObject, SheetSymbol};
use altium_format_types::common::RotationBy90;
use altium_format_types::coord::CoordPoint;
use altium_format_types::project::FlattenMode;
use altium_format_types::sch::{LeftRightSide, PinElectricalType, PortArrowStyle};

use crate::error::{Error, Result};
use crate::geom::{Point, Rect};
use crate::schematic::{
    PinKind, RawComponent, RawEntry, RawLabel, RawPin, RawPort, RawPower, RawSheet, RawSheetSymbol,
    RawWire,
};

/// How a project says nets connect across sheets (Altium's "net identifier scope").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectHierarchy {
    Automatic,
    Flat,
    Hierarchical,
    StrictHierarchical,
    Global,
}

#[derive(Debug, Clone)]
pub struct ProjectFile {
    pub name: String,
    pub schematics: Vec<PathBuf>,
    pub other_documents: Vec<PathBuf>,
    /// Document paths listed in the project that were not found on disk.
    pub missing: Vec<String>,
    pub hierarchy: ProjectHierarchy,
}

fn failure(path: &Path, error: impl std::fmt::Display) -> Error {
    Error::Altium {
        path: path.to_path_buf(),
        message: error.to_string(),
    }
}

fn point(p: CoordPoint) -> Point {
    Point::new(p.x.raw() as i64, p.y.raw() as i64)
}

fn step(orientation: RotationBy90) -> (i64, i64) {
    match orientation {
        RotationBy90::Rotate0 => (1, 0),
        RotationBy90::Rotate90 => (0, 1),
        RotationBy90::Rotate180 => (-1, 0),
        _ => (0, -1),
    }
}

fn pin_kind(kind: PinElectricalType) -> PinKind {
    match kind {
        PinElectricalType::Input => PinKind::Input,
        PinElectricalType::InputOutput => PinKind::InputOutput,
        PinElectricalType::Output => PinKind::Output,
        PinElectricalType::OpenCollector => PinKind::OpenCollector,
        PinElectricalType::HiZ => PinKind::HiZ,
        PinElectricalType::OpenEmitter => PinKind::OpenEmitter,
        PinElectricalType::Power => PinKind::Power,
        _ => PinKind::Passive,
    }
}

fn component(c: &SchDocComponent) -> RawComponent {
    let parameters: Vec<(String, String)> = c
        .parameters()
        .iter()
        .map(|p| (p.name.clone(), p.text.clone()))
        .collect();
    let lookup = |name: &str| {
        parameters
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.clone())
    };
    // A comment such as "=Part Number" means "use the Part Number parameter".
    let comment = lookup("Comment")
        .and_then(|text| match text.strip_prefix('=') {
            Some(other) => lookup(other.trim()),
            None => Some(text),
        })
        .filter(|text| !text.trim().is_empty());

    let footprints = c.footprints();
    let footprint = footprints
        .iter()
        .find(|f| f.is_current)
        .or(footprints.first())
        .map(|f| f.model_name.clone());

    let pins = c
        .pins()
        .into_iter()
        .filter(|p| p.owner_part_id <= 0 || p.owner_part_id == c.current_part_id)
        .map(|p| {
            let base = point(p.location);
            let (dx, dy) = step(p.orientation);
            let length = p.length.raw() as i64;
            let hidden_net = p.hidden_net_name.trim();
            RawPin {
                number: p.designator.clone(),
                name: p.name.clone(),
                kind: pin_kind(p.electrical),
                base,
                hot: Point::new(base.x + dx * length, base.y + dy * length),
                hidden_net: (p.is_hidden && !hidden_net.is_empty()).then(|| hidden_net.to_string()),
            }
        })
        .collect();

    RawComponent {
        designator: c.designator.clone(),
        part_id: c.current_part_id,
        lib_reference: c.lib_reference.clone(),
        description: c.description.clone().filter(|d| !d.trim().is_empty()),
        comment,
        footprint,
        location: point(c.location),
        pins,
        parameters,
    }
}

fn symbol(s: &SheetSymbol) -> RawSheetSymbol {
    let origin = point(s.location);
    let (width, height) = (s.x_size.raw() as i64, s.y_size.raw() as i64);
    let entries = s
        .entries()
        .into_iter()
        .map(|e| {
            let d = e.distance_from_top.raw() as i64;
            let at = match e.side {
                LeftRightSide::Right => Point::new(origin.x + width, origin.y - d),
                LeftRightSide::Top => Point::new(origin.x + d, origin.y),
                LeftRightSide::Bottom => Point::new(origin.x + d, origin.y - height),
                _ => Point::new(origin.x, origin.y - d),
            };
            RawEntry {
                name: e.name.clone(),
                at,
            }
        })
        .collect();
    RawSheetSymbol {
        name: s.sheet_name.clone(),
        file_name: s.file_name.clone(),
        rect: Rect {
            min: Point::new(origin.x, origin.y - height),
            max: Point::new(origin.x + width, origin.y),
        },
        entries,
    }
}

/// Load one schematic file as drawn objects.
pub fn load_sheet(path: &Path) -> Result<RawSheet> {
    let doc = altium_format::SchDoc::open(path).map_err(|e| failure(path, e))?;
    let sheet = doc.sheet().map_err(|e| failure(path, e))?;
    let mut raw = RawSheet {
        path: path.to_path_buf(),
        ..Default::default()
    };

    for object in &sheet.objects {
        match object {
            SheetObject::Component(c) => raw.components.push(component(c)),
            SheetObject::Wire(w) => raw.wires.push(RawWire {
                vertices: w.vertices.iter().map(|&v| point(v)).collect(),
            }),
            SheetObject::Junction(j) => raw.junctions.push(point(j.location)),
            SheetObject::NetLabel(l) => raw.labels.push(RawLabel {
                text: l.text.clone(),
                at: point(l.location),
            }),
            SheetObject::PowerObject(p) => raw.power.push(RawPower {
                name: p.text.clone(),
                at: point(p.location),
            }),
            SheetObject::Port(p) => {
                let a = point(p.location);
                let width = p.width.raw() as i64;
                let vertical = matches!(
                    p.style,
                    PortArrowStyle::NoneVertical
                        | PortArrowStyle::Top
                        | PortArrowStyle::Bottom
                        | PortArrowStyle::TopBottom
                );
                let b = if vertical {
                    Point::new(a.x, a.y + width)
                } else {
                    Point::new(a.x + width, a.y)
                };
                raw.ports.push(RawPort {
                    name: p.name.clone(),
                    ends: [a, b],
                });
            }
            SheetObject::SheetSymbol(s) => raw.symbols.push(symbol(s)),
            SheetObject::NoConnect(n) => raw.no_connects.push(point(n.location)),
            SheetObject::BusEntry(b) => raw.bus_entries.push([point(b.location), point(b.corner)]),
            SheetObject::Bus(_) => raw.bus_count += 1,
            SheetObject::HarnessConnector(_) | SheetObject::SignalHarness(_) => {
                raw.harness_count += 1
            }
            _ => {}
        }
    }
    Ok(raw)
}

/// Render one schematic file to an SVG document.
pub fn render_sheet_svg(path: &Path) -> Result<String> {
    let doc = altium_format::SchDoc::open(path).map_err(|e| failure(path, e))?;
    altium_format_render_svg::render_schdoc(&doc).map_err(|e| failure(path, e))
}

/// Find a file named by a project or sheet symbol. Altium writes Windows-style
/// relative paths and Windows ignores letter case, so both are tolerated here.
pub fn resolve(dir: &Path, relative: &str) -> Option<PathBuf> {
    let mut current = dir.to_path_buf();
    for part in relative
        .split(['/', '\\'])
        .filter(|p| !p.is_empty() && *p != ".")
    {
        let exact = current.join(part);
        if exact.exists() {
            current = exact;
            continue;
        }
        let found = std::fs::read_dir(&current)
            .ok()?
            .filter_map(|entry| entry.ok())
            .find(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .eq_ignore_ascii_case(part)
            })?;
        current = found.path();
    }
    current.is_file().then_some(current)
}

/// Read an Altium project file and locate its documents.
pub fn load_project(path: &Path) -> Result<ProjectFile> {
    let project = altium_format::AltiumProject::open(path).map_err(|e| failure(path, e))?;
    let typed = project.project().map_err(|e| failure(path, e))?;
    let dir = path.parent().unwrap_or(Path::new("."));

    let mut loaded = ProjectFile {
        name: project.name().to_string(),
        schematics: Vec::new(),
        other_documents: Vec::new(),
        missing: Vec::new(),
        hierarchy: match typed.hierarchy_mode {
            FlattenMode::Flat => ProjectHierarchy::Flat,
            FlattenMode::HierarchicalGlobalPorts => ProjectHierarchy::Hierarchical,
            FlattenMode::HierarchicalStrict => ProjectHierarchy::StrictHierarchical,
            FlattenMode::Global => ProjectHierarchy::Global,
            _ => ProjectHierarchy::Automatic,
        },
    };
    for document in &typed.documents {
        match resolve(dir, &document.path) {
            None => loaded.missing.push(document.path.clone()),
            Some(found) => {
                let is_schematic = found
                    .extension()
                    .is_some_and(|ext| ext.to_string_lossy().eq_ignore_ascii_case("schdoc"));
                if is_schematic {
                    loaded.schematics.push(found);
                } else {
                    loaded.other_documents.push(found);
                }
            }
        }
    }
    Ok(loaded)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::geom::Point;

    fn fixture(name: &str) -> PathBuf {
        PathBuf::from(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../fixtures/altiumsharp"
        ))
        .join(name)
    }

    #[test]
    fn loads_components_with_absolute_pin_hot_spots() {
        let sheet = load_sheet(&fixture("Level Shifter.SchDoc")).unwrap();
        assert_eq!(sheet.components.len(), 5);
        let ic5 = sheet
            .components
            .iter()
            .find(|c| c.designator == "IC5")
            .unwrap();
        let pin2 = ic5.pins.iter().find(|p| p.number == "2").unwrap();
        assert_eq!(pin2.base, Point::new(52_000_000, 37_000_000));
        assert_eq!(pin2.hot, Point::new(49_000_000, 37_000_000));
    }

    #[test]
    fn resolves_a_comment_that_refers_to_another_parameter() {
        let sheet = load_sheet(&fixture("Level Shifter.SchDoc")).unwrap();
        let ic5 = sheet
            .components
            .iter()
            .find(|c| c.designator == "IC5")
            .unwrap();
        assert_eq!(ic5.comment.as_deref(), Some("TXU0304PWR"));
        let c17 = sheet
            .components
            .iter()
            .find(|c| c.designator == "C17")
            .unwrap();
        assert_eq!(c17.comment.as_deref(), Some("1 µF"));
    }

    #[test]
    fn loads_ports_with_both_ends() {
        let sheet = load_sheet(&fixture("Level Shifter.SchDoc")).unwrap();
        assert_eq!(sheet.ports.len(), 8);
        let port = sheet.ports.iter().find(|p| p.name == "CS_3V3").unwrap();
        assert_eq!(
            port.ends,
            [
                Point::new(39_000_000, 37_000_000),
                Point::new(46_000_000, 37_000_000)
            ]
        );
    }

    #[test]
    fn loads_sheet_symbols_with_entry_positions() {
        let sheet = load_sheet(&fixture("Overview.SchDoc")).unwrap();
        let names: Vec<(&str, &str)> = sheet
            .symbols
            .iter()
            .map(|s| (s.name.as_str(), s.file_name.as_str()))
            .collect();
        assert_eq!(
            names,
            [
                ("DAC", "DAC.SchDoc"),
                ("PSU", "Power Supply.SchDoc"),
                ("SPI_SHIFT", "Level Shifter.SchDoc"),
                ("LDAC_SHIFT", "Level Shifter.SchDoc"),
            ]
        );
        let spi = &sheet.symbols[2];
        let entry = spi.entries.iter().find(|e| e.name == "CS_3V3").unwrap();
        assert_eq!(entry.at, Point::new(40_000_000, 43_000_000));
        let right = spi.entries.iter().find(|e| e.name == "CS_5V").unwrap();
        assert_eq!(right.at, Point::new(55_000_000, 43_000_000));
    }

    #[test]
    fn every_sample_sheet_loads_and_renders() {
        for name in [
            "DAC",
            "Fanout Buffer",
            "Level Shifter",
            "Overview",
            "Power Supplies",
            "Power Supply",
            "SPI Isolator",
            "USB Power",
        ] {
            let path = fixture(&format!("{name}.SchDoc"));
            load_sheet(&path).unwrap_or_else(|e| panic!("{name}: {e}"));
            let svg = render_sheet_svg(&path).unwrap_or_else(|e| panic!("{name}: {e}"));
            assert!(svg.starts_with("<svg"), "{name}");
        }
    }

    #[test]
    fn a_file_that_is_not_a_schematic_is_an_error_not_a_panic() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("broken.SchDoc");
        std::fs::write(&path, b"this is not an Altium file").unwrap();
        assert!(matches!(
            load_sheet(&path),
            Err(crate::error::Error::Altium { .. })
        ));
    }

    #[test]
    fn resolves_windows_paths_and_mismatched_case() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("Sheets")).unwrap();
        std::fs::write(dir.path().join("Sheets/Power.SchDoc"), b"x").unwrap();
        let found = resolve(dir.path(), r"sheets\POWER.schdoc").unwrap();
        assert_eq!(found, dir.path().join("Sheets/Power.SchDoc"));
        assert_eq!(resolve(dir.path(), r"Sheets\Missing.SchDoc"), None);
    }

    #[test]
    fn reads_a_project_file() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("Sub")).unwrap();
        std::fs::write(dir.path().join("Sub/Main.SchDoc"), b"x").unwrap();
        std::fs::write(dir.path().join("Board.PcbDoc"), b"x").unwrap();
        let project = dir.path().join("Demo.PrjPcb");
        std::fs::write(
            &project,
            "[Design]\r\nVersion=1.0\r\nHierarchyMode=1\r\n\r\n\
             [Document1]\r\nDocumentPath=sub\\main.SchDoc\r\n\r\n\
             [Document2]\r\nDocumentPath=Board.PcbDoc\r\n\r\n\
             [Document3]\r\nDocumentPath=Gone.SchDoc\r\n",
        )
        .unwrap();

        let loaded = load_project(&project).unwrap();
        assert_eq!(loaded.name, "Demo");
        assert_eq!(loaded.schematics, [dir.path().join("Sub/Main.SchDoc")]);
        assert_eq!(loaded.other_documents, [dir.path().join("Board.PcbDoc")]);
        assert_eq!(loaded.missing, ["Gone.SchDoc"]);
        assert_eq!(loaded.hierarchy, ProjectHierarchy::Flat);
    }
}
