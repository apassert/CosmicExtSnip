//! The editor: a plain libcosmic toplevel window that opens after the portal
//! returned the snip. It owns no layer-shell surface, so closing it is an
//! ordinary window close.
//!
//! The app starts with no window at all: the snip is taken first, so the editor
//! can never be in its own picture, and the window opens on the result. It runs
//! as a single instance - launching it again takes a new snip in the running
//! process - because in a Flatpak sandbox a copy is held by this process's own
//! window clipboard and must outlive the window (see sandbox.rs).

use std::path::PathBuf;

use cosmic::app::{Core, Task};
use cosmic::iced::keyboard::{self, Key, key::Named};
use cosmic::iced::widget::image::Handle;
use cosmic::iced::widget::stack;
use cosmic::iced::{
    Background, Border, Color, ContentFit, Length, Point as IPoint, Rectangle, Size, Subscription,
    mouse, window,
};
use cosmic::widget::canvas::{self, Frame, Geometry, Path, Program, Stroke as IStroke};
use cosmic::widget::{self, button, container};
use cosmic::{Element, Renderer, Theme};
use tiny_skia::Pixmap;

use crate::annotation::{Document, Hold, Point, Shape, Stroke, Tool, arrow_barbs};
use crate::capture::Grab;
use crate::prefs::Prefs;
use crate::{clipboard, config, render};

pub const APP_ID: &str = "io.github.apassert.CosmicExtSnip";

pub struct Flags {
    /// Where a copy is left for `main` to serve after the app exits (outside a sandbox).
    pub handoff: clipboard::Handoff,
}

impl cosmic::app::CosmicFlags for Flags {
    type SubCommand = String;
    type Args = Vec<String>;
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
    /// A new snip from the portal: Ok(None) when the selection was cancelled.
    Captured(Result<Option<Grab>, String>),
    /// The portal's copy, read through our window; the attempt number.
    ClipboardImage(Option<clipboard::Png>, u8),
    CloseWindow,
    /// While drawing freehand: has the pointer been held still long enough?
    Tick(std::time::Instant),
    Exit,
    Begin(Point),
    Extend(Point),
    Finish,
    Key(keyboard::Event),
}

pub struct App {
    core: Core,
    snip: Pixmap,
    handoff: clipboard::Handoff,
    /// A portal selection is on screen; a second one is not started.
    capturing: bool,
    /// A sandboxed copy is being held by this process's window clipboard.
    holding_copy: bool,
    /// Hold-to-straighten: where the pointer last moved, and when.
    hold: Hold,
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
        let png = match self.export() {
            Ok(png) => png,
            Err(e) => {
                log::error!("{e}");
                self.error = Some(e);
                return Task::none();
            }
        };
        if crate::sandbox::sandboxed() {
            // Through our own window, the only clipboard a sandbox has; then
            // the window goes and the process stays, holding the copy.
            self.holding_copy = true;
            let write =
                cosmic::iced::clipboard::write_data::<cosmic::Action<Message>>(clipboard::Png(png));
            return write.chain(self.close_window());
        }
        if let Ok(mut slot) = self.handoff.lock() {
            *slot = Some(png);
        }
        cosmic::iced::exit()
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
            return self.close_window();
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
                'n' => self.start_capture(),
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

    /// A new snip: at start, on Ctrl+N, and when the app is launched again. An
    /// open editor steps aside first so it is not in the picture.
    fn start_capture(&mut self) -> Task<Message> {
        if self.capturing {
            return Task::none();
        }
        self.capturing = true;
        let open = self.core.main_window_id();
        let hide = match open {
            Some(id) => window::minimize(id, true),
            None => Task::none(),
        };
        let capture = cosmic::task::future(async move {
            if open.is_some() {
                // Long enough for the window to be gone before the screen is taken.
                tokio::time::sleep(std::time::Duration::from_millis(400)).await;
            }
            Message::Captured(crate::capture::request().await)
        });
        hide.chain(capture)
    }

    /// The snip plus the header bar, within what fits on a screen.
    fn window_size(&self) -> Size {
        Size::new(
            (self.snip.width() as f32).clamp(560.0, 1600.0),
            (self.snip.height() as f32 + 56.0).clamp(360.0, 1000.0),
        )
    }

    /// Opens the editor window for the current snip, or brings the open one back.
    fn ensure_window(&mut self) -> Task<Message> {
        if let Some(id) = self.core.main_window_id() {
            return window::minimize::<cosmic::Action<Message>>(id, false)
                .chain(window::gain_focus(id));
        }
        let mut settings = window::Settings {
            size: self.window_size(),
            min_size: Some(Size::new(560.0, 360.0)),
            resizable: true,
            decorations: false,
            transparent: true,
            exit_on_close_request: false,
            ..Default::default()
        };
        settings.platform_specific.application_id = APP_ID.to_string();
        let (id, opened) = window::open(settings);
        self.core.set_main_window_id(Some(id));
        opened.discard()
    }

    /// Esc, the close button and a finished save. The process ends with the
    /// window unless it holds a sandboxed copy. Then the window is minimised,
    /// not closed: iced ties its clipboard to a window, and closing the last one
    /// drops the clipboard connection (iced_winit lib.rs, RemoveWindow ->
    /// Clipboard::unconnected). With it went the copy - the second Ctrl+C in a
    /// session "just closed" on the desktop, 2026-09-30. The same window comes
    /// back for the next snip.
    fn close_window(&mut self) -> Task<Message> {
        if !self.holding_copy {
            return cosmic::iced::exit();
        }
        match self.core.main_window_id() {
            Some(id) => window::minimize(id, true),
            None => Task::none(),
        }
    }

    /// The portal left the snip on the clipboard and a sandbox can read it only
    /// through a focused window of its own: read it once the editor is up.
    fn read_clipboard_image(attempt: u8) -> Task<Message> {
        cosmic::iced::Task::future(async move {
            tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        })
        .discard()
        .chain(
            cosmic::iced::clipboard::read_data::<clipboard::Png>()
                .map(move |png| cosmic::Action::App(Message::ClipboardImage(png, attempt))),
        )
    }

    fn set_snip(&mut self, snip: Pixmap) {
        self.handle = handle_for(&snip);
        self.snip = snip;
        self.doc = Document::default();
        self.error = None;
    }
}

/// The on-screen image: straight RGBA, as the snip's pixels are premultiplied.
fn handle_for(snip: &Pixmap) -> Handle {
    let mut rgba = Vec::with_capacity(snip.data().len());
    for p in snip.pixels() {
        let c = p.demultiply();
        rgba.extend_from_slice(&[c.red(), c.green(), c.blue(), c.alpha()]);
    }
    Handle::from_rgba(snip.width(), snip.height(), rgba)
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
        // Nothing to show until the portal answers; the window opens on the snip.
        let snip = Pixmap::new(1, 1).expect("a 1x1 pixmap");
        let handle = handle_for(&snip);
        let prefs = Prefs::load();
        let mut app = App {
            core,
            snip,
            handoff: flags.handoff,
            capturing: false,
            holding_copy: false,
            hold: Hold::default(),
            handle,
            doc: Document::default(),
            tool: Tool::Pen,
            color: prefs.color,
            pen_width: prefs.pen_width,
            highlight_width: prefs.highlight_width,
            error: None,
        };
        let first = app.start_capture();
        (app, first)
    }

    fn dbus_activation(&mut self, _msg: cosmic::dbus_activation::Message) -> Task<Message> {
        // Launched again while running: that is a request for a new snip.
        self.start_capture()
    }

    fn on_close_requested(&self, _id: window::Id) -> Option<Message> {
        Some(Message::CloseWindow)
    }

    fn on_escape(&mut self) -> Task<Message> {
        self.close_window()
    }

    fn subscription(&self) -> Subscription<Message> {
        let keys = keyboard::listen().map(Message::Key);
        // Only while a freehand stroke is being drawn and is not straight yet.
        let freehand = matches!(self.tool, Tool::Pen | Tool::Highlighter);
        if freehand && self.doc.is_drawing() && !self.doc.is_straight() {
            let tick =
                cosmic::iced::time::every(std::time::Duration::from_millis(100)).map(Message::Tick);
            return Subscription::batch([keys, tick]);
        }
        keys
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
                    Ok(()) => return self.close_window(),
                    Err(e) => {
                        log::error!("{e}");
                        self.error = Some(e);
                    }
                }
            }
            Message::New => return self.start_capture(),
            Message::Captured(result) => {
                self.capturing = false;
                let open = self.core.main_window_id().is_some();
                match result {
                    Ok(Some(Grab::Image(snip))) => {
                        self.set_snip(snip);
                        return self.ensure_window();
                    }
                    Ok(Some(Grab::OnClipboard)) => {
                        let window = self.ensure_window();
                        return window.chain(Self::read_clipboard_image(1));
                    }
                    Ok(None) if open => return self.ensure_window(),
                    Ok(None) => return self.close_window(),
                    Err(e) => {
                        log::error!("{e}");
                        self.error = Some(e);
                        if !open && !self.holding_copy {
                            return cosmic::iced::exit();
                        }
                        return if open {
                            self.ensure_window()
                        } else {
                            Task::none()
                        };
                    }
                }
            }
            Message::ClipboardImage(Some(png), _) => match crate::render::decode_png(&png.0) {
                Ok(snip) => {
                    self.set_snip(snip);
                    // The window opened before its size was known (the snip was
                    // still on the clipboard); fit it to the snip now.
                    if let Some(id) = self.core.main_window_id() {
                        return window::resize(id, self.window_size());
                    }
                }
                Err(e) => self.error = Some(format!("the copied snip cannot be read: {e}")),
            },
            // The window may not have the keyboard yet; the clipboard is only
            // offered to a focused client. A few tries, then say so.
            Message::ClipboardImage(None, attempt) if attempt < 10 => {
                return Self::read_clipboard_image(attempt + 1);
            }
            Message::ClipboardImage(None, _) => {
                self.error = Some("the snip was copied, but it could not be read back; take it with Enter instead".into());
            }
            Message::CloseWindow => return self.close_window(),
            Message::Exit => return cosmic::iced::exit(),
            Message::Begin(at) => {
                let rgba = config::PALETTE[self.color].rgba;
                self.doc.begin(self.tool, rgba, self.width(), at);
                self.hold.moved(at, std::time::Instant::now());
            }
            Message::Extend(at) => {
                self.doc.extend(at);
                self.hold.moved(at, std::time::Instant::now());
            }
            Message::Finish => {
                self.doc.finish();
                self.hold.reset();
            }
            Message::Tick(now) => {
                if self.hold.held(now) {
                    self.doc.straighten();
                }
            }
            Message::Key(event) => return self.key(event),
        }
        Task::none()
    }

    fn header_start(&self) -> Vec<Element<'_, Message>> {
        let new = button::standard("New snip")
            .leading_icon(widget::icon::from_name("list-add-symbolic"))
            .on_press(Message::New);
        let mut items: Vec<Element<'_, Message>> = vec![
            widget::tooltip(
                new,
                widget::text::body("New snip (Ctrl+N)"),
                widget::tooltip::Position::Bottom,
            )
            .into(),
        ];
        items.extend(Tool::ALL.iter().map(|&tool| {
            button::icon(widget::icon::from_name(tool.icon()))
                .selected(tool == self.tool)
                .tooltip(format!("{} ({})", tool.label(), tool_hotkey(tool)))
                .on_press(Message::Tool(tool))
                .into()
        }));
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
