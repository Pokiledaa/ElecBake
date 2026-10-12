//! Findings about a design that the user and the AI should both see.

use serde::{Deserialize, Serialize};

use crate::geom::Point;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticKind {
    SheetLoadFailed,
    MissingSheet,
    HierarchyCycle,
    RepeatedSheet,
    UnmatchedPort,
    UnconnectedPin,
    DanglingWire,
    DuplicateDesignator,
    MultipleNetNames,
    DuplicateNetName,
    BusNotExpanded,
    OffSheetNotFollowed,
    DuplicatePinNumber,
    ProjectOptionsIgnored,
}

impl DiagnosticKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SheetLoadFailed => "sheet_load_failed",
            Self::MissingSheet => "missing_sheet",
            Self::HierarchyCycle => "hierarchy_cycle",
            Self::RepeatedSheet => "repeated_sheet",
            Self::UnmatchedPort => "unmatched_port",
            Self::UnconnectedPin => "unconnected_pin",
            Self::DanglingWire => "dangling_wire",
            Self::DuplicateDesignator => "duplicate_designator",
            Self::MultipleNetNames => "multiple_net_names",
            Self::DuplicateNetName => "duplicate_net_name",
            Self::BusNotExpanded => "bus_not_expanded",
            Self::OffSheetNotFollowed => "off_sheet_not_followed",
            Self::DuplicatePinNumber => "duplicate_pin_number",
            Self::ProjectOptionsIgnored => "project_options_ignored",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostic {
    pub kind: DiagnosticKind,
    pub message: String,
    /// Sheet instance id, when the finding belongs to one sheet.
    pub sheet: Option<String>,
    pub at: Option<Point>,
    pub designators: Vec<String>,
    pub nets: Vec<String>,
}

impl Diagnostic {
    pub fn new(kind: DiagnosticKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            sheet: None,
            at: None,
            designators: Vec::new(),
            nets: Vec::new(),
        }
    }

    pub fn on_sheet(mut self, id: &str) -> Self {
        self.sheet = Some(id.to_string());
        self
    }

    pub fn at(mut self, point: Point) -> Self {
        self.at = Some(point);
        self
    }

    pub fn designator(mut self, designator: &str) -> Self {
        self.designators.push(designator.to_string());
        self
    }

    pub fn net(mut self, name: &str) -> Self {
        self.nets.push(name.to_string());
        self
    }
}
