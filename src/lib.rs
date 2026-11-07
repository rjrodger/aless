//! aless, a jless-style terminal viewer for every format the tabnas
//! parsers read. The binary in `src/main.rs` owns the terminal; everything
//! here is terminal-free and unit tested.

pub mod app;
pub mod cli;
pub mod clip;
pub mod doc;
pub mod explorer;
pub mod fmt;
pub mod grammar;
pub mod highlight;
pub mod history;
pub mod load;
pub mod prov;
pub mod render;
pub mod search;
pub mod tab;
pub mod watch;
