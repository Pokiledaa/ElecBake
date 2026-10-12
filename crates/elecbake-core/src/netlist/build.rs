//! Joins per-sheet nets across the sheet hierarchy and names them.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use serde::{Deserialize, Serialize};

use super::sheet::{SheetNets, connect_sheet, net_key};
use super::union_find::UnionFind;
use crate::diagnostic::{Diagnostic, DiagnosticKind};
use crate::geom::Point;
use crate::schematic::RawSheet;

/// How nets connect across sheets (Altium's "net identifier scope").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NetScope {
    Automatic,
    Flat,
    Hierarchical,
    StrictHierarchical,
    Global,
}

/// One use of a sheet in the design. A sheet referenced by two sheet symbols has two.
#[derive(Debug, Clone)]
pub struct SheetInstance {
    /// Path of sheet-symbol names from the top sheet, for example `Overview/SPI_SHIFT`.
    pub id: String,
    /// Index into the sheets given to `build`.
    pub sheet: usize,
    pub parent: Option<usize>,
    /// Index of the sheet symbol on the parent's sheet that created this instance.
    pub symbol: Option<usize>,
    /// Appended to designators on this instance; empty unless a sheet is used more than once.
    pub suffix: String,
    pub depth: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct PinRef {
    pub designator: String,
    pub number: String,
    pub instance: usize,
    pub component: usize,
    pub pin: usize,
}

#[derive(Debug, Clone)]
pub struct NetEntry {
    pub name: String,
    /// True when no label, power object or port named the net.
    pub generated: bool,
    pub pins: Vec<PinRef>,
    /// (instance index, vertices of one wire)
    pub wires: Vec<(usize, Vec<Point>)>,
    /// Instances the net appears on, ascending.
    pub instances: Vec<usize>,
    pub no_connect: bool,
    pub touches_bus: bool,
}

#[derive(Debug, Clone)]
pub struct Netlist {
    /// The scope actually applied; never `Automatic`.
    pub scope: NetScope,
    pub instances: Vec<SheetInstance>,
    pub nets: Vec<NetEntry>,
    pub diagnostics: Vec<Diagnostic>,
}

/// The file name at the end of a path, lower-cased, whatever separators it uses.
fn file_key(path: &str) -> String {
    path.rsplit(['/', '\\']).next().unwrap_or(path).trim().to_lowercase()
}

fn file_stem(path: &str) -> String {
    let name = path.rsplit(['/', '\\']).next().unwrap_or(path).trim();
    match name.rsplit_once('.') {
        Some((stem, _)) if !stem.is_empty() => stem.to_string(),
        _ => name.to_string(),
    }
}

struct Hierarchy<'a> {
    sheets: &'a [RawSheet],
    by_file: HashMap<String, usize>,
    /// How many sheet symbols refer to each sheet.
    uses: Vec<usize>,
    instances: Vec<SheetInstance>,
    diagnostics: Vec<Diagnostic>,
}

impl Hierarchy<'_> {
    fn add(&mut self, sheet: usize, parent: Option<(usize, usize)>, open: &mut Vec<usize>) {
        let (mut id, suffix, depth) = match parent {
            None => (file_stem(&self.sheets[sheet].path.to_string_lossy()), String::new(), 0),
            Some((p, s)) => {
                let above = &self.instances[p];
                let symbol = &self.sheets[above.sheet].symbols[s];
                let name = match symbol.name.trim() {
                    "" => file_stem(&symbol.file_name),
                    name => name.to_string(),
                };
                let suffix = if self.uses[sheet] > 1 {
                    format!("{}_{}", above.suffix, name)
                } else {
                    above.suffix.clone()
                };
                (format!("{}/{}", above.id, name), suffix, above.depth + 1)
            }
        };
        while self.instances.iter().any(|i| i.id == id) {
            id.push('\'');
        }

        let me = self.instances.len();
        self.instances.push(SheetInstance {
            id,
            sheet,
            parent: parent.map(|(p, _)| p),
            symbol: parent.map(|(_, s)| s),
            suffix,
            depth,
        });

        open.push(sheet);
        for s in 0..self.sheets[sheet].symbols.len() {
            let symbol = &self.sheets[sheet].symbols[s];
            match self.by_file.get(&file_key(&symbol.file_name)).copied() {
                None => self.diagnostics.push(
                    Diagnostic::new(
                        DiagnosticKind::MissingSheet,
                        format!(
                            "sheet symbol '{}' refers to '{}', which was not loaded",
                            symbol.name, symbol.file_name
                        ),
                    )
                    .on_sheet(&self.instances[me].id),
                ),
                Some(child) if open.contains(&child) => self.diagnostics.push(
                    Diagnostic::new(
                        DiagnosticKind::HierarchyCycle,
                        format!(
                            "sheet symbol '{}' refers to '{}', which contains this sheet",
                            symbol.name, symbol.file_name
                        ),
                    )
                    .on_sheet(&self.instances[me].id),
                ),
                Some(child) => self.add(child, Some((me, s)), open),
            }
        }
        open.pop();
    }
}

fn instantiate(sheets: &[RawSheet]) -> (Vec<SheetInstance>, Vec<Diagnostic>) {
    let by_file: HashMap<String, usize> = sheets
        .iter()
        .enumerate()
        .map(|(i, s)| (file_key(&s.path.to_string_lossy()), i))
        .collect();
    let mut uses = vec![0usize; sheets.len()];
    for sheet in sheets {
        for symbol in &sheet.symbols {
            if let Some(&child) = by_file.get(&file_key(&symbol.file_name)) {
                uses[child] += 1;
            }
        }
    }
    let mut tops: Vec<usize> = (0..sheets.len()).filter(|&i| uses[i] == 0).collect();
    if tops.is_empty() && !sheets.is_empty() {
        // Every sheet is referenced by another, so the references form a loop.
        tops.push(0);
    }

    let mut hierarchy = Hierarchy { sheets, by_file, uses, instances: Vec::new(), diagnostics: Vec::new() };
    for top in tops {
        hierarchy.add(top, None, &mut Vec::new());
    }
    (hierarchy.instances, hierarchy.diagnostics)
}

fn effective_scope(sheets: &[RawSheet], scope: NetScope) -> NetScope {
    if scope != NetScope::Automatic {
        return scope;
    }
    if sheets.iter().any(|s| s.symbols.iter().any(|symbol| !symbol.entries.is_empty())) {
        NetScope::Hierarchical
    } else if sheets.iter().any(|s| !s.ports.is_empty()) {
        NetScope::Flat
    } else {
        NetScope::Global
    }
}

fn join_by_name(groups: &mut UnionFind, seen: &mut HashMap<String, usize>, name: &str, node: usize) {
    match seen.get(&net_key(name)) {
        Some(&other) => groups.union(node, other),
        None => {
            seen.insert(net_key(name), node);
        }
    }
}

pub fn build(sheets: &[RawSheet], scope: NetScope) -> Netlist {
    let local: Vec<SheetNets> = sheets.iter().map(connect_sheet).collect();
    let (instances, mut diagnostics) = instantiate(sheets);
    let scope = effective_scope(sheets, scope);

    // One node per (instance, local net).
    let mut offset = Vec::with_capacity(instances.len());
    let mut total = 0;
    for instance in &instances {
        offset.push(total);
        total += local[instance.sheet].nets.len();
    }
    let mut groups = UnionFind::new(total);

    let mut named: HashMap<String, usize> = HashMap::new();
    let mut ported: HashMap<String, usize> = HashMap::new();
    for (i, instance) in instances.iter().enumerate() {
        for (n, net) in local[instance.sheet].nets.iter().enumerate() {
            let node = offset[i] + n;
            if scope != NetScope::StrictHierarchical {
                for name in &net.power {
                    join_by_name(&mut groups, &mut named, name, node);
                }
            }
            if scope == NetScope::Global {
                for name in &net.labels {
                    join_by_name(&mut groups, &mut named, name, node);
                }
            }
            if matches!(scope, NetScope::Global | NetScope::Flat) {
                for name in &net.ports {
                    join_by_name(&mut groups, &mut ported, name, node);
                }
            }
        }
    }

    if matches!(scope, NetScope::Hierarchical | NetScope::StrictHierarchical) {
        for (i, instance) in instances.iter().enumerate() {
            let (Some(parent), Some(s)) = (instance.parent, instance.symbol) else { continue };
            let parent_sheet = instances[parent].sheet;
            let symbol = &sheets[parent_sheet].symbols[s];
            let mut used = vec![false; symbol.entries.len()];

            for (n, net) in local[instance.sheet].nets.iter().enumerate() {
                for port in &net.ports {
                    let entry = symbol.entries.iter().position(|e| net_key(&e.name) == net_key(port));
                    let above = entry.and_then(|e| {
                        local[parent_sheet].nets.iter().position(|net| net.entries.contains(&(s, e)))
                    });
                    match (entry, above) {
                        (Some(e), Some(above)) => {
                            used[e] = true;
                            groups.union(offset[i] + n, offset[parent] + above);
                        }
                        _ => diagnostics.push(
                            Diagnostic::new(
                                DiagnosticKind::UnmatchedPort,
                                format!(
                                    "port '{port}' has no matching entry on sheet symbol '{}'",
                                    symbol.name
                                ),
                            )
                            .on_sheet(&instance.id),
                        ),
                    }
                }
            }
            for (e, entry) in symbol.entries.iter().enumerate() {
                if !used[e] {
                    diagnostics.push(
                        Diagnostic::new(
                            DiagnosticKind::UnmatchedPort,
                            format!(
                                "entry '{}' on sheet symbol '{}' has no matching port in '{}'",
                                entry.name, symbol.name, symbol.file_name
                            ),
                        )
                        .on_sheet(&instances[parent].id)
                        .at(entry.at),
                    );
                }
            }
        }
    }

    let mut members: BTreeMap<usize, Vec<(usize, usize)>> = BTreeMap::new();
    for (i, instance) in instances.iter().enumerate() {
        for n in 0..local[instance.sheet].nets.len() {
            members.entry(groups.find(offset[i] + n)).or_default().push((i, n));
        }
    }

    let mut nets: Vec<NetEntry> = Vec::new();
    for parts in members.into_values() {
        let mut entry = NetEntry {
            name: String::new(),
            generated: false,
            pins: Vec::new(),
            wires: Vec::new(),
            instances: Vec::new(),
            no_connect: false,
            touches_bus: false,
        };
        // Candidate names by class: power, label, port. Each with the depth of its sheet.
        let mut candidates: [Vec<(usize, String)>; 3] = Default::default();

        for &(i, n) in &parts {
            let instance = &instances[i];
            let sheet = &sheets[instance.sheet];
            let net = &local[instance.sheet].nets[n];
            for &(c, p) in &net.pins {
                entry.pins.push(PinRef {
                    designator: format!("{}{}", sheet.components[c].designator, instance.suffix),
                    number: sheet.components[c].pins[p].number.clone(),
                    instance: i,
                    component: c,
                    pin: p,
                });
            }
            for &w in &net.wires {
                entry.wires.push((i, sheet.wires[w].vertices.clone()));
            }
            entry.instances.push(i);
            entry.no_connect |= net.no_connect;
            entry.touches_bus |= net.touches_bus;
            candidates[0].extend(net.power.iter().map(|name| (instance.depth, name.clone())));
            candidates[1].extend(net.labels.iter().map(|name| (instance.depth, name.clone())));
            candidates[2].extend(net.ports.iter().map(|name| (instance.depth, name.clone())));
        }
        if entry.pins.is_empty() {
            continue;
        }
        entry.pins.sort();
        entry.instances.sort();
        entry.instances.dedup();

        match candidates.iter_mut().find(|class| !class.is_empty()) {
            Some(class) => {
                class.sort();
                entry.name = class[0].1.clone();
                let distinct: BTreeSet<String> = class.iter().map(|(_, name)| net_key(name)).collect();
                if distinct.len() > 1 {
                    let others: Vec<&str> = class
                        .iter()
                        .map(|(_, name)| name.as_str())
                        .filter(|name| net_key(name) != net_key(&entry.name))
                        .collect();
                    diagnostics.push(
                        Diagnostic::new(
                            DiagnosticKind::MultipleNetNames,
                            format!(
                                "net '{}' also carries the name(s) {}; they are connected together",
                                entry.name,
                                others.join(", ")
                            ),
                        )
                        .net(&entry.name),
                    );
                }
            }
            None => {
                entry.name = format!("N${}_{}", entry.pins[0].designator, entry.pins[0].number);
                entry.generated = true;
            }
        }
        nets.push(entry);
    }

    // Separate nets must not share a name: it is the identifier everything else uses.
    let mut count: HashMap<String, usize> = HashMap::new();
    for net in &nets {
        *count.entry(net_key(&net.name)).or_default() += 1;
    }
    let mut taken: BTreeSet<String> = BTreeSet::new();
    for net in &mut nets {
        if count[&net_key(&net.name)] > 1 {
            let shallowest = net
                .instances
                .iter()
                .copied()
                .min_by_key(|&i| (instances[i].depth, i))
                .expect("a net with pins appears on at least one sheet");
            let renamed = format!("{}@{}", net.name, instances[shallowest].id);
            diagnostics.push(
                Diagnostic::new(
                    DiagnosticKind::DuplicateNetName,
                    format!(
                        "the name '{}' is used by separate nets; this one is shown as '{renamed}'. \
                         Altium may merge same-named nets on the PCB",
                        net.name
                    ),
                )
                .on_sheet(&instances[shallowest].id)
                .net(&renamed),
            );
            net.name = renamed;
        }
        while !taken.insert(net_key(&net.name)) {
            net.name.push('\'');
        }
    }
    nets.sort_by(|a, b| a.name.cmp(&b.name));

    super::diagnose::diagnose(sheets, &local, &instances, &nets, &mut diagnostics);
    Netlist { scope, instances, nets, diagnostics }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::DiagnosticKind;
    use crate::schematic::testkit::*;

    /// "R1.1 R2.1" for the net with this name.
    fn pins(netlist: &Netlist, name: &str) -> String {
        let net = netlist
            .nets
            .iter()
            .find(|n| n.name == name)
            .unwrap_or_else(|| panic!("no net {name}; have {:?}", names(netlist)));
        net.pins.iter().map(|p| format!("{}.{}", p.designator, p.number)).collect::<Vec<_>>().join(" ")
    }

    fn names(netlist: &Netlist) -> Vec<&str> {
        netlist.nets.iter().map(|n| n.name.as_str()).collect()
    }

    /// Diagnostic kinds, leaving out the per-pin and per-wire findings of `diagnose`.
    fn kinds(netlist: &Netlist) -> Vec<DiagnosticKind> {
        netlist
            .diagnostics
            .iter()
            .map(|d| d.kind)
            .filter(|k| !matches!(k, DiagnosticKind::DanglingWire | DiagnosticKind::UnconnectedPin))
            .collect()
    }

    /// A sheet with one resistor pin wired to a point, for hanging names on.
    fn sheet_with_pin(file: &str, designator: &str) -> RawSheet {
        let mut s = sheet(file);
        s.components = vec![component(designator, &[("1", (0, 0))])];
        s.wires = vec![wire(&[(0, 0), (10, 0)])];
        s
    }

    #[test]
    fn an_unnamed_net_is_named_after_its_first_pin() {
        let mut s = sheet("a.SchDoc");
        s.components = vec![component("R2", &[("1", (0, 0))]), component("R1", &[("2", (10, 0))])];
        s.wires = vec![wire(&[(0, 0), (10, 0)])];
        let netlist = build(&[s], NetScope::Automatic);
        assert_eq!(names(&netlist), ["N$R1_2"]);
        assert!(netlist.nets[0].generated);
        assert_eq!(pins(&netlist, "N$R1_2"), "R1.2 R2.1");
    }

    #[test]
    fn power_names_win_over_labels_and_labels_over_ports() {
        let mut s = sheet_with_pin("a.SchDoc", "R1");
        s.ports = vec![port("P", (10, 0), (20, 0))];
        assert_eq!(names(&build(&[s.clone()], NetScope::Automatic)), ["P"]);
        s.labels = vec![label("L", (5, 0))];
        assert_eq!(names(&build(&[s.clone()], NetScope::Automatic)), ["L"]);
        s.power = vec![power("VCC", (0, 0))];
        let netlist = build(&[s], NetScope::Automatic);
        assert_eq!(names(&netlist), ["VCC"]);
        assert!(!netlist.nets[0].generated);
    }

    #[test]
    fn power_objects_join_across_sheets() {
        let mut a = sheet_with_pin("a.SchDoc", "R1");
        a.power = vec![power("GND", (10, 0))];
        let mut b = sheet_with_pin("b.SchDoc", "R2");
        b.power = vec![power("gnd", (10, 0))];
        let netlist = build(&[a, b], NetScope::Flat);
        assert_eq!(pins(&netlist, "GND"), "R1.1 R2.1");
        assert_eq!(netlist.nets[0].instances, [0, 1]);
    }

    #[test]
    fn strict_hierarchy_keeps_power_objects_local() {
        let mut a = sheet_with_pin("a.SchDoc", "R1");
        a.power = vec![power("GND", (10, 0))];
        let mut b = sheet_with_pin("b.SchDoc", "R2");
        b.power = vec![power("GND", (10, 0))];
        let netlist = build(&[a, b], NetScope::StrictHierarchical);
        assert_eq!(names(&netlist), ["GND@a", "GND@b"]);
    }

    #[test]
    fn labels_join_across_sheets_only_in_global_scope() {
        let mut a = sheet_with_pin("a.SchDoc", "R1");
        a.labels = vec![label("EN", (5, 0))];
        let mut b = sheet_with_pin("b.SchDoc", "R2");
        b.labels = vec![label("EN", (5, 0))];

        let global = build(&[a.clone(), b.clone()], NetScope::Global);
        assert_eq!(pins(&global, "EN"), "R1.1 R2.1");

        let flat = build(&[a, b], NetScope::Flat);
        assert_eq!(names(&flat), ["EN@a", "EN@b"]);
        assert_eq!(pins(&flat, "EN@a"), "R1.1");
        assert!(kinds(&flat).contains(&DiagnosticKind::DuplicateNetName));
    }

    #[test]
    fn ports_join_across_sheets_in_flat_scope() {
        let mut a = sheet_with_pin("a.SchDoc", "R1");
        a.ports = vec![port("CLK", (10, 0), (20, 0))];
        let mut b = sheet_with_pin("b.SchDoc", "R2");
        b.ports = vec![port("CLK", (10, 0), (20, 0))];
        let netlist = build(&[a, b], NetScope::Flat);
        assert_eq!(pins(&netlist, "CLK"), "R1.1 R2.1");
    }

    #[test]
    fn automatic_scope_follows_what_the_design_uses() {
        let plain = sheet_with_pin("a.SchDoc", "R1");
        assert_eq!(build(&[plain.clone()], NetScope::Automatic).scope, NetScope::Global);

        let mut with_port = plain.clone();
        with_port.ports = vec![port("X", (10, 0), (20, 0))];
        assert_eq!(build(&[with_port.clone()], NetScope::Automatic).scope, NetScope::Flat);

        let mut parent = plain;
        parent.symbols = vec![symbol("SUB", "b.SchDoc", &[("X", (10, 0))])];
        let mut child = sheet_with_pin("b.SchDoc", "R2");
        child.ports = vec![port("X", (10, 0), (20, 0))];
        assert_eq!(build(&[parent, child], NetScope::Automatic).scope, NetScope::Hierarchical);
    }

    #[test]
    fn a_child_port_joins_the_parent_sheet_entry() {
        let mut parent = sheet_with_pin("top.SchDoc", "R1");
        parent.symbols = vec![symbol("SUB", "child.SchDoc", &[("IN", (10, 0))])];
        parent.labels = vec![label("DATA", (5, 0))];
        let mut child = sheet_with_pin("child.SchDoc", "R2");
        child.ports = vec![port("IN", (10, 0), (20, 0))];

        let netlist = build(&[parent, child], NetScope::Automatic);
        assert_eq!(pins(&netlist, "DATA"), "R1.1 R2.1");
        let ids: Vec<&str> = netlist.instances.iter().map(|i| i.id.as_str()).collect();
        assert_eq!(ids, ["top", "top/SUB"]);
        assert_eq!(netlist.instances[1].parent, Some(0));
        assert!(kinds(&netlist).is_empty(), "{:?}", netlist.diagnostics);
    }

    #[test]
    fn the_file_name_in_a_sheet_symbol_is_matched_ignoring_case_and_folders() {
        let mut parent = sheet_with_pin("top.SchDoc", "R1");
        parent.symbols = vec![symbol("SUB", r"Sheets\CHILD.schdoc", &[("IN", (10, 0))])];
        let mut child = sheet_with_pin("/work/sheets/child.SchDoc", "R2");
        child.ports = vec![port("IN", (10, 0), (20, 0))];
        let netlist = build(&[parent, child], NetScope::Automatic);
        assert_eq!(pins(&netlist, "IN"), "R1.1 R2.1");
    }

    #[test]
    fn a_port_or_entry_without_a_partner_is_reported() {
        let mut parent = sheet_with_pin("top.SchDoc", "R1");
        parent.symbols = vec![symbol("SUB", "child.SchDoc", &[("IN", (10, 0))])];
        let mut child = sheet_with_pin("child.SchDoc", "R2");
        child.ports = vec![port("OUT", (10, 0), (20, 0))];
        let netlist = build(&[parent, child], NetScope::Automatic);
        assert_eq!(kinds(&netlist), [DiagnosticKind::UnmatchedPort, DiagnosticKind::UnmatchedPort]);
        let unmatched: Vec<Option<&str>> = netlist
            .diagnostics
            .iter()
            .filter(|d| d.kind == DiagnosticKind::UnmatchedPort)
            .map(|d| d.sheet.as_deref())
            .collect();
        assert_eq!(unmatched, [Some("top/SUB"), Some("top")]);
    }

    #[test]
    fn a_sheet_used_twice_gets_one_instance_per_symbol() {
        let mut parent = sheet("top.SchDoc");
        parent.components = vec![component("J1", &[("1", (0, 0)), ("2", (0, 50))])];
        parent.wires = vec![wire(&[(0, 0), (10, 0)]), wire(&[(0, 50), (10, 50)])];
        parent.symbols = vec![
            symbol("A", "child.SchDoc", &[("IN", (10, 0))]),
            symbol("B", "child.SchDoc", &[("IN", (10, 50))]),
        ];
        let mut child = sheet_with_pin("child.SchDoc", "R1");
        child.ports = vec![port("IN", (10, 0), (20, 0))];

        let netlist = build(&[parent, child], NetScope::Automatic);
        let ids: Vec<&str> = netlist.instances.iter().map(|i| i.id.as_str()).collect();
        assert_eq!(ids, ["top", "top/A", "top/B"]);
        let all: Vec<String> = netlist
            .nets
            .iter()
            .map(|n| n.pins.iter().map(|p| format!("{}.{}", p.designator, p.number)).collect::<Vec<_>>().join(" "))
            .collect();
        assert!(all.contains(&"J1.1 R1_A.1".to_string()), "{all:?}");
        assert!(all.contains(&"J1.2 R1_B.1".to_string()), "{all:?}");
    }

    #[test]
    fn a_sheet_symbol_pointing_at_a_missing_file_is_reported() {
        let mut parent = sheet_with_pin("top.SchDoc", "R1");
        parent.symbols = vec![symbol("SUB", "gone.SchDoc", &[("IN", (10, 0))])];
        let netlist = build(&[parent], NetScope::Automatic);
        assert_eq!(kinds(&netlist), [DiagnosticKind::MissingSheet]);
        assert_eq!(netlist.instances.len(), 1);
        assert_eq!(netlist.nets.len(), 1);
    }

    #[test]
    fn a_sheet_symbol_pointing_at_its_own_sheet_does_not_loop() {
        let mut only = sheet_with_pin("top.SchDoc", "R1");
        only.symbols = vec![symbol("SELF", "top.SchDoc", &[])];
        let netlist = build(&[only], NetScope::Automatic);
        assert_eq!(kinds(&netlist), [DiagnosticKind::HierarchyCycle]);
        assert_eq!(netlist.instances.len(), 1);
    }

    #[test]
    fn two_power_names_on_one_net_are_reported() {
        let mut s = sheet_with_pin("a.SchDoc", "R1");
        s.power = vec![power("3V3", (0, 0)), power("5V", (10, 0))];
        let netlist = build(&[s], NetScope::Automatic);
        assert_eq!(names(&netlist), ["3V3"]);
        assert_eq!(kinds(&netlist), [DiagnosticKind::MultipleNetNames]);
        let found = netlist.diagnostics.iter().find(|d| d.kind == DiagnosticKind::MultipleNetNames).unwrap();
        assert_eq!(found.nets, ["3V3"]);
    }

    #[test]
    fn groups_without_pins_are_not_nets() {
        let mut s = sheet("a.SchDoc");
        s.wires = vec![wire(&[(0, 0), (10, 0)])];
        s.labels = vec![label("FLOATING", (5, 0))];
        assert!(build(&[s], NetScope::Automatic).nets.is_empty());
    }

    #[test]
    fn nets_carry_their_wire_geometry_and_flags() {
        let mut s = sheet_with_pin("a.SchDoc", "R1");
        s.no_connects = vec![pt(0, 0)];
        s.bus_entries = vec![[pt(10, 0), pt(12, 2)]];
        let netlist = build(&[s], NetScope::Automatic);
        let net = &netlist.nets[0];
        assert_eq!(net.wires, [(0, vec![pt(0, 0), pt(10, 0)])]);
        assert!(net.no_connect);
        assert!(net.touches_bus);
    }

    #[test]
    fn no_sheets_gives_an_empty_netlist() {
        let netlist = build(&[], NetScope::Automatic);
        assert!(netlist.nets.is_empty() && netlist.instances.is_empty() && netlist.diagnostics.is_empty());
    }
}
