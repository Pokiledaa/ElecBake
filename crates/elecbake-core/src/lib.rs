//! ElecBake core: Altium loading, netlist derivation and the design model.
pub mod error;
pub mod geom;
pub mod schematic;
pub mod altium;
pub mod netlist;
pub mod diagnostic;
pub mod design;
pub mod model;

pub use design::load_design;
pub use model::Design;
