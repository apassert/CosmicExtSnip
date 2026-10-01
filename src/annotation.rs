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
    /// An ellipse inside the rectangle that was dragged.
    Circle,
    Text,
}

impl Tool {
    pub const ALL: [Tool; 6] = [
        Tool::Pen,
        Tool::Highlighter,
        Tool::Arrow,
        Tool::Rect,
        Tool::Circle,
        Tool::Text,
    ];

    /// CosmicSnip's single-key shortcuts: P, H, A, R.
    pub fn from_hotkey(c: char) -> Option<Tool> {
        match c.to_ascii_lowercase() {
            'p' => Some(Tool::Pen),
            'h' => Some(Tool::Highlighter),
            'a' => Some(Tool::Arrow),
            'r' => Some(Tool::Rect),
            'c' => Some(Tool::Circle),
            't' => Some(Tool::Text),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Tool::Pen => "Pen (P)",
            Tool::Highlighter => "Highlighter (H)",
            Tool::Arrow => "Arrow (A)",
            Tool::Rect => "Rectangle (R)",
            Tool::Circle => "Circle (C)",
            Tool::Text => "Text (T)",
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            Tool::Pen => "edit-symbolic",
            Tool::Highlighter => "format-text-highlight-symbolic",
            Tool::Arrow => "go-next-symbolic",
            Tool::Rect => "checkbox-symbolic",
            Tool::Circle => "radio-symbolic",
            Tool::Text => "insert-text-symbolic",
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
    /// The ellipse inscribed in the rectangle from `start` to `end`.
    Ellipse {
        start: Point,
        end: Point,
    },
    /// One line of text whose top-left corner is `at`, `size` snip pixels high,
    /// in the desktop's interface font.
    Text {
        at: Point,
        text: String,
        size: f32,
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
            Shape::Arrow { start, end }
            | Shape::Rect { start, end }
            | Shape::Ellipse { start, end } => start != end,
            Shape::Text { text, .. } => !text.trim().is_empty(),
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
    /// The freehand stroke in progress was straightened: it is now a line whose
    /// far end follows the pointer.
    straight: bool,
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
            Tool::Circle => Shape::Ellipse { start: at, end: at },
            // For text, the width is the text's height in snip pixels.
            Tool::Text => Shape::Text {
                at,
                text: String::new(),
                size: width,
            },
        };
        self.current = Some(Stroke { shape, rgba, width });
        self.straight = false;
    }

    /// Hold still while drawing freehand, and the stroke becomes a straight line
    /// from where it started to where the pointer is - as the Windows Snipping
    /// Tool does. Returns whether it changed.
    pub fn straighten(&mut self) -> bool {
        let Some(Stroke {
            shape: Shape::Path(points),
            ..
        }) = self.current.as_mut()
        else {
            return false;
        };
        if self.straight || points.len() < 2 {
            return false;
        }
        let (first, last) = (points[0], points[points.len() - 1]);
        if first == last {
            return false;
        }
        *points = vec![first, last];
        self.straight = true;
        true
    }

    pub fn is_straight(&self) -> bool {
        self.straight
    }

    /// Move the stroke being drawn: a freehand path gains a point, a line or
    /// rectangle moves its far end.
    pub fn extend(&mut self, at: Point) {
        let Some(stroke) = self.current.as_mut() else {
            return;
        };
        match &mut stroke.shape {
            // A straightened stroke keeps its start and moves its end.
            Shape::Path(points) if self.straight => {
                if let Some(end) = points.last_mut() {
                    *end = at;
                }
            }
            Shape::Path(points) => {
                if points.len() < MAX_STROKE_POINTS && points.last() != Some(&at) {
                    points.push(at);
                }
            }
            Shape::Arrow { end, .. } | Shape::Rect { end, .. } | Shape::Ellipse { end, .. } => {
                *end = at
            }
            // Text is placed by a click, not dragged.
            Shape::Text { .. } => {}
        }
    }

    /// Commit the stroke being drawn, if it is visible.
    pub fn finish(&mut self) {
        self.straight = false;
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

    /// Text is being typed into the stroke in progress.
    pub fn is_typing(&self) -> bool {
        matches!(
            self.current,
            Some(Stroke {
                shape: Shape::Text { .. },
                ..
            })
        )
    }

    /// Type into the text being written. Control characters are not text.
    pub fn type_text(&mut self, typed: &str) {
        if let Some(Stroke {
            shape: Shape::Text { text, .. },
            ..
        }) = self.current.as_mut()
        {
            text.extend(typed.chars().filter(|c| !c.is_control()));
        }
    }

    pub fn backspace(&mut self) {
        if let Some(Stroke {
            shape: Shape::Text { text, .. },
            ..
        }) = self.current.as_mut()
        {
            text.pop();
        }
    }

    /// Drop the stroke in progress without keeping it.
    pub fn cancel(&mut self) {
        self.current = None;
        self.straight = false;
    }

    pub fn is_drawing(&self) -> bool {
        self.current.is_some()
    }

    /// Remove the last committed stroke. Returns whether there was one.
    pub fn undo(&mut self) -> bool {
        self.current = None;
        self.straight = false;
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

/// Whether the pointer has been held still while drawing. Time comes in from
/// outside, so the rule is tested without a clock.
#[derive(Debug, Default)]
pub struct Hold {
    at: Option<(Point, std::time::Instant)>,
}

impl Hold {
    /// The pointer is at `at` now. Movement within the jitter radius is not
    /// movement: a hand holding a mouse still still trembles a pixel or two.
    pub fn moved(&mut self, at: Point, now: std::time::Instant) {
        let still = self.at.is_some_and(|(p, _)| {
            ((p.x - at.x).powi(2) + (p.y - at.y).powi(2)).sqrt() <= crate::config::STRAIGHTEN_JITTER
        });
        if !still {
            self.at = Some((at, now));
        }
    }

    pub fn held(&self, now: std::time::Instant) -> bool {
        self.at
            .is_some_and(|(_, since)| now.duration_since(since) >= crate::config::STRAIGHTEN_AFTER)
    }

    pub fn reset(&mut self) {
        self.at = None;
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

    #[test]
    fn holding_still_straightens_a_pen_stroke_from_its_start() {
        let mut d = Document::default();
        d.begin(Tool::Pen, [1.0, 0.0, 0.0, 1.0], 3.0, Point::new(0.0, 0.0));
        for (x, y) in [(3.0, 5.0), (8.0, 2.0), (20.0, 10.0)] {
            d.extend(Point::new(x, y));
        }
        assert!(d.straighten());
        assert!(d.is_straight());
        assert_eq!(
            d.all().last().unwrap().shape,
            Shape::Path(vec![Point::new(0.0, 0.0), Point::new(20.0, 10.0)])
        );
        // After that the line's end follows the pointer; it gains no points.
        d.extend(Point::new(40.0, 0.0));
        assert_eq!(
            d.all().last().unwrap().shape,
            Shape::Path(vec![Point::new(0.0, 0.0), Point::new(40.0, 0.0)])
        );
        d.finish();
        assert_eq!(d.committed().len(), 1);
        assert!(!d.is_straight());
    }

    #[test]
    fn only_a_freehand_stroke_that_went_somewhere_is_straightened() {
        let mut d = Document::default();
        d.begin(Tool::Pen, [1.0; 4], 3.0, Point::new(5.0, 5.0));
        assert!(!d.straighten(), "a dot has no direction");
        d.begin(Tool::Arrow, [1.0; 4], 3.0, Point::new(0.0, 0.0));
        d.extend(Point::new(9.0, 9.0));
        assert!(!d.straighten(), "an arrow is straight already");
        let mut d = Document::default();
        assert!(!d.straighten(), "nothing is being drawn");
    }

    #[test]
    fn a_hold_is_a_second_without_moving_beyond_the_jitter() {
        let t0 = std::time::Instant::now();
        let ms = |n: u64| t0 + std::time::Duration::from_millis(n);
        let mut h = Hold::default();
        h.moved(Point::new(0.0, 0.0), ms(0));
        h.moved(Point::new(1.5, 1.0), ms(600)); // a tremble, not a move
        assert!(!h.held(ms(900)));
        assert!(h.held(ms(1000)));
        h.moved(Point::new(10.0, 0.0), ms(1100)); // a real move starts the clock again
        assert!(!h.held(ms(1500)));
        assert!(h.held(ms(2100)));
        h.reset();
        assert!(!h.held(ms(5000)));
    }

    #[test]
    fn a_circle_is_the_ellipse_in_the_dragged_rectangle() {
        let mut d = Document::default();
        d.begin(
            Tool::Circle,
            [0.0, 0.0, 1.0, 1.0],
            3.0,
            Point::new(10.0, 10.0),
        );
        d.extend(Point::new(50.0, 30.0));
        d.finish();
        assert_eq!(
            d.committed()[0].shape,
            Shape::Ellipse {
                start: Point::new(10.0, 10.0),
                end: Point::new(50.0, 30.0)
            }
        );
    }

    #[test]
    fn text_is_typed_corrected_and_committed_and_empty_text_is_not_kept() {
        let mut d = Document::default();
        d.begin(Tool::Text, [1.0, 0.0, 0.0, 1.0], 24.0, Point::new(5.0, 5.0));
        assert!(d.is_typing());
        d.type_text("Helo");
        d.backspace();
        d.type_text("lo!\u{8}");
        d.extend(Point::new(99.0, 99.0)); // a drag does not move text
        d.finish();
        assert!(!d.is_typing());
        assert_eq!(
            d.committed()[0].shape,
            Shape::Text {
                at: Point::new(5.0, 5.0),
                text: "Hello!".into(),
                size: 24.0
            }
        );
        d.begin(Tool::Text, [1.0; 4], 24.0, Point::new(0.0, 0.0));
        d.type_text("   ");
        d.finish();
        assert_eq!(d.committed().len(), 1, "blank text leaves no undo step");
    }

    #[test]
    fn cancelling_text_keeps_nothing() {
        let mut d = Document::default();
        d.begin(Tool::Text, [1.0; 4], 24.0, Point::new(0.0, 0.0));
        d.type_text("abc");
        d.cancel();
        assert!(!d.is_drawing());
        assert!(d.committed().is_empty());
    }

    #[test]
    fn the_new_tools_have_hotkeys() {
        assert_eq!(Tool::from_hotkey('c'), Some(Tool::Circle));
        assert_eq!(Tool::from_hotkey('T'), Some(Tool::Text));
    }
}
