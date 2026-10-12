//! Derives electrical connectivity from drawn schematic objects.

pub mod build;
pub mod sheet;
pub mod union_find;

pub use build::{NetEntry, NetScope, Netlist, PinRef, SheetInstance, build};
pub use sheet::net_key;
