//! Composite the annotations onto the snip at its native resolution.
//!
//! Pure: pixels in, pixels out, no display. The editor draws the same strokes
//! on screen through the toolkit; this is what Ctrl+C and Ctrl+S export, so it
//! is what the tests pin down pixel by pixel.

use tiny_skia::{
    Color, LineCap, LineJoin, Paint, PathBuilder, Pixmap, Rect, Stroke as Pen, Transform,
};

use crate::annotation::{Point, Shape, Stroke, arrow_barbs};
use crate::config::{MAX_IMAGE_HEIGHT, MAX_IMAGE_WIDTH};

/// Decode the portal's PNG, refusing anything larger than a 16K display.
pub fn decode_png(bytes: &[u8]) -> Result<Pixmap, String> {
    let pixmap = Pixmap::decode_png(bytes).map_err(|e| format!("not a readable PNG: {e}"))?;
    if pixmap.width() > MAX_IMAGE_WIDTH || pixmap.height() > MAX_IMAGE_HEIGHT {
        return Err(format!(
            "the snip is {}x{}, larger than the {MAX_IMAGE_WIDTH}x{MAX_IMAGE_HEIGHT} this app accepts",
            pixmap.width(),
            pixmap.height()
        ));
    }
    Ok(pixmap)
}

pub fn encode_png(pixmap: &Pixmap) -> Result<Vec<u8>, String> {
    pixmap
        .encode_png()
        .map_err(|e| format!("could not encode PNG: {e}"))
}

/// A copy of `base` with every stroke painted on it; text in a sans-serif font.
pub fn composite<'a>(base: &Pixmap, strokes: impl IntoIterator<Item = &'a Stroke>) -> Pixmap {
    composite_with_font(base, strokes, None)
}

/// The same, with text in `family` - the desktop's interface font, so the
/// export shows what the editor showed.
pub fn composite_with_font<'a>(
    base: &Pixmap,
    strokes: impl IntoIterator<Item = &'a Stroke>,
    family: Option<&str>,
) -> Pixmap {
    let mut out = base.clone();
    for stroke in strokes {
        if let Shape::Text { at, text, size } = &stroke.shape {
            paint_text(&mut out, *at, text, *size, stroke.rgba, family);
        } else {
            paint(&mut out, stroke);
        }
    }
    out
}

/// The line height the editor uses too, so text sits in the same place.
pub const TEXT_LINE_HEIGHT: f32 = 1.25;

/// Loading the system's fonts takes a moment; it is done once.
fn fonts() -> &'static std::sync::Mutex<(cosmic_text::FontSystem, cosmic_text::SwashCache)> {
    static FONTS: std::sync::OnceLock<
        std::sync::Mutex<(cosmic_text::FontSystem, cosmic_text::SwashCache)>,
    > = std::sync::OnceLock::new();
    FONTS.get_or_init(|| {
        std::sync::Mutex::new((
            cosmic_text::FontSystem::new(),
            cosmic_text::SwashCache::new(),
        ))
    })
}

/// One line of text, its top-left corner at `at`, shaped and rasterised with
/// cosmic-text and blended in pixel by pixel.
fn paint_text(
    pixmap: &mut Pixmap,
    at: Point,
    text: &str,
    size: f32,
    rgba: [f32; 4],
    family: Option<&str>,
) {
    use cosmic_text::{Attrs, Buffer, Family, Metrics, Shaping};
    let Ok(mut guard) = fonts().lock() else {
        return;
    };
    let (font_system, cache) = &mut *guard;
    let mut buffer = Buffer::new(font_system, Metrics::new(size, size * TEXT_LINE_HEIGHT));
    buffer.set_size(None, None);
    let attrs = match family {
        Some(name) => Attrs::new().family(Family::Name(name)),
        None => Attrs::new().family(Family::SansSerif),
    };
    buffer.set_text(text, &attrs, Shaping::Advanced, None);
    let to8 = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    let color = cosmic_text::Color::rgba(to8(rgba[0]), to8(rgba[1]), to8(rgba[2]), to8(rgba[3]));
    buffer.draw(font_system, cache, color, |x, y, w, h, c| {
        if c.a() == 0 {
            return;
        }
        let Some(rect) = Rect::from_xywh(at.x + x as f32, at.y + y as f32, w as f32, h as f32)
        else {
            return;
        };
        let mut paint = Paint::default();
        paint.set_color_rgba8(c.r(), c.g(), c.b(), c.a());
        pixmap.fill_rect(rect, &paint, Transform::identity(), None);
    });
}

fn paint(pixmap: &mut Pixmap, stroke: &Stroke) {
    let Some(path) = outline(&stroke.shape, stroke.width) else {
        return;
    };
    let [r, g, b, a] = stroke.rgba;
    let mut paint = Paint::default();
    paint.set_color(Color::from_rgba(r, g, b, a).unwrap_or(Color::BLACK));
    paint.anti_alias = true;
    let pen = Pen {
        width: stroke.width,
        line_cap: LineCap::Round,
        line_join: LineJoin::Round,
        ..Pen::default()
    };
    // One path per stroke, stroked once: a translucent highlighter that
    // crosses itself stays one even tint instead of darkening where it overlaps.
    pixmap.stroke_path(&path, &paint, &pen, Transform::identity(), None);
}

fn outline(shape: &Shape, width: f32) -> Option<tiny_skia::Path> {
    let mut pb = PathBuilder::new();
    match shape {
        Shape::Path(points) => {
            let (first, rest) = points.split_first()?;
            if rest.is_empty() {
                return None;
            }
            pb.move_to(first.x, first.y);
            for p in rest {
                pb.line_to(p.x, p.y);
            }
        }
        Shape::Arrow { start, end } => {
            pb.move_to(start.x, start.y);
            pb.line_to(end.x, end.y);
            for barb in arrow_barbs(*start, *end, width) {
                pb.move_to(end.x, end.y);
                pb.line_to(barb.x, barb.y);
            }
        }
        Shape::Rect { start, end } => {
            let rect = rect_between(*start, *end)?;
            return Some(PathBuilder::from_rect(rect));
        }
        Shape::Ellipse { start, end } => {
            let rect = rect_between(*start, *end)?;
            return PathBuilder::from_oval(rect);
        }
        // Painted by paint_text, not stroked.
        Shape::Text { .. } => return None,
    }
    pb.finish()
}

fn rect_between(a: Point, b: Point) -> Option<Rect> {
    Rect::from_ltrb(a.x.min(b.x), a.y.min(b.y), a.x.max(b.x), a.y.max(b.y))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::annotation::{Document, Tool};

    const RED: [f32; 4] = [0.93, 0.16, 0.16, 1.0];
    const BLUE: [f32; 4] = [0.15, 0.45, 0.93, 1.0];

    fn white(w: u32, h: u32) -> Pixmap {
        let mut p = Pixmap::new(w, h).unwrap();
        p.fill(Color::WHITE);
        p
    }

    fn rgba(p: &Pixmap, x: u32, y: u32) -> [u8; 4] {
        let c = p.pixel(x, y).unwrap().demultiply();
        [c.red(), c.green(), c.blue(), c.alpha()]
    }

    fn drawn(
        tool: Tool,
        color: [f32; 4],
        width: f32,
        from: (f32, f32),
        to: (f32, f32),
    ) -> Document {
        let mut doc = Document::default();
        doc.begin(tool, color, width, Point::new(from.0, from.1));
        doc.extend(Point::new(to.0, to.1));
        doc.finish();
        doc
    }

    #[test]
    fn a_pen_line_is_red_where_it_runs_and_nowhere_else() {
        let doc = drawn(Tool::Pen, RED, 3.0, (5.0, 20.0), (35.0, 20.0));
        let out = composite(&white(40, 40), doc.committed());
        let [r, g, b, _] = rgba(&out, 20, 20);
        assert!(r > 200 && g < 80 && b < 80, "on the line: {r},{g},{b}");
        assert_eq!(
            rgba(&out, 20, 5),
            [255, 255, 255, 255],
            "far from the line: untouched"
        );
        assert_eq!(rgba(&out, 20, 30), [255, 255, 255, 255]);
    }

    #[test]
    fn the_highlighter_tints_rather_than_covers() {
        let doc = drawn(Tool::Highlighter, BLUE, 20.0, (5.0, 20.0), (35.0, 20.0));
        let out = composite(&white(40, 40), doc.committed());
        let [r, _, b, _] = rgba(&out, 20, 20);
        // Blue at 35% over white: the red channel drops, but nowhere near the
        // 38 a solid stroke would leave. That is what makes it a highlighter.
        assert!(r < 230 && r > 150, "red channel {r}");
        assert!(b > 240, "blue channel {b}");
    }

    #[test]
    fn a_rectangle_is_an_outline_not_a_fill() {
        let doc = drawn(Tool::Rect, RED, 3.0, (5.0, 5.0), (35.0, 35.0));
        let out = composite(&white(40, 40), doc.committed());
        let [r, g, _, _] = rgba(&out, 5, 20);
        assert!(r > 200 && g < 80, "on the left edge");
        assert_eq!(
            rgba(&out, 20, 20),
            [255, 255, 255, 255],
            "the middle stays clear"
        );
    }

    #[test]
    fn an_arrow_has_a_head() {
        // Shaft along y=20; the barbs reach back from the tip at (35,20), so a
        // pixel off the shaft but near the tip must be painted.
        let doc = drawn(Tool::Arrow, RED, 3.0, (5.0, 20.0), (35.0, 20.0));
        let out = composite(&white(40, 40), doc.committed());
        let [bx, by] = arrow_barbs(Point::new(5.0, 20.0), Point::new(35.0, 20.0), 3.0);
        let mid = |a: f32, b: f32| ((a + b) / 2.0).round() as u32;
        let [r, g, _, _] = rgba(&out, mid(35.0, bx.x), mid(20.0, bx.y));
        assert!(r > 200 && g < 100, "halfway along a barb");
        let _ = by;
    }

    #[test]
    fn the_snip_itself_is_not_changed() {
        let base = white(40, 40);
        let doc = drawn(Tool::Pen, RED, 3.0, (5.0, 20.0), (35.0, 20.0));
        let _ = composite(&base, doc.committed());
        assert_eq!(rgba(&base, 20, 20), [255, 255, 255, 255]);
    }

    #[test]
    fn png_round_trips_the_pixels() {
        let doc = drawn(Tool::Pen, RED, 3.0, (5.0, 20.0), (35.0, 20.0));
        let out = composite(&white(40, 40), doc.committed());
        let back = decode_png(&encode_png(&out).unwrap()).unwrap();
        assert_eq!(rgba(&back, 20, 20), rgba(&out, 20, 20));
        assert_eq!((back.width(), back.height()), (40, 40));
    }

    #[test]
    fn garbage_is_refused_with_a_reason() {
        assert!(
            decode_png(b"not a png")
                .unwrap_err()
                .contains("not a readable PNG")
        );
    }

    #[test]
    fn a_circle_is_an_outline_inside_its_rectangle() {
        let mut doc = Document::default();
        doc.begin(Tool::Circle, BLUE, 2.0, Point::new(5.0, 5.0));
        doc.extend(Point::new(35.0, 35.0));
        doc.finish();
        let out = composite(&white(40, 40), doc.committed());
        let px = |x, y| out.pixel(x, y).unwrap().demultiply();
        assert!(
            px(20, 5).blue() > 200 && px(20, 5).red() < 120,
            "the top of the ring is blue"
        );
        assert_eq!(
            (px(20, 20).red(), px(20, 20).blue()),
            (255, 255),
            "the middle is not filled"
        );
        assert_eq!(
            (px(6, 6).red(), px(6, 6).blue()),
            (255, 255),
            "the rectangle's corner is outside it"
        );
    }

    #[test]
    fn text_is_drawn_in_its_colour_where_it_was_placed_and_nowhere_else() {
        let mut doc = Document::default();
        doc.begin(Tool::Text, RED, 24.0, Point::new(10.0, 10.0));
        doc.type_text("HH");
        doc.finish();
        let out = composite(&white(120, 60), doc.committed());
        let mut inked = 0;
        for y in 0..60 {
            for x in 0..120 {
                let p = out.pixel(x, y).unwrap().demultiply();
                if p.green() < 128 {
                    inked += 1;
                    assert!(
                        (10..80).contains(&x) && (10..45).contains(&y),
                        "ink outside the text box at ({x},{y})"
                    );
                    assert!(p.red() > p.green(), "the ink is red at ({x},{y})");
                }
            }
        }
        assert!(
            inked > 40,
            "only {inked} pixels inked: the text was not drawn"
        );
    }
}
