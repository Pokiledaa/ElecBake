//! The design model: what every consumer (viewer, MCP tools, chat) reads.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::diagnostic::Diagnostic;
use crate::geom::{Point, Rect};
use crate::netlist::{NetScope, Netlist, net_key};
use crate::schematic::{PinKind, RawComponent, RawSheet};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Design {
    pub name: String,
    /// The `.PrjPcb` or `.SchDoc` the design was loaded from.
    pub source: PathBuf,
    pub scope: NetScope,
    pub sheets: Vec<Sheet>,
    pub components: Vec<Component>,
    pub nets: Vec<Net>,
    pub diagnostics: Vec<Diagnostic>,
}

/// One use of a sheet in the design.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sheet {
    pub id: String,
    pub name: String,
    pub path: PathBuf,
    pub parent: Option<String>,
    /// Bounding box of everything drawn on the sheet, in Altium raw units.
    pub extent: Option<Rect>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Component {
    pub designator: String,
    pub lib_reference: String,
    pub value: Option<String>,
    pub footprint: Option<String>,
    pub description: Option<String>,
    pub parameters: BTreeMap<String, String>,
    /// Where the component is drawn; more than one for a multi-part component.
    pub placements: Vec<Placement>,
    pub pins: Vec<Pin>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Placement {
    pub sheet: String,
    pub bbox: Rect,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pin {
    pub number: String,
    pub name: String,
    pub kind: PinKind,
    pub net: String,
    pub sheet: String,
    pub at: Point,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Net {
    pub name: String,
    pub generated: bool,
    pub pins: Vec<NetPin>,
    pub sheets: Vec<String>,
    pub wires: Vec<NetWire>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetPin {
    pub designator: String,
    pub number: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetWire {
    pub sheet: String,
    pub vertices: Vec<Point>,
}

impl Design {
    pub fn component(&self, designator: &str) -> Option<&Component> {
        self.components.iter().find(|c| c.designator.eq_ignore_ascii_case(designator.trim()))
    }

    pub fn net(&self, name: &str) -> Option<&Net> {
        self.nets.iter().find(|n| net_key(&n.name) == net_key(name))
    }
}

fn sheet_extent(sheet: &RawSheet) -> Option<Rect> {
    let mut points: Vec<Point> = Vec::new();
    for component in &sheet.components {
        points.push(component.location);
        points.extend(component.pins.iter().flat_map(|pin| [pin.base, pin.hot]));
    }
    points.extend(sheet.wires.iter().flat_map(|wire| wire.vertices.iter().copied()));
    points.extend(sheet.junctions.iter().copied());
    points.extend(sheet.labels.iter().map(|label| label.at));
    points.extend(sheet.power.iter().map(|power| power.at));
    points.extend(sheet.ports.iter().flat_map(|port| port.ends));
    points.extend(sheet.symbols.iter().flat_map(|symbol| [symbol.rect.min, symbol.rect.max]));
    Rect::around(points)
}

fn component_bbox(component: &RawComponent) -> Rect {
    let points = component.pins.iter().flat_map(|pin| [pin.base, pin.hot]).chain([component.location]);
    Rect::around(points).unwrap_or_default()
}

/// Build the design model from loaded sheets and their netlist.
/// `earlier` holds diagnostics raised while loading; they are listed first.
pub fn assemble(
    name: String,
    source: PathBuf,
    sheets: &[RawSheet],
    netlist: Netlist,
    earlier: Vec<Diagnostic>,
) -> Design {
    let instances = &netlist.instances;

    let sheets_out = instances
        .iter()
        .map(|instance| {
            let raw = &sheets[instance.sheet];
            Sheet {
                id: instance.id.clone(),
                name: raw.path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default(),
                path: raw.path.clone(),
                parent: instance.parent.map(|p| instances[p].id.clone()),
                extent: sheet_extent(raw),
            }
        })
        .collect();

    let mut net_of: HashMap<(usize, usize, usize), &str> = HashMap::new();
    for net in &netlist.nets {
        for pin in &net.pins {
            net_of.insert((pin.instance, pin.component, pin.pin), &net.name);
        }
    }

    let mut components: BTreeMap<String, Component> = BTreeMap::new();
    for (i, instance) in instances.iter().enumerate() {
        for (c, raw) in sheets[instance.sheet].components.iter().enumerate() {
            let designator = format!("{}{}", raw.designator, instance.suffix);
            let component = components.entry(designator.clone()).or_insert_with(|| Component {
                designator,
                lib_reference: raw.lib_reference.clone(),
                value: raw.comment.clone(),
                footprint: raw.footprint.clone(),
                description: raw.description.clone(),
                parameters: raw.parameters.iter().cloned().collect(),
                placements: Vec::new(),
                pins: Vec::new(),
            });
            component.placements.push(Placement { sheet: instance.id.clone(), bbox: component_bbox(raw) });
            for (p, pin) in raw.pins.iter().enumerate() {
                component.pins.push(Pin {
                    number: pin.number.clone(),
                    name: pin.name.clone(),
                    kind: pin.kind,
                    net: net_of.get(&(i, c, p)).copied().unwrap_or_default().to_string(),
                    sheet: instance.id.clone(),
                    at: pin.hot,
                });
            }
        }
    }

    let nets = netlist
        .nets
        .iter()
        .map(|net| {
            let mut pins: Vec<NetPin> = net
                .pins
                .iter()
                .map(|pin| NetPin { designator: pin.designator.clone(), number: pin.number.clone() })
                .collect();
            pins.dedup();
            let sheet_ids: BTreeSet<&str> = net.instances.iter().map(|&i| instances[i].id.as_str()).collect();
            Net {
                name: net.name.clone(),
                generated: net.generated,
                pins,
                sheets: sheet_ids.into_iter().map(str::to_string).collect(),
                wires: net
                    .wires
                    .iter()
                    .map(|(i, vertices)| NetWire { sheet: instances[*i].id.clone(), vertices: vertices.clone() })
                    .collect(),
            }
        })
        .collect();

    let mut diagnostics = earlier;
    diagnostics.extend(netlist.diagnostics.iter().cloned());

    Design {
        name,
        source,
        scope: netlist.scope,
        sheets: sheets_out,
        components: components.into_values().collect(),
        nets,
        diagnostics,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::DiagnosticKind;
    use crate::netlist::{NetScope, build};
    use crate::schematic::testkit::*;

    fn two_sheet_design() -> Design {
        let mut parent = sheet("top.SchDoc");
        parent.components = vec![component("R1", &[("1", (0, 0)), ("2", (0, 50))])];
        parent.components[0].comment = Some("10k".into());
        parent.components[0].parameters = vec![("Tolerance".into(), "1%".into())];
        parent.wires = vec![wire(&[(0, 0), (10, 0)])];
        parent.power = vec![power("GND", (0, 50))];
        parent.symbols = vec![symbol("SUB", "child.SchDoc", &[("IN", (10, 0))])];
        let mut child = sheet("child.SchDoc");
        child.components = vec![component("C1", &[("1", (0, 0)), ("2", (0, 50))])];
        child.wires = vec![wire(&[(0, 0), (10, 0)])];
        child.ports = vec![port("IN", (10, 0), (20, 0))];
        child.power = vec![power("GND", (0, 50))];

        let sheets = [parent, child];
        let netlist = build(&sheets, NetScope::Automatic);
        assemble("demo".into(), "top.SchDoc".into(), &sheets, netlist, Vec::new())
    }

    #[test]
    fn sheets_list_instances_with_their_parents() {
        let design = two_sheet_design();
        let sheets: Vec<(&str, Option<&str>)> =
            design.sheets.iter().map(|s| (s.id.as_str(), s.parent.as_deref())).collect();
        assert_eq!(sheets, [("top", None), ("top/SUB", Some("top"))]);
        assert_eq!(design.sheets[1].name, "child");
        assert_eq!(design.sheets[0].extent.unwrap().max, crate::geom::Point::new(10, 50));
    }

    #[test]
    fn components_carry_values_parameters_and_pin_nets() {
        let design = two_sheet_design();
        let r1 = design.component("r1").unwrap();
        assert_eq!(r1.value.as_deref(), Some("10k"));
        assert_eq!(r1.parameters["Tolerance"], "1%");
        assert_eq!(r1.placements[0].sheet, "top");
        let nets: Vec<(&str, &str)> = r1.pins.iter().map(|p| (p.number.as_str(), p.net.as_str())).collect();
        assert_eq!(nets, [("1", "IN"), ("2", "GND")]);
    }

    #[test]
    fn nets_list_pins_sheets_and_wires() {
        let design = two_sheet_design();
        let net = design.net("in").unwrap();
        let pins: Vec<String> = net.pins.iter().map(|p| format!("{}.{}", p.designator, p.number)).collect();
        assert_eq!(pins, ["C1.1", "R1.1"]);
        assert_eq!(net.sheets, ["top", "top/SUB"]);
        assert_eq!(net.wires.len(), 2);
        assert_eq!(net.wires[0].sheet, "top");
        assert!(!net.generated);
    }

    #[test]
    fn parts_of_a_multi_part_component_become_one_component() {
        let mut s = sheet("a.SchDoc");
        s.components = vec![component("U1", &[("1", (0, 0))]), component("U1", &[("5", (50, 0))])];
        s.components[1].part_id = 2;
        let sheets = [s];
        let design =
            assemble("demo".into(), "a.SchDoc".into(), &sheets, build(&sheets, NetScope::Automatic), Vec::new());
        assert_eq!(design.components.len(), 1);
        assert_eq!(design.components[0].pins.len(), 2);
        assert_eq!(design.components[0].placements.len(), 2);
    }

    #[test]
    fn earlier_diagnostics_come_first_and_the_model_round_trips_through_json() {
        let sheets = [sheet("a.SchDoc")];
        let earlier = vec![Diagnostic::new(DiagnosticKind::SheetLoadFailed, "b.SchDoc: broken")];
        let design =
            assemble("demo".into(), "a.SchDoc".into(), &sheets, build(&sheets, NetScope::Automatic), earlier);
        assert_eq!(design.diagnostics[0].kind, DiagnosticKind::SheetLoadFailed);

        let json = serde_json::to_string(&two_sheet_design()).unwrap();
        let back: Design = serde_json::from_str(&json).unwrap();
        assert_eq!(back.components.len(), 2);
        assert!(json.contains(r#""scope":"hierarchical""#));
    }
}
