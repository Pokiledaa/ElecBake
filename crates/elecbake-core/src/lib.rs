//! ElecBake core: Altium loading, netlist derivation and the design model.
pub mod altium;
pub mod design;
pub mod diagnostic;
pub mod error;
pub mod geom;
pub mod model;
pub mod netlist;
pub mod schematic;

pub use design::load_design;
pub use model::Design;
pub mod compare;
pub mod text;
