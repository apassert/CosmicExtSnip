//! Renders the scene used for the store screenshot: a few lines of this
//! project's own code, annotated with every tool through the app's own model
//! (`Document`) and renderer. Open the result in the editor to photograph it:
//!
//! ```text
//! cargo run --example screenshot_scene -- scene.png
//! COSMIC_EXT_SNIP_DUMP=editor.png cosmic-ext-snip scene.png
//! ```
use cosmic_ext_snip::annotation::{Document, Point, Tool};
use cosmic_ext_snip::{config, render};
use tiny_skia::{Color, Pixmap};

const CODE: [&str; 11] = [
    "/// The corner opposite `start` of the square that holds a circle",
    "/// reaching towards `at`: the longer side of the dragged box wins.",
    "fn circle_corner(start: Point, at: Point) -> Point {",
    "    let (dx, dy) = (at.x - start.x, at.y - start.y);",
    "    let side = dx.abs().max(dy.abs());",
    "    let x_sign = if dx < 0.0 { -1.0 } else { 1.0 };",
    "    let y_sign = if dy < 0.0 { -1.0 } else { 1.0 };",
    "    Point::new(start.x + x_sign * side, start.y + y_sign * side)",
    "}",
    "",
    "",
];
const SIZE: f32 = 22.0;
const LEFT: f32 = 40.0;
const TOP: f32 = 36.0;
const LINE: f32 = SIZE * render::TEXT_LINE_HEIGHT;
/// Fira Mono's advance, measured on the rendered line.
const CHAR: f32 = SIZE * 0.616;

fn colour(name: &str) -> [f32; 4] {
    config::PALETTE
        .iter()
        .find(|c| c.name == name)
        .map(|c| c.rgba)
        .unwrap_or([1.0; 4])
}

fn line_y(i: usize) -> f32 {
    TOP + LINE * i as f32
}

fn main() {
    let out = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "scene.png".into());
    let mut base = Pixmap::new(1120, 500).expect("size");
    base.fill(Color::from_rgba8(30, 32, 38, 255));

    // The code, in a monospace font.
    let mut code = Document::default();
    for (i, text) in CODE.iter().enumerate() {
        let grey = text.starts_with("///");
        let rgba = if grey {
            [0.55, 0.58, 0.62, 1.0]
        } else {
            [0.86, 0.88, 0.90, 1.0]
        };
        code.begin(Tool::Text, rgba, SIZE, Point::new(LEFT, line_y(i)));
        code.type_text(text);
        code.finish();
    }
    let base = render::composite_with_font(&base, code.committed(), Some("Fira Mono"));

    // The annotations, as a user would draw them.
    let mut d = Document::default();
    let mid = |i: usize| line_y(i) + LINE / 2.0;
    // Highlighter over `dx.abs().max(dy.abs())`.
    d.begin(
        Tool::Highlighter,
        colour("orange"),
        20.0,
        Point::new(LEFT + 15.0 * CHAR, mid(4)),
    );
    d.extend(Point::new(LEFT + 39.0 * CHAR, mid(4)));
    d.finish();
    // Rectangle around the signature.
    d.begin(
        Tool::Rect,
        colour("red"),
        3.0,
        Point::new(LEFT - 8.0, line_y(2) - 2.0),
    );
    d.extend(Point::new(LEFT + 53.0 * CHAR + 8.0, line_y(3) - 2.0));
    d.finish();
    // A circle around `side` in the last line.
    let sx = LEFT + 58.0 * CHAR;
    d.begin(
        Tool::Circle,
        colour("blue"),
        3.0,
        Point::new(sx - 16.0, mid(7) - 30.0),
    );
    d.extend(Point::new(sx + 4.0 * CHAR + 16.0, mid(7) + 30.0));
    d.finish();
    // An arrow from the note to the highlighted call.
    d.begin(Tool::Arrow, colour("green"), 4.0, Point::new(820.0, 420.0));
    d.extend(Point::new(LEFT + 36.0 * CHAR, mid(4) + 16.0));
    d.finish();
    d.begin(Tool::Text, colour("green"), 30.0, Point::new(700.0, 426.0));
    d.type_text("the longer side wins");
    d.finish();
    // A freehand tick with the pen.
    d.begin(
        Tool::Pen,
        colour("red"),
        4.0,
        Point::new(LEFT + 54.0 * CHAR + 30.0, mid(2)),
    );
    for (x, y) in [(8.0, 10.0), (14.0, 16.0), (26.0, -4.0), (40.0, -22.0)] {
        d.extend(Point::new(LEFT + 54.0 * CHAR + 30.0 + x, mid(2) + y));
    }
    d.finish();

    let family = std::env::var("SCENE_FONT").ok();
    let scene = render::composite_with_font(&base, d.committed(), family.as_deref());
    std::fs::write(&out, render::encode_png(&scene).expect("encode")).expect("write");
    println!("{out}");
}
