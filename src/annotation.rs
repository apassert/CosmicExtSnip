//! The annotations drawn over a snip, and their undo history.
//!
//! Pure: no toolkit, no display. Every coordinate is in **image pixels**, so
//! the on-screen editor and the exported PNG draw exactly the same thing
//! whatever the window's size or the display's scale.

use crate::config::{
    ARROW_HEAD_ANGLE, ARROW_HEAD_MIN, ARROW_HEAD_RATIO, HIGHLIGHT_ALPHA, MAX_STROKE_POINTS,
    MAX_UNDO_HISTORY,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Tool {
    Pen,
    Highlighter,
    Arrow,
    Rect,
}

impl Tool {
    pub const ALL: [Tool; 4] = [Tool::Pen, Tool::Highlighter, Tool::Arrow, Tool::Rect];

    /// CosmicSnip's single-key shortcuts: P, H, A, R.
    pub fn from_hotkey(c: char) -> Option<Tool> {
        match c.to_ascii_lowercase() {
            'p' => Some(Tool::Pen),
            'h' => Some(Tool::Highlighter),
            'a' => Some(Tool::Arrow),
            'r' => Some(Tool::Rect),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Tool::Pen => "Pen (P)",
            Tool::Highlighter => "Highlighter (H)",
            Tool::Arrow => "Arrow (A)",
            Tool::Rect => "Rectangle (R)",
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            Tool::Pen => "edit-symbolic",
            Tool::Highlighter => "format-text-highlight-symbolic",
            Tool::Arrow => "go-next-symbolic",
            Tool::Rect => "checkbox-symbolic",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

impl Point {
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Shape {
    /// Freehand: pen and highlighter.
    Path(Vec<Point>),
    /// A straight shaft with a head at `end`.
    Arrow {
        start: Point,
        end: Point,
    },
    Rect {
        start: Point,
        end: Point,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Stroke {
    pub shape: Shape,
    /// Straight RGBA; a highlighter's alpha is already reduced here.
    pub rgba: [f32; 4],
    pub width: f32,
}

impl Stroke {
    /// A stroke too small to see is not kept: a click without a drag should
    /// not leave an undo step that does nothing.
    fn is_visible(&self) -> bool {
        match &self.shape {
            Shape::Path(points) => points.len() >= 2,
            Shape::Arrow { start, end } | Shape::Rect { start, end } => start != end,
        }
    }
}

/// The two points the barbs of an arrowhead reach, for an arrow from `start`
/// to `end` drawn `width` wide. The same formula as CosmicSnip's `_draw_arrow`.
pub fn arrow_barbs(start: Point, end: Point, width: f32) -> [Point; 2] {
    let angle = (end.y - start.y).atan2(end.x - start.x);
    let head = ARROW_HEAD_MIN.max(width * ARROW_HEAD_RATIO);
    let barb = |a: f32| Point::new(end.x - head * a.cos(), end.y - head * a.sin());
    [
        barb(angle - ARROW_HEAD_ANGLE),
        barb(angle + ARROW_HEAD_ANGLE),
    ]
}

/// The strokes of one snip: the committed ones, and the one being drawn.
#[derive(Debug, Default)]
pub struct Document {
    strokes: Vec<Stroke>,
    current: Option<Stroke>,
}

impl Document {
    /// Start a stroke at `at` with the given tool, colour and width.
    pub fn begin(&mut self, tool: Tool, rgba: [f32; 4], width: f32, at: Point) {
        let rgba = if tool == Tool::Highlighter {
            [rgba[0], rgba[1], rgba[2], HIGHLIGHT_ALPHA]
        } else {
            rgba
        };
        let shape = match tool {
            Tool::Pen | Tool::Highlighter => Shape::Path(vec![at]),
            Tool::Arrow => Shape::Arrow { start: at, end: at },
            Tool::Rect => Shape::Rect { start: at, end: at },
        };
        self.current = Some(Stroke { shape, rgba, width });
    }

    /// Move the stroke being drawn: a freehand path gains a point, a line or
    /// rectangle moves its far end.
    pub fn extend(&mut self, at: Point) {
        let Some(stroke) = self.current.as_mut() else {
            return;
        };
        match &mut stroke.shape {
            Shape::Path(points) => {
                if points.len() < MAX_STROKE_POINTS && points.last() != Some(&at) {
                    points.push(at);
                }
            }
            Shape::Arrow { end, .. } | Shape::Rect { end, .. } => *end = at,
        }
    }

    /// Commit the stroke being drawn, if it is visible.
    pub fn finish(&mut self) {
        if let Some(stroke) = self.current.take() {
            if stroke.is_visible() {
                self.strokes.push(stroke);
                if self.strokes.len() > MAX_UNDO_HISTORY {
                    let excess = self.strokes.len() - MAX_UNDO_HISTORY;
                    self.strokes.drain(..excess);
                }
            }
        }
    }

    pub fn is_drawing(&self) -> bool {
        self.current.is_some()
    }

    /// Remove the last committed stroke. Returns whether there was one.
    pub fn undo(&mut self) -> bool {
        self.current = None;
        self.strokes.pop().is_some()
    }

    pub fn committed(&self) -> &[Stroke] {
        &self.strokes
    }

    /// Everything to draw, the stroke in progress last.
    pub fn all(&self) -> impl Iterator<Item = &Stroke> {
        self.strokes.iter().chain(self.current.iter())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RED: [f32; 4] = [0.93, 0.16, 0.16, 1.0];

    #[test]
    fn a_drag_is_one_undo_step() {
        let mut doc = Document::default();
        doc.begin(Tool::Pen, RED, 3.0, Point::new(1.0, 1.0));
        doc.extend(Point::new(5.0, 5.0));
        doc.extend(Point::new(9.0, 2.0));
        doc.finish();
        assert_eq!(doc.committed().len(), 1);
        assert!(doc.undo());
        assert!(doc.committed().is_empty());
        assert!(!doc.undo(), "nothing left to undo");
    }

    #[test]
    fn a_click_without_a_drag_leaves_nothing_behind() {
        let mut doc = Document::default();
        for tool in Tool::ALL {
            doc.begin(tool, RED, 3.0, Point::new(4.0, 4.0));
            doc.finish();
        }
        assert!(doc.committed().is_empty());
    }

    #[test]
    fn the_highlighter_is_translucent_and_the_pen_is_not() {
        let mut doc = Document::default();
        doc.begin(Tool::Highlighter, RED, 20.0, Point::new(0.0, 0.0));
        doc.extend(Point::new(10.0, 0.0));
        doc.finish();
        doc.begin(Tool::Pen, RED, 3.0, Point::new(0.0, 0.0));
        doc.extend(Point::new(10.0, 0.0));
        doc.finish();
        assert_eq!(doc.committed()[0].rgba[3], HIGHLIGHT_ALPHA);
        assert_eq!(doc.committed()[1].rgba[3], 1.0);
    }

    #[test]
    fn a_rectangle_keeps_only_its_last_corner() {
        let mut doc = Document::default();
        doc.begin(Tool::Rect, RED, 3.0, Point::new(2.0, 2.0));
        doc.extend(Point::new(8.0, 8.0));
        doc.extend(Point::new(12.0, 6.0));
        doc.finish();
        assert_eq!(
            doc.committed()[0].shape,
            Shape::Rect {
                start: Point::new(2.0, 2.0),
                end: Point::new(12.0, 6.0)
            }
        );
    }

    #[test]
    fn the_history_is_bounded() {
        let mut doc = Document::default();
        for i in 0..(MAX_UNDO_HISTORY + 5) {
            doc.begin(Tool::Arrow, RED, 3.0, Point::new(0.0, 0.0));
            doc.extend(Point::new(i as f32 + 1.0, 0.0));
            doc.finish();
        }
        assert_eq!(doc.committed().len(), MAX_UNDO_HISTORY);
    }

    #[test]
    fn an_arrowhead_points_back_along_the_shaft() {
        // A shaft pointing right: both barbs sit to the left of the tip, one
        // above and one below, at the minimum head length for a thin stroke.
        let [a, b] = arrow_barbs(Point::new(0.0, 0.0), Point::new(100.0, 0.0), 1.0);
        assert!(a.x < 100.0 && b.x < 100.0);
        assert!((a.y + b.y).abs() < 1e-4, "symmetric about the shaft");
        let len = ((100.0 - a.x).powi(2) + a.y.powi(2)).sqrt();
        assert!((len - ARROW_HEAD_MIN).abs() < 1e-3);
    }

    #[test]
    fn hotkeys_match_cosmicsnip() {
        assert_eq!(Tool::from_hotkey('P'), Some(Tool::Pen));
        assert_eq!(Tool::from_hotkey('h'), Some(Tool::Highlighter));
        assert_eq!(Tool::from_hotkey('a'), Some(Tool::Arrow));
        assert_eq!(Tool::from_hotkey('r'), Some(Tool::Rect));
        assert_eq!(Tool::from_hotkey('x'), None);
    }
}
