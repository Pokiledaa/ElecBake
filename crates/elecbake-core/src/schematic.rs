//! A schematic sheet as drawn: objects and positions, before connectivity is derived.
//! These are ElecBake's own types; the `altium` module converts into them.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::geom::{Point, Rect};

#[derive(Debug, Clone, Default)]
pub struct RawSheet {
    pub path: PathBuf,
    pub components: Vec<RawComponent>,
    pub wires: Vec<RawWire>,
    pub junctions: Vec<Point>,
    pub labels: Vec<RawLabel>,
    pub power: Vec<RawPower>,
    pub ports: Vec<RawPort>,
    pub symbols: Vec<RawSheetSymbol>,
    pub no_connects: Vec<Point>,
    /// Both ends of each bus entry. Buses are not expanded; these mark nets that touch one.
    pub bus_entries: Vec<[Point; 2]>,
    pub bus_count: usize,
    pub harness_count: usize,
}

#[derive(Debug, Clone, Default)]
pub struct RawComponent {
    pub designator: String,
    /// Which part of a multi-part component this placement is.
    pub part_id: i32,
    pub lib_reference: String,
    pub description: Option<String>,
    /// The resolved "Comment" parameter, usually the value or part number.
    pub comment: Option<String>,
    pub footprint: Option<String>,
    pub parameters: Vec<(String, String)>,
    pub location: Point,
    pub pins: Vec<RawPin>,
}

#[derive(Debug, Clone, Default)]
pub struct RawPin {
    pub number: String,
    pub name: String,
    pub kind: PinKind,
    /// Where the pin meets the component body.
    pub base: Point,
    /// The electrical end, where wires attach.
    pub hot: Point,
    /// For a hidden pin, the net it is tied to by name.
    pub hidden_net: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PinKind {
    Input,
    InputOutput,
    Output,
    OpenCollector,
    #[default]
    Passive,
    HiZ,
    OpenEmitter,
    Power,
}

#[derive(Debug, Clone, Default)]
pub struct RawWire {
    pub vertices: Vec<Point>,
}

#[derive(Debug, Clone, Default)]
pub struct RawLabel {
    pub text: String,
    pub at: Point,
}

#[derive(Debug, Clone, Default)]
pub struct RawPower {
    pub name: String,
    pub at: Point,
}

#[derive(Debug, Clone, Default)]
pub struct RawPort {
    pub name: String,
    /// A wire may attach at either end.
    pub ends: [Point; 2],
}

#[derive(Debug, Clone, Default)]
pub struct RawSheetSymbol {
    pub name: String,
    /// File name of the child sheet, as written in the parent.
    pub file_name: String,
    pub rect: Rect,
    pub entries: Vec<RawEntry>,
}

#[derive(Debug, Clone, Default)]
pub struct RawEntry {
    pub name: String,
    pub at: Point,
}

/// Small builders for hand-made sheets in unit tests.
#[cfg(test)]
pub(crate) mod testkit {
    use super::*;

    pub fn pt(x: i64, y: i64) -> Point {
        Point::new(x, y)
    }

    pub fn sheet(file: &str) -> RawSheet {
        RawSheet {
            path: file.into(),
            ..Default::default()
        }
    }

    pub fn wire(points: &[(i64, i64)]) -> RawWire {
        RawWire {
            vertices: points.iter().map(|&(x, y)| pt(x, y)).collect(),
        }
    }

    /// A component whose pins are given as (number, hot-spot position).
    pub fn component(designator: &str, pins: &[(&str, (i64, i64))]) -> RawComponent {
        RawComponent {
            designator: designator.into(),
            part_id: 1,
            pins: pins
                .iter()
                .map(|&(number, (x, y))| RawPin {
                    number: number.into(),
                    name: number.into(),
                    base: pt(x, y),
                    hot: pt(x, y),
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        }
    }

    pub fn label(text: &str, at: (i64, i64)) -> RawLabel {
        RawLabel {
            text: text.into(),
            at: pt(at.0, at.1),
        }
    }

    pub fn power(name: &str, at: (i64, i64)) -> RawPower {
        RawPower {
            name: name.into(),
            at: pt(at.0, at.1),
        }
    }

    pub fn port(name: &str, a: (i64, i64), b: (i64, i64)) -> RawPort {
        RawPort {
            name: name.into(),
            ends: [pt(a.0, a.1), pt(b.0, b.1)],
        }
    }

    pub fn symbol(name: &str, file: &str, entries: &[(&str, (i64, i64))]) -> RawSheetSymbol {
        RawSheetSymbol {
            name: name.into(),
            file_name: file.into(),
            rect: Rect::default(),
            entries: entries
                .iter()
                .map(|&(n, (x, y))| RawEntry {
                    name: n.into(),
                    at: pt(x, y),
                })
                .collect(),
        }
    }
}
