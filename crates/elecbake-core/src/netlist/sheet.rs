//! Connectivity inside a single sheet.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use super::union_find::UnionFind;
use crate::geom::{Point, on_segment};
use crate::schematic::RawSheet;

/// One connected group of objects on a sheet. Indices refer to the sheet's own lists.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LocalNet {
    /// (component index, pin index)
    pub pins: Vec<(usize, usize)>,
    pub labels: BTreeSet<String>,
    pub power: BTreeSet<String>,
    pub ports: BTreeSet<String>,
    /// (sheet symbol index, entry index)
    pub entries: Vec<(usize, usize)>,
    pub wires: Vec<usize>,
    pub touches_bus: bool,
    pub no_connect: bool,
}

#[derive(Debug, Clone, Default)]
pub struct SheetNets {
    pub nets: Vec<LocalNet>,
    /// Wire ends that touch nothing.
    pub dangling: Vec<Point>,
}

/// The form of a net name used for matching: Altium ignores letter case.
pub fn net_key(name: &str) -> String {
    name.trim().to_uppercase()
}

#[derive(Clone, Copy)]
enum Item {
    Wire(usize),
    Pin(usize, usize),
    Label(usize),
    Power(usize),
    Port(usize),
    Entry(usize, usize),
}

fn wire_contains(vertices: &[Point], p: Point) -> bool {
    match vertices {
        [only] => *only == p,
        _ => vertices.windows(2).any(|s| on_segment(p, s[0], s[1])),
    }
}

pub fn connect_sheet(sheet: &RawSheet) -> SheetNets {
    // Every object that can belong to a net gets a node. Wires come first, so a wire's
    // node number equals its index.
    let mut items: Vec<Item> = (0..sheet.wires.len()).map(Item::Wire).collect();
    // Points where a non-wire object attaches, with that object's node.
    let mut anchors: Vec<(Point, usize)> = Vec::new();

    for (c, component) in sheet.components.iter().enumerate() {
        for (p, pin) in component.pins.iter().enumerate() {
            anchors.push((pin.hot, items.len()));
            items.push(Item::Pin(c, p));
        }
    }
    for (i, label) in sheet.labels.iter().enumerate() {
        anchors.push((label.at, items.len()));
        items.push(Item::Label(i));
    }
    for (i, power) in sheet.power.iter().enumerate() {
        anchors.push((power.at, items.len()));
        items.push(Item::Power(i));
    }
    for (i, port) in sheet.ports.iter().enumerate() {
        anchors.push((port.ends[0], items.len()));
        anchors.push((port.ends[1], items.len()));
        items.push(Item::Port(i));
    }
    for (s, symbol) in sheet.symbols.iter().enumerate() {
        for (e, entry) in symbol.entries.iter().enumerate() {
            anchors.push((entry.at, items.len()));
            items.push(Item::Entry(s, e));
        }
    }

    let mut groups = UnionFind::new(items.len());

    // Rule 1: a vertex of one wire on another wire.
    for (i, a) in sheet.wires.iter().enumerate() {
        for &vertex in &a.vertices {
            for (j, b) in sheet.wires.iter().enumerate() {
                if i != j && wire_contains(&b.vertices, vertex) {
                    groups.union(i, j);
                }
            }
        }
    }

    // Rule 2: a junction joins every wire through its point.
    for &junction in &sheet.junctions {
        let through: Vec<usize> = (0..sheet.wires.len())
            .filter(|&w| wire_contains(&sheet.wires[w].vertices, junction))
            .collect();
        for pair in through.windows(2) {
            groups.union(pair[0], pair[1]);
        }
    }

    // Rule 3: objects on a wire, and objects at the same point.
    let mut at_point: HashMap<Point, usize> = HashMap::new();
    for &(point, node) in &anchors {
        for (w, wire) in sheet.wires.iter().enumerate() {
            if wire_contains(&wire.vertices, point) {
                groups.union(node, w);
            }
        }
        match at_point.get(&point) {
            Some(&other) => groups.union(node, other),
            None => {
                at_point.insert(point, node);
            }
        }
    }

    // Rules 4 and 5: objects sharing a name. Labels, power objects and hidden pins
    // share one namespace; ports have their own.
    let mut by_name: HashMap<(bool, String), usize> = HashMap::new();
    for (node, item) in items.iter().enumerate() {
        let key = match *item {
            Item::Label(i) => Some((false, net_key(&sheet.labels[i].text))),
            Item::Power(i) => Some((false, net_key(&sheet.power[i].name))),
            Item::Pin(c, p) => sheet.components[c].pins[p]
                .hidden_net
                .as_deref()
                .map(|name| (false, net_key(name))),
            Item::Port(i) => Some((true, net_key(&sheet.ports[i].name))),
            _ => None,
        };
        let Some(key) = key else { continue };
        match by_name.get(&key) {
            Some(&other) => groups.union(node, other),
            None => {
                by_name.insert(key, node);
            }
        }
    }

    let on_bus_entry = |vertices: &[Point]| {
        sheet
            .bus_entries
            .iter()
            .flatten()
            .any(|&end| wire_contains(vertices, end))
    };

    let mut index: BTreeMap<usize, usize> = BTreeMap::new();
    let mut nets: Vec<LocalNet> = Vec::new();
    for (node, item) in items.iter().enumerate() {
        let root = groups.find(node);
        let n = *index.entry(root).or_insert_with(|| {
            nets.push(LocalNet::default());
            nets.len() - 1
        });
        let net = &mut nets[n];
        match *item {
            Item::Wire(w) => {
                net.wires.push(w);
                net.touches_bus |= on_bus_entry(&sheet.wires[w].vertices);
            }
            Item::Pin(c, p) => {
                let pin = &sheet.components[c].pins[p];
                net.pins.push((c, p));
                net.no_connect |= sheet.no_connects.contains(&pin.hot);
                if let Some(name) = &pin.hidden_net {
                    net.power.insert(name.clone());
                }
            }
            Item::Label(i) => {
                net.labels.insert(sheet.labels[i].text.trim().to_string());
            }
            Item::Power(i) => {
                net.power.insert(sheet.power[i].name.trim().to_string());
            }
            Item::Port(i) => {
                net.ports.insert(sheet.ports[i].name.trim().to_string());
            }
            Item::Entry(s, e) => net.entries.push((s, e)),
        }
    }

    let mut dangling = Vec::new();
    for (i, wire) in sheet.wires.iter().enumerate() {
        let ends = match wire.vertices.as_slice() {
            [] => continue,
            [only] => vec![*only],
            [first, .., last] => vec![*first, *last],
        };
        for end in ends {
            let touches_wire = sheet
                .wires
                .iter()
                .enumerate()
                .any(|(j, other)| j != i && wire_contains(&other.vertices, end));
            let touches_bus = sheet.bus_entries.iter().flatten().any(|&p| p == end);
            if !touches_wire && !touches_bus && !at_point.contains_key(&end) {
                dangling.push(end);
            }
        }
    }

    SheetNets { nets, dangling }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schematic::testkit::*;

    /// The net holding pin `number` of component `designator`.
    fn net_of<'a>(
        sheet: &RawSheet,
        nets: &'a SheetNets,
        designator: &str,
        number: &str,
    ) -> &'a LocalNet {
        let c = sheet
            .components
            .iter()
            .position(|c| c.designator == designator)
            .unwrap();
        let p = sheet.components[c]
            .pins
            .iter()
            .position(|p| p.number == number)
            .unwrap();
        nets.nets.iter().find(|n| n.pins.contains(&(c, p))).unwrap()
    }

    fn same_net(sheet: &RawSheet, a: (&str, &str), b: (&str, &str)) -> bool {
        let nets = connect_sheet(sheet);
        std::ptr::eq(
            net_of(sheet, &nets, a.0, a.1),
            net_of(sheet, &nets, b.0, b.1),
        )
    }

    #[test]
    fn pins_at_the_two_ends_of_a_wire_connect() {
        let mut s = sheet("a.SchDoc");
        s.components = vec![
            component("R1", &[("1", (0, 0))]),
            component("R2", &[("1", (10, 0))]),
        ];
        s.wires = vec![wire(&[(0, 0), (10, 0)])];
        assert!(same_net(&s, ("R1", "1"), ("R2", "1")));
    }

    #[test]
    fn a_pin_in_the_middle_of_a_wire_connects() {
        let mut s = sheet("a.SchDoc");
        s.components = vec![
            component("R1", &[("1", (0, 0))]),
            component("R2", &[("1", (5, 0))]),
        ];
        s.wires = vec![wire(&[(0, 0), (10, 0)])];
        assert!(same_net(&s, ("R1", "1"), ("R2", "1")));
    }

    #[test]
    fn pins_with_no_wire_between_them_do_not_connect() {
        let mut s = sheet("a.SchDoc");
        s.components = vec![
            component("R1", &[("1", (0, 0))]),
            component("R2", &[("1", (10, 0))]),
        ];
        assert!(!same_net(&s, ("R1", "1"), ("R2", "1")));
    }

    #[test]
    fn crossing_wires_connect_only_with_a_junction() {
        let mut s = sheet("a.SchDoc");
        s.components = vec![
            component("R1", &[("1", (0, 5))]),
            component("R2", &[("1", (5, 0))]),
        ];
        s.wires = vec![wire(&[(0, 5), (10, 5)]), wire(&[(5, 0), (5, 10)])];
        assert!(!same_net(&s, ("R1", "1"), ("R2", "1")));
        s.junctions = vec![pt(5, 5)];
        assert!(same_net(&s, ("R1", "1"), ("R2", "1")));
    }

    #[test]
    fn a_wire_ending_on_another_wire_connects() {
        let mut s = sheet("a.SchDoc");
        s.components = vec![
            component("R1", &[("1", (0, 5))]),
            component("R2", &[("1", (5, 0))]),
        ];
        s.wires = vec![wire(&[(0, 5), (10, 5)]), wire(&[(5, 0), (5, 5)])];
        assert!(same_net(&s, ("R1", "1"), ("R2", "1")));
    }

    #[test]
    fn a_wire_with_a_corner_connects_end_to_end() {
        let mut s = sheet("a.SchDoc");
        s.components = vec![
            component("R1", &[("1", (0, 0))]),
            component("R2", &[("1", (10, 10))]),
        ];
        s.wires = vec![wire(&[(0, 0), (10, 0), (10, 10)])];
        assert!(same_net(&s, ("R1", "1"), ("R2", "1")));
    }

    #[test]
    fn pins_touching_each_other_connect() {
        let mut s = sheet("a.SchDoc");
        s.components = vec![
            component("R1", &[("1", (3, 3))]),
            component("R2", &[("1", (3, 3))]),
        ];
        assert!(same_net(&s, ("R1", "1"), ("R2", "1")));
    }

    #[test]
    fn the_same_label_joins_separate_wires_ignoring_case() {
        let mut s = sheet("a.SchDoc");
        s.components = vec![
            component("R1", &[("1", (0, 0))]),
            component("R2", &[("1", (0, 50))]),
        ];
        s.wires = vec![wire(&[(0, 0), (10, 0)]), wire(&[(0, 50), (10, 50)])];
        s.labels = vec![label("VCC", (5, 0)), label("vcc", (5, 50))];
        assert!(same_net(&s, ("R1", "1"), ("R2", "1")));
        let nets = connect_sheet(&s);
        let names: Vec<&String> = net_of(&s, &nets, "R1", "1").labels.iter().collect();
        assert_eq!(names, ["VCC", "vcc"]);
    }

    #[test]
    fn different_labels_stay_separate() {
        let mut s = sheet("a.SchDoc");
        s.components = vec![
            component("R1", &[("1", (0, 0))]),
            component("R2", &[("1", (0, 50))]),
        ];
        s.wires = vec![wire(&[(0, 0), (10, 0)]), wire(&[(0, 50), (10, 50)])];
        s.labels = vec![label("A", (5, 0)), label("B", (5, 50))];
        assert!(!same_net(&s, ("R1", "1"), ("R2", "1")));
    }

    #[test]
    fn a_power_object_joins_a_label_of_the_same_name() {
        let mut s = sheet("a.SchDoc");
        s.components = vec![
            component("R1", &[("1", (0, 0))]),
            component("R2", &[("1", (0, 50))]),
        ];
        s.wires = vec![wire(&[(0, 50), (10, 50)])];
        s.power = vec![power("GND", (0, 0))];
        s.labels = vec![label("GND", (5, 50))];
        assert!(same_net(&s, ("R1", "1"), ("R2", "1")));
        let nets = connect_sheet(&s);
        assert!(net_of(&s, &nets, "R1", "1").power.contains("GND"));
    }

    #[test]
    fn a_wire_connects_at_either_end_of_a_port() {
        for wire_x in [20, 30] {
            let mut s = sheet("a.SchDoc");
            s.components = vec![component("R1", &[("1", (wire_x, 10))])];
            s.wires = vec![wire(&[(wire_x, 0), (wire_x, 10)])];
            s.ports = vec![port("OUT", (20, 0), (30, 0))];
            let nets = connect_sheet(&s);
            assert!(
                net_of(&s, &nets, "R1", "1").ports.contains("OUT"),
                "wire at x={wire_x}"
            );
        }
    }

    #[test]
    fn a_sheet_entry_is_recorded_on_its_net() {
        let mut s = sheet("a.SchDoc");
        s.components = vec![component("R1", &[("1", (0, 0))])];
        s.wires = vec![wire(&[(0, 0), (10, 0)])];
        s.symbols = vec![symbol(
            "SUB",
            "sub.SchDoc",
            &[("IN", (10, 0)), ("OUT", (10, 9))],
        )];
        let nets = connect_sheet(&s);
        assert_eq!(net_of(&s, &nets, "R1", "1").entries, [(0, 0)]);
    }

    #[test]
    fn a_hidden_pin_joins_its_named_net() {
        let mut s = sheet("a.SchDoc");
        s.components = vec![
            component("U1", &[("7", (0, 0))]),
            component("C1", &[("2", (50, 50))]),
        ];
        s.components[0].pins[0].hidden_net = Some("GND".into());
        s.power = vec![power("GND", (50, 50))];
        assert!(same_net(&s, ("U1", "7"), ("C1", "2")));
    }

    #[test]
    fn a_no_connect_marker_is_recorded() {
        let mut s = sheet("a.SchDoc");
        s.components = vec![component("U1", &[("1", (0, 0)), ("2", (0, 10))])];
        s.no_connects = vec![pt(0, 0)];
        let nets = connect_sheet(&s);
        assert!(net_of(&s, &nets, "U1", "1").no_connect);
        assert!(!net_of(&s, &nets, "U1", "2").no_connect);
    }

    #[test]
    fn a_wire_end_touching_nothing_is_dangling() {
        let mut s = sheet("a.SchDoc");
        s.components = vec![component("R1", &[("1", (0, 0))])];
        s.wires = vec![wire(&[(0, 0), (10, 0)])];
        assert_eq!(connect_sheet(&s).dangling, [pt(10, 0)]);
    }

    #[test]
    fn a_net_touching_a_bus_entry_is_flagged() {
        let mut s = sheet("a.SchDoc");
        s.components = vec![component("R1", &[("1", (0, 0))])];
        s.wires = vec![wire(&[(0, 0), (10, 0)])];
        s.bus_entries = vec![[pt(10, 0), pt(12, 2)]];
        let nets = connect_sheet(&s);
        assert!(net_of(&s, &nets, "R1", "1").touches_bus);
        assert!(nets.dangling.is_empty());
    }

    #[test]
    fn an_empty_sheet_has_no_nets() {
        let nets = connect_sheet(&sheet("empty.SchDoc"));
        assert!(nets.nets.is_empty());
        assert!(nets.dangling.is_empty());
    }
}
