//! trafford as a library.
//!
//! The binary is one consumer of this; the docs site in `site/` is the other.
//! A second consumer is worth having for a reason beyond the site: the one-way
//! dependency direction this project relies on — `vault` and `editor` know
//! nothing about the UI — used to be a convention, and is now a thing that
//! fails to compile when it is broken.
//!
//! What the site actually reaches for is `vault`, `ui::fold`, `ui::table`,
//! `ui::callout`, `ui::markdown::scan` and `ui::theme`. The rest is here
//! because the binary needs it, not because anything outside should.

pub mod app;
pub mod clipboard;
pub mod config;
pub mod editor;
pub mod git;
pub mod keymap;
pub mod layout;
pub mod llm;
pub mod mouse;
pub mod tree;
pub mod ui;
pub mod vault;
pub mod watch;

/// Test helpers, shared with the workspace's other member.
///
/// Behind a feature rather than `#[cfg(test)]`: `cfg(test)` only exists while
/// *this* crate's tests build, and `site` links the ordinary library. Nothing
/// enables the feature except a dev-dependency, so the shipped binary does not
/// carry it.
#[cfg(any(test, feature = "testing"))]
pub mod testing;
