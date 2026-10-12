//! Loads a design from an Altium project or schematic file.

use std::collections::{HashSet, VecDeque};
use std::path::{Path, PathBuf};

use crate::altium::{self, ProjectHierarchy};
use crate::diagnostic::{Diagnostic, DiagnosticKind};
use crate::error::{Error, Result};
use crate::model::{Design, assemble};
use crate::netlist::{NetScope, build};
use crate::schematic::RawSheet;

fn extension(path: &Path) -> String {
    path.extension()
        .map(|ext| ext.to_string_lossy().to_lowercase())
        .unwrap_or_default()
}

fn file_key(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().to_lowercase())
        .unwrap_or_default()
}

/// Load an Altium project (`.PrjPcb`) or a schematic (`.SchDoc`) with its child sheets.
pub fn load_design(path: &Path) -> Result<Design> {
    let mut diagnostics: Vec<Diagnostic> = Vec::new();
    let (name, files, scope, is_project) = match extension(path).as_str() {
        "prjpcb" => {
            let project = altium::load_project(path)?;
            for missing in &project.missing {
                if missing.to_lowercase().ends_with(".schdoc") {
                    diagnostics.push(Diagnostic::new(
                        DiagnosticKind::SheetLoadFailed,
                        format!("{missing}: listed in the project but not found on disk"),
                    ));
                }
            }
            let scope = match project.hierarchy {
                ProjectHierarchy::Automatic => NetScope::Automatic,
                ProjectHierarchy::Flat => NetScope::Flat,
                ProjectHierarchy::Hierarchical => NetScope::Hierarchical,
                ProjectHierarchy::StrictHierarchical => NetScope::StrictHierarchical,
                ProjectHierarchy::Global => NetScope::Global,
            };
            (project.name, project.schematics, scope, true)
        }
        "schdoc" => {
            let name = path
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            (name, vec![path.to_path_buf()], NetScope::Automatic, false)
        }
        _ => return Err(Error::Unsupported(path.to_path_buf())),
    };

    let mut sheets: Vec<RawSheet> = Vec::new();
    let mut first_error: Option<Error> = None;
    let mut queue: VecDeque<PathBuf> = files.into();
    let mut seen: HashSet<String> = HashSet::new();

    while let Some(file) = queue.pop_front() {
        if !seen.insert(file_key(&file)) {
            continue;
        }
        match altium::load_sheet(&file) {
            Ok(sheet) => {
                let dir = file.parent().unwrap_or(Path::new("."));
                for symbol in &sheet.symbols {
                    if let Some(child) = altium::resolve(dir, &symbol.file_name) {
                        queue.push_back(child);
                    }
                }
                sheets.push(sheet);
            }
            Err(error) => {
                diagnostics.push(Diagnostic::new(
                    DiagnosticKind::SheetLoadFailed,
                    error.to_string(),
                ));
                first_error.get_or_insert(error);
            }
        }
    }

    if sheets.is_empty() && !is_project {
        return Err(first_error.unwrap_or_else(|| Error::Unsupported(path.to_path_buf())));
    }

    let netlist = build(&sheets, scope);
    Ok(assemble(
        name,
        path.to_path_buf(),
        &sheets,
        netlist,
        diagnostics,
    ))
}
