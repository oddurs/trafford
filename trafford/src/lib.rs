//! trafford as a library.
//!
//! The binary is one consumer of this. A library is worth having for a reason
//! beyond that: the one-way dependency direction this project relies on —
//! `vault` and `editor` know nothing about the UI — used to be a convention
//! held up by care, and a second consumer would make it a thing that fails to
//! compile when it is broken.
//!
//! `app`, `keymap`, `mouse` and `llm` are public because the binary needs
//! them, not because anything outside should. Every extra `pub` is API this
//! project then has to keep working, which is why the three renames in this
//! commit happened the day the modules became public rather than later.

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

#[cfg(test)]
mod testing;
