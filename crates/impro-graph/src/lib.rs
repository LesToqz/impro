//! Engine of impro's Graph view: parsing, project index, filters, preferences
//! and force layout. This crate has no GUI dependencies; the view modules that
//! draw the graph live next to it and consume the types below.
//!
//! # Data flow
//!
//! ```text
//! files ──scan──▶ ProjectIndex ──build_graph(GraphOptions)──▶ Graph ──▶ Layout ──▶ x/y/radius arrays
//!                   ▲  update_file / remove / rename            │
//!                   └── IndexChange                  delta::diff(old, new) ─▶ GraphDelta
//! ```
//!
//! * [`parse`] turns text into links, tags, properties and source imports.
//! * [`index`] owns the files, resolves references and updates incrementally.
//! * [`query`] is the search language; [`filter`] applies it together with the
//!   visibility toggles, colour rules, the node cap and the local view.
//! * [`prefs`] normalises the persisted preferences blob.
//! * [`layout`] is the force simulation; [`geometry`] holds the drawing maths.

pub mod delta;
pub mod filter;
pub mod geometry;
pub mod index;
pub mod layout;
pub mod model;
pub mod parse;
pub mod prefs;
pub mod query;
pub mod view;

pub use delta::{diff, GraphDelta};
pub use filter::{build, local, GraphOptions, LocalGraph};
pub use index::{CancelToken, IndexChange, ProjectIndex, ScanConfig};
pub use layout::{ForceParams, Layout};
pub use model::{Graph, Node, NodeKind};
pub use prefs::{normalize, GraphPrefs};
