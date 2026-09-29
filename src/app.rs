//! The editor: a plain libcosmic toplevel window that opens after the portal
//! returned the snip. It owns no layer-shell surface, so closing it is an
//! ordinary window close.

use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use cosmic::app::{Core, Task};
use cosmic::iced::keyboard::{self, Key, key::Named};
use cosmic::iced::widget::image::Handle;
use cosmic::iced::widget::stack;
use cosmic::iced::{
    Background, Border, Color, ContentFit, Length, Point as IPoint, Rectangle, Subscription, mouse,
};
use cosmic::widget::canvas::{self, Frame, Geometry, Path, Program, Stroke as IStroke};
use cosmic::widget::{self, button, container};
use cosmic::{Element, Renderer, Theme};
use tiny_skia::Pixmap;

use crate::annotation::{Document, Point, Shape, Stroke, Tool, arrow_barbs};
use crate::prefs::Prefs;
use crate::{clipboard, config, render};

pub const APP_ID: &str = "io.github.itssoup.CosmicSnip";

pub struct Flags {
    pub snip: Pixmap,
}

#[derive(Clone, Debug)]
pub enum Message {
    Tool(Tool),
    Color(usize),
    Wider,
    Narrower,
    Undo,
    Copy,
    Save,
    Saved(Result<Option<PathBuf>, String>),
    New,
    Exit,
    Begin(Point),
    Extend(Point),
    Finish,
    Key(keyboard::Event),
}

pub struct App {
    core: Core,
    snip: Pixmap,
    handle: Handle,
    doc: Document,
    tool: Tool,
    color: usize,
    pen_width: f32,
    highlight_width: f32,
    error: Option<String>,
}

impl App {
    fn width(&self) -> f32 {
        match self.tool {
            Tool::Highlighter => self.highlight_width,
            _ => self.pen_width,
        }
    }

    /// Colour and widths carry over to the next snip; the tool does not.
    fn remember(&self) {
        let prefs = Prefs {
            color: self.color,
            pen_width: self.pen_width,
            highlight_width: self.highlight_width,
        };
        if let Err(e) = prefs.store() {
            log::warn!("{e}");
        }
    }

    fn change_width(&mut self, delta: f32) {
        match self.tool {
            Tool::Highlighter => {
                self.highlight_width = (self.highlight_width + 4.0 * delta)
                    .clamp(config::HIGHLIGHT_WIDTH_MIN, config::HIGHLIGHT_WIDTH_MAX);
            }
            _ => {
                self.pen_width =
                    (self.pen_width + delta).clamp(config::PEN_WIDTH_MIN, config::PEN_WIDTH_MAX);
            }
        }
        self.remember();
    }

    /// The snip with every committed stroke, at the snip's own resolution.
    fn export(&self) -> Result<Vec<u8>, String> {
        render::encode_png(&render::composite(&self.snip, self.doc.committed()))
    }

    fn copy_and_exit(&mut self) -> Task<Message> {
        match self.export().and_then(|png| clipboard::spawn_server(&png)) {
            Ok(()) => cosmic::iced::exit(),
            Err(e) => {
                log::error!("{e}");
                self.error = Some(e);
                Task::none()
            }
        }
    }

    fn save(&self) -> Task<Message> {
        let dir = config::save_dir();
        let _ = std::fs::create_dir_all(&dir);
        let name = format!(
            "snip-{}.png",
            jiff::Zoned::now().strftime("%Y-%m-%d-%H%M%S")
        );
        log::debug!("save: asking the portal for a path in {}", dir.display());
        cosmic::task::future(async move {
            let dialog = cosmic::dialog::file_chooser::save::Dialog::new()
                .title("Save snip".to_string())
                .directory(dir)
                .file_name(name);
            let answer = dialog.save_file().await;
            log::debug!("save: portal answered, ok = {}", answer.is_ok());
            Message::Saved(match answer {
                Ok(response) => Ok(response.url().and_then(|u| u.to_file_path().ok())),
                Err(cosmic::dialog::file_chooser::Error::Cancelled) => Ok(None),
                Err(e) => Err(format!("save dialog failed: {e}")),
            })
        })
    }

    fn key(&mut self, event: keyboard::Event) -> Task<Message> {
        let keyboard::Event::KeyPressed { key, modifiers, .. } = event else {
            return Task::none();
        };
        if let Key::Named(Named::Escape) = key {
            return cosmic::iced::exit();
        }
        let Key::Character(c) = key else {
            return Task::none();
        };
        let Some(c) = c.chars().next().map(|c| c.to_ascii_lowercase()) else {
            return Task::none();
        };
        if modifiers.control() {
            return match c {
                'c' => self.copy_and_exit(),
                's' => self.save(),
                'z' => {
                    self.doc.undo();
                    Task::none()
                }
                'n' => self.update_new(),
                'q' => cosmic::iced::exit(),
                _ => Task::none(),
            };
        }
        match c {
            '+' | '=' | ']' => self.change_width(1.0),
            '-' | '[' => self.change_width(-1.0),
            _ => {
                if let Some(tool) = Tool::from_hotkey(c) {
                    self.tool = tool;
                }
            }
        }
        Task::none()
    }

    fn update_new(&mut self) -> Task<Message> {
        if let Ok(exe) = std::env::current_exe() {
            let spawned = Command::new(exe)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .process_group(0)
                .spawn();
            if let Err(e) = spawned {
                log::error!("cannot start a new snip: {e}");
                return Task::none();
            }
        }
        cosmic::iced::exit()
    }
}

impl cosmic::Application for App {
    type Executor = cosmic::executor::Default;
    type Flags = Flags;
    type Message = Message;
    const APP_ID: &'static str = APP_ID;

    fn core(&self) -> &Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut Core {
        &mut self.core
    }

    fn init(core: Core, flags: Flags) -> (Self, Task<Message>) {
        let snip = flags.snip;
        let mut rgba = Vec::with_capacity(snip.data().len());
        for p in snip.pixels() {
            let c = p.demultiply();
            rgba.extend_from_slice(&[c.red(), c.green(), c.blue(), c.alpha()]);
        }
        let handle = Handle::from_rgba(snip.width(), snip.height(), rgba);
        let prefs = Prefs::load();
        let app = App {
            core,
            snip,
            handle,
            doc: Document::default(),
            tool: Tool::Pen,
            color: prefs.color,
            pen_width: prefs.pen_width,
            highlight_width: prefs.highlight_width,
            error: None,
        };
        (app, Task::none())
    }

    fn on_escape(&mut self) -> Task<Message> {
        cosmic::iced::exit()
    }

    fn subscription(&self) -> Subscription<Message> {
        keyboard::listen().map(Message::Key)
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Tool(tool) => self.tool = tool,
            Message::Color(i) => {
                self.color = i.min(config::PALETTE.len() - 1);
                self.remember();
            }
            Message::Wider => self.change_width(1.0),
            Message::Narrower => self.change_width(-1.0),
            Message::Undo => {
                self.doc.undo();
            }
            Message::Copy => return self.copy_and_exit(),
            Message::Save => {
                log::debug!("save: requested");
                return self.save();
            }
            Message::Saved(Ok(None)) => {}
            Message::Saved(Err(e)) => {
                log::error!("{e}");
                self.error = Some(e);
            }
            Message::Saved(Ok(Some(path))) => {
                match self.export().and_then(|png| {
                    std::fs::write(&path, png)
                        .map_err(|e| format!("cannot write {}: {e}", path.display()))
                }) {
                    Ok(()) => return cosmic::iced::exit(),
                    Err(e) => {
                        log::error!("{e}");
                        self.error = Some(e);
                    }
                }
            }
            Message::New => return self.update_new(),
            Message::Exit => return cosmic::iced::exit(),
            Message::Begin(at) => {
                let rgba = config::PALETTE[self.color].rgba;
                self.doc.begin(self.tool, rgba, self.width(), at);
            }
            Message::Extend(at) => self.doc.extend(at),
            Message::Finish => self.doc.finish(),
            Message::Key(event) => return self.key(event),
        }
        Task::none()
    }

    fn header_start(&self) -> Vec<Element<'_, Message>> {
        let mut items: Vec<Element<'_, Message>> = Tool::ALL
            .iter()
            .map(|&tool| {
                button::icon(widget::icon::from_name(tool.icon()))
                    .selected(tool == self.tool)
                    .tooltip(format!("{} ({})", tool.label(), tool_hotkey(tool)))
                    .on_press(Message::Tool(tool))
                    .into()
            })
            .collect();
        items.push(
            widget::divider::vertical::default()
                .height(Length::Fixed(24.0))
                .into(),
        );
        for (i, c) in config::PALETTE.iter().enumerate() {
            items.push(swatch(i, c.rgba, i == self.color));
        }
        items
    }

    fn header_end(&self) -> Vec<Element<'_, Message>> {
        let mut items: Vec<Element<'_, Message>> = vec![
            button::icon(widget::icon::from_name("list-remove-symbolic"))
                .tooltip("Thinner ( - )")
                .on_press(Message::Narrower)
                .into(),
            widget::text::body(format!("{} px", self.width() as u32)).into(),
            button::icon(widget::icon::from_name("list-add-symbolic"))
                .tooltip("Thicker ( + )")
                .on_press(Message::Wider)
                .into(),
            button::icon(widget::icon::from_name("edit-undo-symbolic"))
                .tooltip("Undo (Ctrl+Z)")
                .on_press_maybe((!self.doc.committed().is_empty()).then_some(Message::Undo))
                .into(),
            button::standard("Save").on_press(Message::Save).into(),
            button::suggested("Copy").on_press(Message::Copy).into(),
        ];
        if let Some(e) = &self.error {
            items.insert(0, widget::text::body(e.clone()).into());
        }
        items
    }

    fn view(&self) -> Element<'_, Message> {
        // The snip and the strokes are separate layers: within one layer the
        // renderer paints images after meshes, so a snip drawn by the canvas
        // itself would cover every stroke. `stack` gives the canvas its own
        // layer above the image. ScaleDown centres the snip and never
        // enlarges it, which is what `Fit` assumes.
        let snip = widget::image(self.handle.clone())
            .content_fit(ContentFit::ScaleDown)
            .width(Length::Fill)
            .height(Length::Fill);
        let board = canvas::Canvas::new(Board { app: self })
            .width(Length::Fill)
            .height(Length::Fill);
        container(stack([snip.into(), board.into()]))
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }
}

fn tool_hotkey(tool: Tool) -> char {
    match tool {
        Tool::Pen => 'P',
        Tool::Highlighter => 'H',
        Tool::Arrow => 'A',
        Tool::Rect => 'R',
    }
}

fn to_color(rgba: [f32; 4]) -> Color {
    Color::from_rgba(rgba[0], rgba[1], rgba[2], rgba[3])
}

fn swatch<'a>(index: usize, rgba: [f32; 4], selected: bool) -> Element<'a, Message> {
    let fill = to_color(rgba);
    let dot = container(
        widget::Space::new()
            .width(Length::Fixed(18.0))
            .height(Length::Fixed(18.0)),
    )
    .class(cosmic::theme::Container::custom(move |theme: &Theme| {
        container::Style {
            background: Some(Background::Color(fill)),
            border: Border {
                radius: 9.0.into(),
                width: 1.0,
                color: theme.cosmic().palette.neutral_6.into(),
            },
            ..Default::default()
        }
    }));
    button::custom(dot)
        .padding(4)
        .selected(selected)
        .class(cosmic::theme::Button::Icon)
        .on_press(Message::Color(index))
        .into()
}

/// Maps between the snip's pixels and the canvas: the snip is fitted inside
/// the canvas, centred, never enlarged past one logical pixel per image pixel.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fit {
    pub scale: f32,
    pub offset_x: f32,
    pub offset_y: f32,
}

impl Fit {
    pub fn new(image_w: f32, image_h: f32, bounds_w: f32, bounds_h: f32) -> Self {
        let scale = (bounds_w / image_w)
            .min(bounds_h / image_h)
            .clamp(f32::MIN_POSITIVE, 1.0);
        Fit {
            scale,
            offset_x: ((bounds_w - image_w * scale) / 2.0).max(0.0),
            offset_y: ((bounds_h - image_h * scale) / 2.0).max(0.0),
        }
    }

    pub fn to_image(&self, x: f32, y: f32) -> Point {
        Point::new(
            (x - self.offset_x) / self.scale,
            (y - self.offset_y) / self.scale,
        )
    }

    pub fn to_canvas(&self, p: Point) -> IPoint {
        IPoint::new(
            self.offset_x + p.x * self.scale,
            self.offset_y + p.y * self.scale,
        )
    }
}

struct Board<'a> {
    app: &'a App,
}

#[derive(Default)]
struct Pointer {
    drawing: bool,
}

impl Board<'_> {
    fn fit(&self, bounds: Rectangle) -> Fit {
        Fit::new(
            self.app.snip.width() as f32,
            self.app.snip.height() as f32,
            bounds.width,
            bounds.height,
        )
    }
}

impl Program<Message, Theme, Renderer> for Board<'_> {
    type State = Pointer;

    fn update(
        &self,
        state: &mut Pointer,
        event: &canvas::Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<canvas::Action<Message>> {
        let canvas::Event::Mouse(event) = event else {
            return None;
        };
        let fit = self.fit(bounds);
        let at = |p: IPoint| {
            let q = fit.to_image(p.x, p.y);
            Point::new(
                q.x.clamp(0.0, self.app.snip.width() as f32),
                q.y.clamp(0.0, self.app.snip.height() as f32),
            )
        };
        match event {
            mouse::Event::ButtonPressed(mouse::Button::Left) => {
                let p = cursor.position_in(bounds)?;
                state.drawing = true;
                Some(canvas::Action::publish(Message::Begin(at(p))).and_capture())
            }
            mouse::Event::CursorMoved { .. } if state.drawing => {
                let p = cursor.position_in(bounds).or_else(|| {
                    cursor
                        .position()
                        .map(|p| IPoint::new(p.x - bounds.x, p.y - bounds.y))
                })?;
                Some(canvas::Action::publish(Message::Extend(at(p))).and_capture())
            }
            mouse::Event::ButtonReleased(mouse::Button::Left) if state.drawing => {
                state.drawing = false;
                Some(canvas::Action::publish(Message::Finish).and_capture())
            }
            _ => None,
        }
    }

    fn draw(
        &self,
        _state: &Pointer,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let fit = self.fit(bounds);
        let mut frame = Frame::new(renderer, bounds.size());
        for stroke in self.app.doc.all() {
            draw_stroke(&mut frame, &fit, stroke);
        }
        vec![frame.into_geometry()]
    }

    fn mouse_interaction(
        &self,
        _state: &Pointer,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        if cursor.is_over(bounds) {
            mouse::Interaction::Crosshair
        } else {
            mouse::Interaction::default()
        }
    }
}

fn draw_stroke(frame: &mut Frame, fit: &Fit, stroke: &Stroke) {
    let style = IStroke::default()
        .with_color(to_color(stroke.rgba))
        .with_width(stroke.width * fit.scale)
        .with_line_cap(canvas::LineCap::Round)
        .with_line_join(canvas::LineJoin::Round);
    let path = Path::new(|b| match &stroke.shape {
        Shape::Path(points) => {
            if let Some((first, rest)) = points.split_first() {
                b.move_to(fit.to_canvas(*first));
                if rest.is_empty() {
                    b.line_to(fit.to_canvas(*first));
                }
                for p in rest {
                    b.line_to(fit.to_canvas(*p));
                }
            }
        }
        Shape::Arrow { start, end } => {
            b.move_to(fit.to_canvas(*start));
            b.line_to(fit.to_canvas(*end));
            for barb in arrow_barbs(*start, *end, stroke.width) {
                b.move_to(fit.to_canvas(*end));
                b.line_to(fit.to_canvas(barb));
            }
        }
        Shape::Rect { start, end } => {
            let a = fit.to_canvas(*start);
            let c = fit.to_canvas(*end);
            b.move_to(a);
            b.line_to(IPoint::new(c.x, a.y));
            b.line_to(c);
            b.line_to(IPoint::new(a.x, c.y));
            b.close();
        }
    });
    frame.stroke(&path, style);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_small_snip_is_centred_and_not_enlarged() {
        let fit = Fit::new(100.0, 50.0, 300.0, 250.0);
        assert_eq!(fit.scale, 1.0);
        assert_eq!((fit.offset_x, fit.offset_y), (100.0, 100.0));
    }

    #[test]
    fn a_large_snip_is_shrunk_to_fit_and_maps_back() {
        let fit = Fit::new(2000.0, 1000.0, 1000.0, 1000.0);
        assert_eq!(fit.scale, 0.5);
        let p = fit.to_image(500.0, 250.0 + fit.offset_y);
        assert_eq!((p.x, p.y), (1000.0, 500.0));
        let back = fit.to_canvas(p);
        assert_eq!((back.x, back.y), (500.0, 250.0 + fit.offset_y));
    }
}
