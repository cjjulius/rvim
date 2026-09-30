//! rvim — a modular, vim-emulating terminal text editor.
//!
//! The crate is split into small, focused modules so that new components
//! (languages, themes, plugins, motions) can be added with minimal coupling:
//!
//! - [`buffer`]   — the text storage + edit primitives (with undo/redo).
//! - [`config`]   — startup `~/.rvimrc` loading + `:source`.
//! - [`mode`]     — the modal state machine (Normal / Insert / Visual / Command).
//! - [`editor`]   — cursor, viewport and high-level editing operations.
//! - [`command`]  — the `:` ex-command parser/dispatcher.
//! - [`syntax`]   — language autodetection + pluggable highlighters.
//! - [`theme`]    — color themes (matrix, retrowave, cobalt, …).
//! - [`plugin`]   — the plugin trait + manager for extensibility.
//! - [`terminal`] — raw-mode / alternate-screen RAII guard (cross-platform).
//! - [`ui`]       — rendering of the text area, gutter and status line.
//! - [`app`]      — wires everything together and runs the event loop.

pub mod app;
pub mod buffer;
pub mod command;
pub mod config;
pub mod editor;
pub mod mode;
pub mod pattern;
pub mod plugin;
pub mod syntax;
pub mod terminal;
pub mod theme;
pub mod ui;

/// The current rvim version, surfaced by `--version` and `:version`.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
