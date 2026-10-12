use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use elecbake_core::{altium, load_design, text};

#[derive(Parser)]
#[command(name = "elecbake", version, about = "AI copilot for electronic circuit design")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Print the netlist derived from an Altium project (.PrjPcb) or schematic (.SchDoc)
    Netlist {
        path: PathBuf,
        #[arg(long, value_enum, default_value_t = Format::Text)]
        format: Format,
    },
    /// Render every schematic sheet of a project or schematic to SVG
    Render {
        path: PathBuf,
        /// Directory to write the SVG files into
        #[arg(short, long, default_value = ".")]
        out: PathBuf,
    },
}

#[derive(Clone, Copy, ValueEnum)]
enum Format {
    Text,
    Json,
}

fn netlist(path: &Path, format: Format) -> Result<ExitCode> {
    let design = load_design(path)?;
    match format {
        Format::Text => print!("{}", text::netlist_text(&design)),
        Format::Json => println!("{}", serde_json::to_string_pretty(&design)?),
    }
    Ok(ExitCode::SUCCESS)
}

fn render(path: &Path, out: &Path) -> Result<ExitCode> {
    let design = load_design(path)?;
    std::fs::create_dir_all(out).with_context(|| format!("cannot create {}", out.display()))?;

    // A sheet file used by several sheet symbols is rendered once.
    let files: BTreeSet<&PathBuf> = design.sheets.iter().map(|sheet| &sheet.path).collect();
    let mut failed = false;
    for file in files {
        let stem = file.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        let target = out.join(format!("{stem}.svg"));
        match altium::render_sheet_svg(file) {
            Ok(svg) => {
                std::fs::write(&target, svg).with_context(|| format!("cannot write {}", target.display()))?;
                println!("{}", target.display());
            }
            Err(error) => {
                eprintln!("error: {error}");
                failed = true;
            }
        }
    }
    Ok(if failed { ExitCode::FAILURE } else { ExitCode::SUCCESS })
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match &cli.command {
        Command::Netlist { path, format } => netlist(path, *format),
        Command::Render { path, out } => render(path, out),
    };
    result.unwrap_or_else(|error| {
        eprintln!("error: {error:#}");
        ExitCode::FAILURE
    })
}
