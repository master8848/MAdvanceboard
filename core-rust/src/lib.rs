//! `kbcore`: configurable predictive text engine core (layout-driven, t9-9 default; no Android deps).

pub mod inflect;
pub mod layout;
pub mod mapping;
pub mod pack;
pub mod palette;
pub mod personal;
pub mod predictor;
pub mod rank;
pub mod session;
pub mod snippet;
pub mod stack;
pub mod store;
pub mod gates;

pub use layout::{KeyMapping, LayoutRegistry, LayoutSpec, DEFAULT_LAYOUT_ID};
pub use predictor::Predictor;
pub use stack::{DictionaryStack, NextSuggestion, SuggestOpts, Suggestion, TargetProbe};

uniffi::setup_scaffolding!("kbcore");
