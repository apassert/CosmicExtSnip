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

/// The header's tools need this much width; a narrower snip is centred.
const TOOLBAR_MIN_WIDTH: f32 = 720.0;
const MIN_WINDOW_HEIGHT: f32 = 240.0;
/// libcosmic draws a 1 px border around a window that is not maximised
/// (view_main: `.padding(if maximized { 0 } else { 1 })`).
const BORDER: f32 = 1.0;
/// One 2560x1440 screen, less the panel.
const MAX_WINDOW: Size = Size::new(2560.0, 1360.0);

pub struct Flags {
    /// Where a copy is left for `main` to serve after the app exits (outside a sandbox).
    pub handoff: clipboard::Handoff,
    /// An image to annotate instead of taking a snip.
    pub open: Option<PathBuf>,
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
    /// The Text tool: a click places a text cursor there.
    PlaceText(Point),
    /// The window manager asks a window of ours to close.
    CloseRequested(window::Id),
    /// What the canvas really got on its first frame: the window is then
    /// corrected by the difference so the snip shows at exactly 1:1.
    CanvasSize(Size),
    /// Debug only (COSMIC_EXT_SNIP_DUMP): the window's own rendering, alpha included.
    Dumped(window::Screenshot),
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
    /// The window iced's clipboard is attached to: the first one opened while
    /// none was attached. While it holds a copy it is never closed.
    clip_window: Option<window::Id>,
    /// An editor was open when the snip started: a cancelled selection brings
    /// the previous snip back instead of ending the app.
    reopen_on_cancel: bool,
    /// The editor has been unpinned after its first frame; resizing is the user's.
    settled: bool,
    /// Hold-to-straighten: where the pointer last moved, and when.
    hold: Hold,
    handle: Handle,
    doc: Document,
    tool: Tool,
    color: usize,
    pen_width: f32,
    highlight_width: f32,
    /// Text height in snip pixels.
    text_size: f32,
    error: Option<String>,
}

impl App {
    fn width(&self) -> f32 {
        match self.tool {
            Tool::Highlighter => self.highlight_width,
            Tool::Text => self.text_size,
            _ => self.pen_width,
        }
    }

    /// Colour and widths carry over to the next snip; the tool does not.
    fn remember(&self) {
        let prefs = Prefs {
            color: self.color,
            pen_width: self.pen_width,
            highlight_width: self.highlight_width,
            text_size: self.text_size,
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
            Tool::Text => {
                self.text_size = (self.text_size + 4.0 * delta)
                    .clamp(config::TEXT_SIZE_MIN, config::TEXT_SIZE_MAX);
            }
            _ => {
                self.pen_width =
                    (self.pen_width + delta).clamp(config::PEN_WIDTH_MIN, config::PEN_WIDTH_MAX);
            }
        }
        self.remember();
    }

    /// The snip with every committed stroke, at the snip's own resolution.
    /// Text in the desktop's interface font, the one the editor showed it in.
    fn export(&mut self) -> Result<Vec<u8>, String> {
        // Text still being typed is part of what the user sees: keep it.
        if self.doc.is_typing() {
            self.doc.finish();
        }
        let family = cosmic::config::interface_font().family;
        render::encode_png(&render::composite_with_font(
            &self.snip,
            self.doc.committed(),
            Some(&family),
        ))
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
        let keyboard::Event::KeyPressed {
            key,
            modifiers,
            text,
            ..
        } = event
        else {
            return Task::none();
        };
        // Typing text: keys are text, not tools. Esc drops it, Enter keeps it,
        // and a shortcut with Ctrl keeps it and then does what it does.
        if self.doc.is_typing() {
            match &key {
                Key::Named(Named::Escape) => {
                    self.doc.cancel();
                    return Task::none();
                }
                Key::Named(Named::Enter) => {
                    self.doc.finish();
                    return Task::none();
                }
                Key::Named(Named::Backspace) => {
                    self.doc.backspace();
                    return Task::none();
                }
                _ if modifiers.control() => self.doc.finish(),
                _ => {
                    if let Some(typed) = text {
                        self.doc.type_text(&typed);
                    }
                    return Task::none();
                }
            }
        }
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

    /// A new snip: at start, on Ctrl+N, from the New snip button, and when the
    /// app is launched again. An open editor goes away first so it is not in
    /// the picture, and a new one opens on the result.
    fn start_capture(&mut self) -> Task<Message> {
        if self.capturing {
            return Task::none();
        }
        self.capturing = true;
        let away = self.put_editor_away();
        self.reopen_on_cancel = away.is_some();
        let wait = away.is_some();
        let capture = cosmic::task::future(async move {
            if wait {
                // Long enough for the window to be gone before the screen is taken.
                tokio::time::sleep(std::time::Duration::from_millis(400)).await;
            }
            Message::Captured(crate::capture::request().await)
        });
        away.unwrap_or_else(Task::none).chain(capture)
    }

    /// Takes the editor off the screen. Wayland can neither hide a window nor
    /// bring back a minimised one (winit-wayland: "You can't unminimize the
    /// window on Wayland"; set_visible: "Not possible on Wayland") - minimising
    /// it for a new snip left it minimised for good. So the editor is closed,
    /// and a new one opens later. The one exception is the window holding a
    /// sandboxed copy: closing it would drop iced's clipboard connection and
    /// the copy with it, so it is only minimised and stays as it is.
    fn put_editor_away(&mut self) -> Option<Task<Message>> {
        let id = self.core.main_window_id()?;
        self.core.set_main_window_id(None);
        if self.holding_copy && self.clip_window == Some(id) {
            return Some(window::minimize(id, true));
        }
        if self.clip_window == Some(id) {
            self.clip_window = None;
        }
        Some(window::close(id))
    }

    /// The snip at 1:1 plus the header bar and the window border, never
    /// narrower than the toolbar and never larger than a screen. Exact at
    /// creation, because COSMIC keeps a floating window at the size it was
    /// mapped with: resizing it afterwards from the app changed nothing
    /// (measured: window::resize, and min = max after mapping, both ignored).
    fn window_size(&self) -> Size {
        Size::new(
            (self.snip.width() as f32 + 2.0 * BORDER).clamp(TOOLBAR_MIN_WIDTH, MAX_WINDOW.width),
            (self.snip.height() as f32 + header_height() + 2.0 * BORDER)
                .clamp(MIN_WINDOW_HEIGHT, MAX_WINDOW.height),
        )
    }

    /// The editor opened pinned (min = max), so that COSMIC mapped it floating
    /// even on a tiled workspace: cosmic-comp decides that once, at map time
    /// (src/shell/mod.rs, map_window -> is_dialog). After its first frame it
    /// may be resized.
    fn settle(&mut self, canvas: Size) -> Task<Message> {
        let Some(id) = self.core.main_window_id() else {
            return Task::none();
        };
        log::debug!(
            "editor: snip {}x{}, canvas {canvas:?}",
            self.snip.width(),
            self.snip.height()
        );
        if self.settled {
            return Task::none();
        }
        self.settled = true;
        window::set_min_size::<cosmic::Action<Message>>(
            id,
            Some(Size::new(TOOLBAR_MIN_WIDTH, MIN_WINDOW_HEIGHT)),
        )
        .chain(window::set_max_size(id, None))
    }

    /// Opens a new editor window for the current snip; an open one is focused.
    fn open_editor(&mut self) -> Task<Message> {
        if let Some(id) = self.core.main_window_id() {
            return window::gain_focus(id);
        }
        self.settled = false;
        // Pinned from the first frame (min = max), so the compositor floats it
        // rather than tiling it: see settle.
        let mut settings = window::Settings {
            size: self.window_size(),
            min_size: Some(self.window_size()),
            max_size: Some(self.window_size()),
            // Resizable once unpinned (fit_to_snip -> unpin).
            resizable: true,
            decorations: false,
            transparent: true,
            exit_on_close_request: false,
            ..Default::default()
        };
        settings.platform_specific.application_id = APP_ID.to_string();
        let (id, opened) = window::open(settings);
        self.core.set_main_window_id(Some(id));
        // iced attaches its clipboard to a window when it opens one and none is
        // attached (iced_winit lib.rs, Opened -> Clipboard::connect).
        if self.clip_window.is_none() {
            self.clip_window = Some(id);
        }
        opened.discard()
    }

    /// Esc, the close button and a finished save. The app ends unless it holds
    /// a sandboxed copy; then the editor goes away (put_editor_away) and the
    /// process stays, holding it.
    fn close_window(&mut self) -> Task<Message> {
        if !self.holding_copy {
            return cosmic::iced::exit();
        }
        self.put_editor_away().unwrap_or_else(Task::none)
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

/// The header bar's height at the user's density: 32 plus libcosmic's padding
/// (header_bar.rs: compact [3, _, 4, _], otherwise [7, _, 8, _]).
fn header_height() -> f32 {
    match cosmic::config::header_size() {
        cosmic::cosmic_theme::Density::Compact => 39.0,
        _ => 47.0,
    }
}

/// Debug only: where to save the window's own rendering once, then exit.
fn dump_path() -> Option<PathBuf> {
    std::env::var_os("COSMIC_EXT_SNIP_DUMP").map(PathBuf::from)
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
            clip_window: None,
            reopen_on_cancel: false,
            settled: false,
            hold: Hold::default(),
            handle,
            doc: Document::default(),
            tool: Tool::Pen,
            color: prefs.color,
            pen_width: prefs.pen_width,
            highlight_width: prefs.highlight_width,
            text_size: prefs.text_size,
            error: None,
        };
        // The snip runs edge to edge under the header: no padded content box,
        // so the window can be exactly the snip plus the header.
        app.core.window.content_container = false;
        let first = match flags.open {
            Some(path) => match std::fs::read(&path)
                .map_err(|e| format!("cannot read {}: {e}", path.display()))
                .and_then(|bytes| render::decode_png(&bytes))
            {
                Ok(snip) => {
                    app.set_snip(snip);
                    app.open_editor()
                }
                Err(e) => {
                    log::error!("{e}");
                    cosmic::iced::exit()
                }
            },
            None => app.start_capture(),
        };
        (app, first)
    }

    fn dbus_activation(&mut self, _msg: cosmic::dbus_activation::Message) -> Task<Message> {
        // Launched again while running: that is a request for a new snip.
        self.start_capture()
    }

    fn on_close_requested(&self, id: window::Id) -> Option<Message> {
        Some(Message::CloseRequested(id))
    }

    /// The window that holds a copy, when it is not the editor: minimised, and
    /// if someone restores it, it says what it is for.
    fn view_window(&self, _id: window::Id) -> Element<'_, Message> {
        container(widget::text::body(
            "Snip is keeping your last copy on the clipboard. Close this window to let it go.",
        ))
        .center(Length::Fill)
        .into()
    }

    /// Esc is handled with the other keys (App::key), where it can tell typed
    /// text from the editor: handling it here too would close the window the
    /// moment Esc dropped the text.
    fn on_escape(&mut self) -> Task<Message> {
        Task::none()
    }

    fn subscription(&self) -> Subscription<Message> {
        let keys = keyboard::listen().map(Message::Key);
        // Only while a freehand stroke or an ellipse is being drawn and has not
        // been straightened (into a line, or a circle) yet.
        let holdable = matches!(self.tool, Tool::Pen | Tool::Highlighter | Tool::Circle);
        if holdable && self.doc.is_drawing() && !self.doc.is_straight() {
            let tick =
                cosmic::iced::time::every(std::time::Duration::from_millis(100)).map(Message::Tick);
            return Subscription::batch([keys, tick]);
        }
        keys
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Tool(tool) => {
                if self.doc.is_typing() {
                    self.doc.finish();
                }
                self.tool = tool;
            }
            Message::PlaceText(at) => {
                // A click elsewhere keeps the text being typed and starts new text.
                if self.doc.is_typing() {
                    self.doc.finish();
                }
                let rgba = config::PALETTE[self.color].rgba;
                self.doc.begin(Tool::Text, rgba, self.text_size, at);
            }
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
                let reopen = std::mem::take(&mut self.reopen_on_cancel);
                match result {
                    Ok(Some(Grab::Image(snip))) => {
                        self.set_snip(snip);
                        return self.open_editor();
                    }
                    Ok(Some(Grab::OnClipboard)) => {
                        let editor = self.open_editor();
                        return editor.chain(Self::read_clipboard_image(1));
                    }
                    Ok(None) | Err(_) => {
                        if let Err(e) = result {
                            log::error!("{e}");
                            self.error = Some(e);
                        }
                        // Cancelled from an open editor: the previous snip comes back.
                        if reopen {
                            return self.open_editor();
                        }
                        if !self.holding_copy {
                            return cosmic::iced::exit();
                        }
                    }
                }
            }
            Message::ClipboardImage(Some(png), _) => match crate::render::decode_png(&png.0) {
                Ok(snip) => {
                    self.set_snip(snip);
                    // The window opened before the size was known, and COSMIC
                    // keeps a mapped window's size: open one of the right size.
                    let away = self.put_editor_away().unwrap_or_else(Task::none);
                    return away.chain(self.open_editor());
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
            Message::CanvasSize(size) => {
                let fit = self.settle(size);
                // Debug: once the window has settled, save what it rendered.
                if let (Some(id), Some(_)) = (self.core.main_window_id(), dump_path()) {
                    let shot = cosmic::iced::Task::future(async {
                        tokio::time::sleep(std::time::Duration::from_millis(900)).await;
                    })
                    .discard()
                    .chain(window::screenshot(id).map(|s| cosmic::Action::App(Message::Dumped(s))));
                    return fit.chain(shot);
                }
                return fit;
            }
            Message::Dumped(shot) => {
                if let Some(path) = dump_path() {
                    let written = tiny_skia::Pixmap::from_vec(
                        shot.rgba.to_vec(),
                        tiny_skia::IntSize::from_wh(shot.size.width, shot.size.height)
                            .expect("a window has a size"),
                    )
                    .ok_or_else(|| "screenshot is not RGBA of its size".to_string())
                    .and_then(|p| render::encode_png(&p))
                    .and_then(|png| std::fs::write(&path, png).map_err(|e| e.to_string()));
                    log::warn!("dumped the window to {}: {written:?}", path.display());
                    return cosmic::iced::exit();
                }
            }
            Message::CloseRequested(id) => {
                if Some(id) == self.core.main_window_id() {
                    return self.close_window();
                }
                if Some(id) == self.clip_window {
                    // Closed on purpose: the copy goes with it. If an editor is
                    // open, iced attaches the clipboard to it instead.
                    self.holding_copy = false;
                    self.clip_window = self.core.main_window_id();
                    let close = window::close(id);
                    if self.clip_window.is_none() {
                        return close.chain(cosmic::iced::exit());
                    }
                    return close;
                }
                return window::close(id);
            }
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
        let mut items: Vec<Element<'_, Message>> = vec![
            button::icon(widget::icon::from_name("list-add-symbolic"))
                .tooltip("New snip (Ctrl+N)")
                .on_press(Message::New)
                .into(),
        ];
        items.extend(Tool::ALL.iter().map(|&tool| {
            button::icon(widget::icon::from_name(tool.icon()))
                .selected(tool == self.tool)
                .tooltip(tool.label())
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
            button::icon(widget::icon::from_name("document-save-symbolic"))
                .tooltip("Save (Ctrl+S)")
                .on_press(Message::Save)
                .into(),
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
    /// The canvas size last reported for this snip, so it is said once.
    reported: Option<(u32, u32, u32, u32)>,
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
        if let canvas::Event::Window(window::Event::RedrawRequested(_)) = event {
            let key = (
                self.app.snip.width(),
                self.app.snip.height(),
                bounds.width.round() as u32,
                bounds.height.round() as u32,
            );
            if state.reported == Some(key) {
                return None;
            }
            state.reported = Some(key);
            return Some(canvas::Action::publish(Message::CanvasSize(bounds.size())));
        }
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
            mouse::Event::ButtonPressed(mouse::Button::Left) if self.app.tool == Tool::Text => {
                let p = cursor.position_in(bounds)?;
                Some(canvas::Action::publish(Message::PlaceText(at(p))).and_capture())
            }
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
        let committed = self.app.doc.committed().len();
        let typing = self.app.doc.is_typing();
        for (i, stroke) in self.app.doc.all().enumerate() {
            draw_stroke(&mut frame, &fit, stroke, typing && i >= committed);
        }
        vec![frame.into_geometry()]
    }

    fn mouse_interaction(
        &self,
        _state: &Pointer,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        if cursor.is_over(bounds) && self.app.tool == Tool::Text {
            mouse::Interaction::Text
        } else if cursor.is_over(bounds) {
            mouse::Interaction::Crosshair
        } else {
            mouse::Interaction::default()
        }
    }
}

fn draw_stroke(frame: &mut Frame, fit: &Fit, stroke: &Stroke, typing: bool) {
    // A shape with no extent draws nothing, and the renderer logs a warning for
    // every such path on every frame ("empty paths and horizontal/vertical lines
    // cannot be filled"): the first point of a stroke, an arrow not yet dragged.
    let degenerate = match &stroke.shape {
        Shape::Path(points) => points.windows(2).all(|w| w[0] == w[1]),
        Shape::Arrow { start, end }
        | Shape::Rect { start, end }
        | Shape::Ellipse { start, end } => start == end,
        Shape::Text { .. } => false,
    };
    if degenerate {
        return;
    }
    if let Shape::Text { at, text, size } = &stroke.shape {
        // The same font and line height the export uses (render.rs), so the
        // text lands where it was shown. While it is being typed, a cursor.
        let content = if typing {
            format!("{text}|")
        } else {
            text.clone()
        };
        frame.fill_text(canvas::Text {
            content,
            position: fit.to_canvas(*at),
            color: to_color(stroke.rgba),
            size: cosmic::iced::Pixels(size * fit.scale),
            line_height: cosmic::iced::widget::text::LineHeight::Relative(render::TEXT_LINE_HEIGHT),
            font: cosmic::font::default(),
            ..canvas::Text::default()
        });
        return;
    }
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
        Shape::Ellipse { start, end } => {
            let a = fit.to_canvas(*start);
            let c = fit.to_canvas(*end);
            b.ellipse(canvas::path::arc::Elliptical {
                center: IPoint::new((a.x + c.x) / 2.0, (a.y + c.y) / 2.0),
                radii: cosmic::iced::Vector::new((c.x - a.x).abs() / 2.0, (c.y - a.y).abs() / 2.0),
                rotation: cosmic::iced::Radians(0.0),
                start_angle: cosmic::iced::Radians(0.0),
                end_angle: cosmic::iced::Radians(std::f32::consts::TAU),
            });
        }
        Shape::Text { .. } => {}
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
