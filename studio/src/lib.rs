//! Studio: native front end for the local ComfyUI install (image generation, photo edits, 2x upscale,
//! LTX-2.5 video, LatentSync lip sync). ComfyUI itself is the only Python part: it runs as a background
//! process that this crate starts on demand and stops when idle.
pub mod api;
pub mod engine;
pub mod jobs;
pub mod media;
pub mod models;
pub mod paths;
pub mod results;
pub mod translate;
pub mod ui;
pub mod workflows;

/// Port of the local control API (the window and `studio-cli serve` both listen here; Claude Code talks to it).
pub const API_PORT: u16 = 8189;
