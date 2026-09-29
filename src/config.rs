//! Tools, colours and limits, ported value for value from CosmicSnip's
//! `cosmicsnip/config.py` and `editor.py`.

/// A colour in the annotation palette, straight (not premultiplied) RGBA.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ToolColor {
    pub name: &'static str,
    pub label: &'static str,
    pub rgba: [f32; 4],
}

pub const PALETTE: [ToolColor; 6] = [
    ToolColor {
        name: "red",
        label: "Red",
        rgba: [0.93, 0.16, 0.16, 1.0],
    },
    ToolColor {
        name: "orange",
        label: "Orange",
        rgba: [0.96, 0.52, 0.10, 1.0],
    },
    ToolColor {
        name: "blue",
        label: "Blue",
        rgba: [0.15, 0.45, 0.93, 1.0],
    },
    ToolColor {
        name: "green",
        label: "Green",
        rgba: [0.20, 0.72, 0.30, 1.0],
    },
    ToolColor {
        name: "black",
        label: "Black",
        rgba: [0.12, 0.12, 0.12, 1.0],
    },
    ToolColor {
        name: "white",
        label: "White",
        rgba: [1.00, 1.00, 1.00, 1.0],
    },
];

pub const DEFAULT_PEN_WIDTH: f32 = 3.0;
pub const PEN_WIDTH_MIN: f32 = 1.0;
pub const PEN_WIDTH_MAX: f32 = 20.0;

pub const DEFAULT_HIGHLIGHT_WIDTH: f32 = 20.0;
pub const HIGHLIGHT_WIDTH_MIN: f32 = 4.0;
pub const HIGHLIGHT_WIDTH_MAX: f32 = 60.0;
/// The highlighter paints the chosen colour at this opacity.
pub const HIGHLIGHT_ALPHA: f32 = 0.35;

/// Half-angle between the shaft and each barb of an arrowhead, in radians.
pub const ARROW_HEAD_ANGLE: f32 = 0.45;
/// Barb length as a multiple of the stroke width, never shorter than the minimum.
pub const ARROW_HEAD_RATIO: f32 = 5.0;
pub const ARROW_HEAD_MIN: f32 = 14.0;

pub const MAX_UNDO_HISTORY: usize = 200;
pub const MAX_STROKE_POINTS: usize = 10_000;

/// Larger than this is refused rather than allocated: 16K by 8K.
pub const MAX_IMAGE_WIDTH: u32 = 15_360;
pub const MAX_IMAGE_HEIGHT: u32 = 8_640;

/// Where Ctrl+S offers to save, as CosmicSnip did: `~/Pictures/screenshots`.
pub fn save_dir() -> std::path::PathBuf {
    dirs::picture_dir()
        .unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join("Pictures"))
        .join("screenshots")
}
