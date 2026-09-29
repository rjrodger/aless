//! aless — a jless-style terminal viewer for every format the tabnas
//! parsers read, with tabs and a watch mode that reloads a changed file
//! while keeping your place.
//!
//! The binary in `main.rs` owns the terminal; everything here is
//! terminal-free and unit tested.

pub mod alchemy;
pub mod app;
pub mod clip;
pub mod doc;
pub mod explorer;
pub mod export;
pub mod fmt;
pub mod grammar;
pub mod headless;
pub mod highlight;
pub mod load;
pub mod pane;
pub mod prov;
pub mod render;
pub mod search;
pub mod tab;
pub mod translate;
pub mod watch;
