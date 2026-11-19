use crate::{
    apps,
    bluetooth::{self, BluetoothAction, BluetoothState, BluetoothView, PairPrompt},
    blur::Blur,
    config::{self, AppConfig, CompiledKeybinds, ConfigStore, StartTarget},
    files,
    history::History,
    menu::{self, Menu, Mode, Target},
    motion::Motion,
    ring::{self, Geometry, Palette},
    settings::{self, AppearanceToggle, ColorField, KeybindField, SettingsAction, SettingsView},
    wifi::{self, WifiAction, WifiState, WifiView},
    windows,
};
use gtk::{
    cairo,
    gdk::{self, keys::constants as key},
    gio, glib,
    prelude::*,
};
use gtk_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    env,
    fs::{self, DirBuilder, OpenOptions},
    io::{self, Write},
    os::unix::{
        fs::{DirBuilderExt, OpenOptionsExt},
        net::{UnixListener, UnixStream},
    },
    path::{Path, PathBuf},
    rc::Rc,
    sync::mpsc,
    time::Duration,
};

struct InstanceGuard {
    socket_path: PathBuf,
    replaced: Rc<Cell<bool>>,
    _listener: UnixListener,
}

impl InstanceGuard {
    fn acquire<F>(on_replaced: F) -> Option<Self>
    where
        F: Fn() + 'static,
    {
        let dir = instance_runtime_dir()?;
        let guard = Self::acquire_in(&dir, on_replaced).ok()?;
        Some(guard)
    }

    fn acquire_in<F>(dir: &Path, on_replaced: F) -> io::Result<Self>
    where
        F: Fn() + 'static,
    {
        DirBuilder::new().recursive(true).mode(0o700).create(dir)?;
        let lock_path = dir.join("instance.lock");
        let socket_path = dir.join("instance.sock");

        let lock = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .open(&lock_path)?;
        lock.lock()?;

        Self::dismiss_existing(&socket_path);
        match fs::remove_file(&socket_path) {
            Ok(()) => {}
            Err(err) if err.kind() == io::ErrorKind::NotFound => {}
            Err(err) => return Err(err),
        }

        let listener = UnixListener::bind(&socket_path)?;
        let thread_listener = listener.try_clone()?;
        let (tx, rx) = mpsc::channel::<()>();
        std::thread::spawn(move || {
            if thread_listener.incoming().flatten().next().is_some() {
                let _ = tx.send(());
            }
        });

        let replaced = Rc::new(Cell::new(false));
        let replaced_flag = replaced.clone();
        glib::timeout_add_local(Duration::from_millis(15), move || match rx.try_recv() {
            Ok(()) => {
                replaced_flag.set(true);
                on_replaced();
                glib::ControlFlow::Break
            }
            Err(mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
            Err(mpsc::TryRecvError::Disconnected) => glib::ControlFlow::Break,
        });

        drop(lock);
        Ok(Self {
            socket_path,
            replaced,
            _listener: listener,
        })
    }

    fn dismiss_existing(socket_path: &Path) {
        let Ok(mut stream) = UnixStream::connect(socket_path) else {
            return;
        };
        let _ = stream.set_write_timeout(Some(Duration::from_millis(50)));
        let _ = stream.write_all(b"quit");
    }
}

impl Drop for InstanceGuard {
    fn drop(&mut self) {
        if !self.replaced.get() {
            let _ = fs::remove_file(&self.socket_path);
        }
    }
}

fn instance_runtime_dir() -> Option<PathBuf> {
    let base = env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| {
            env::var_os("XDG_STATE_HOME")
                .map(PathBuf::from)
                .filter(|path| path.is_absolute())
        })
        .or_else(|| env::var_os("HOME").map(|p| PathBuf::from(p).join(".local/state")))?;
    Some(base.join("recircles"))
}

fn quit_now() {
    for window in gtk::Window::list_toplevels() {
        window.hide();
        if let Ok(dialog) = window.downcast::<gtk::Dialog>() {
            dialog.response(gtk::ResponseType::Cancel);
        }
    }
    if let Some(display) = gdk::Display::default() {
        display.sync();
    }
    if gtk::main_level() > 0 {
        gtk::main_quit();
    }
}

pub fn run(
    store: ConfigStore,
    config: AppConfig,
    start: Option<StartTarget>,
) -> Result<(), String> {
    let palette = config.colors.to_palette()?;
    let keybinds = config.keybinds.compile()?;
    gdk::set_allowed_backends("wayland");
    glib::set_application_name("ReCircles");
    gtk::init().map_err(|_| {
        "Could not connect to a Wayland display. Run recircles from your Wayland session."
            .to_string()
    })?;
    // ReCircles renders its own custom caret and keeps the GTK Entry invisible
    // (opacity 0). Disable GTK's cursor-blink timer so it never wakes the frame
    // clock or invalidates (0, 0) every 600 ms.
    if let Some(settings) = gtk::Settings::default() {
        settings.set_gtk_cursor_blink(false);
    }
    if !gtk_layer_shell::is_supported() {
        return Err("This compositor does not advertise the Wayland layer-shell protocol.".into());
    }
    let display = gdk::Display::default().ok_or("No Wayland display")?;
    let history = History::load().map_err(|e| format!("Could not read usage history: {e}"))?;
    let monitors: Vec<_> = (0..display.n_monitors())
        .filter_map(|i| display.monitor(i))
        .collect();
    if monitors.is_empty() {
        return Err("No connected outputs".into());
    }
    let _instance_guard = InstanceGuard::acquire(quit_now);

    if let Some((x, y)) = crate::pointer::hyprland_position() {
        for monitor in &monitors {
            let rect = monitor.geometry();
            let local = (x - f64::from(rect.x()), y - f64::from(rect.y()));
            if (0.0..f64::from(rect.width())).contains(&local.0)
                && (0.0..f64::from(rect.height())).contains(&local.1)
            {
                let picker = Picker::new(
                    monitor, history, store, config, palette, keybinds, start,
                );
                picker.place(local);
                picker.window.show_all();
                gtk::main();
                return Ok(());
            }
        }
    }

    let surfaces: Vec<_> = monitors
        .iter()
        .map(|monitor| {
            Picker::new(
                monitor,
                history.clone(),
                store.clone(),
                config.clone(),
                palette,
                keybinds.clone(),
                start.clone(),
            )
        })
        .collect();
    // Wayland gives us the pointer position in surface coordinates on enter. Map
    // transparent surfaces on each output and keep only the one under the cursor.
    let active = Rc::new(Cell::new(None));
    for (i, picker) in surfaces.iter().enumerate() {
        let peers = surfaces.clone();
        let active = active.clone();
        picker.area.connect_enter_notify_event(move |_, event| {
            if active.get().is_none() {
                active.set(Some(i));
                for (n, peer) in peers.iter().enumerate() {
                    if n != i {
                        peer.window.hide();
                    }
                }
                peers[i].place(event.position());
            }
            glib::Propagation::Proceed
        });
    }
    for (i, picker) in surfaces.iter().enumerate() {
        if active.get().is_none_or(|chosen| chosen == i) {
            picker.window.show_all();
        }
    }
    // Keyboard-only seats have no pointer-enter event.
    let fallback = surfaces.clone();
    glib::timeout_add_local_once(Duration::from_millis(250), move || {
        if active.get().is_none() {
            active.set(Some(0));
            for p in &fallback[1..] {
                p.window.hide();
            }
            let p = &fallback[0];
            p.place((
                p.area.allocated_width() as f64 / 2.,
                p.area.allocated_height() as f64 / 2.,
            ));
        }
    });
    gtk::main();
    Ok(())
}

#[derive(Default)]
struct ScrollPager {
    accum: f64,
}

impl ScrollPager {
    fn reset(&mut self) {
        self.accum = 0.0;
    }

    fn smooth(&mut self, dx: f64, dy: f64, is_stop: bool, touchpad: bool) -> isize {
        if is_stop {
            self.reset();
            return 0;
        }
        let delta = if dy.abs() >= dx.abs() { dy } else { dx };
        if delta == 0.0 {
            return 0;
        }
        if self.accum != 0.0 && self.accum.signum() != delta.signum() {
            self.accum = 0.0;
        }
        self.accum += delta;
        let threshold = if touchpad { 3.0 } else { 1.0 };
        if self.accum.abs() >= threshold {
            let step = self.accum.signum() as isize;
            self.accum = 0.0;
            step
        } else {
            0
        }
    }
}

enum CustomLevel {
    Root,
    Submenu(String),
    InlineList(config::SubmenuConfig),
}

struct ActiveCustomMenu {
    name: String,
    config: config::MenuConfig,
    /// Submenu navigation stack within this menu.
    stack: Vec<CustomLevel>,
}

impl ActiveCustomMenu {
    fn new(name: String, config: config::MenuConfig) -> Self {
        Self {
            name,
            config,
            stack: vec![CustomLevel::Root],
        }
    }

    fn current_title(&self) -> &str {
        match self.stack.last() {
            None | Some(CustomLevel::Root) => self.config.display_title(&self.name),
            Some(CustomLevel::Submenu(key)) => self
                .config
                .submenus
                .get(key)
                .map_or(key.as_str(), |sub| sub.display_title(key)),
            Some(CustomLevel::InlineList(sub)) => sub.display_title(&self.name),
        }
    }

    fn current_layout(&self) -> config::MenuLayout {
        match self.stack.last() {
            None | Some(CustomLevel::Root) => self.config.layout,
            Some(CustomLevel::Submenu(key)) => self
                .config
                .submenus
                .get(key)
                .map_or(config::MenuLayout::Ring, |sub| sub.layout),
            Some(CustomLevel::InlineList(sub)) => sub.layout,
        }
    }

    fn current_prompt(&self) -> Option<&str> {
        match self.stack.last() {
            None | Some(CustomLevel::Root) => self.config.prompt.as_deref(),
            Some(CustomLevel::Submenu(key)) => self
                .config
                .submenus
                .get(key)
                .and_then(|sub| sub.prompt.as_deref()),
            Some(CustomLevel::InlineList(sub)) => sub.prompt.as_deref(),
        }
        .filter(|s| !s.trim().is_empty())
    }

    fn current_items(&self) -> Vec<config::MenuItem> {
        match self.stack.last() {
            None | Some(CustomLevel::Root) => self.config.resolved_items(),
            Some(CustomLevel::Submenu(key)) => self
                .config
                .submenus
                .get(key)
                .map_or_else(Vec::new, config::SubmenuConfig::resolved_items),
            Some(CustomLevel::InlineList(sub)) => sub.resolved_items(),
        }
    }

    fn current_input_action(
        &self,
    ) -> Option<(
        config::InputPromptConfig,
        Option<String>,
        Option<String>,
        bool,
        Option<bool>,
    )> {
        if self.current_layout() != config::MenuLayout::Input {
            return None;
        }
        match self.stack.last() {
            None | Some(CustomLevel::Root) => Some((
                self.config.input_prompt(&self.name),
                self.config.command.clone(),
                self.config.submenu.clone(),
                self.config.reload,
                Some(self.config.loading_animation),
            )),
            Some(CustomLevel::Submenu(key)) => {
                let sub = self.config.submenus.get(key)?;
                Some((
                    sub.input_prompt(key),
                    sub.command.clone(),
                    sub.submenu.clone(),
                    sub.reload,
                    sub.loading_animation,
                ))
            }
            Some(CustomLevel::InlineList(_)) => None,
        }
    }

    fn current_on_open(&self) -> Option<&str> {
        match self.stack.last() {
            None | Some(CustomLevel::Root) => self.config.on_open.as_deref(),
            Some(CustomLevel::Submenu(key)) => self
                .config
                .submenus
                .get(key)
                .and_then(|sub| sub.on_open.as_deref()),
            Some(CustomLevel::InlineList(sub)) => sub.on_open.as_deref(),
        }
    }

    fn current_loading_animation(&self) -> bool {
        match self.stack.last() {
            None | Some(CustomLevel::Root) => self.config.loading_animation,
            Some(CustomLevel::Submenu(key)) => self
                .config
                .submenus
                .get(key)
                .and_then(|sub| sub.loading_animation)
                .unwrap_or(self.config.loading_animation),
            Some(CustomLevel::InlineList(sub)) => sub
                .loading_animation
                .unwrap_or(self.config.loading_animation),
        }
    }

    fn submenu_refresh_info(&self, key: &str) -> Result<(Option<String>, bool), String> {
        let Some(sub) = self.config.submenus.get(key) else {
            return Err(format!("Unknown submenu {key:?}."));
        };
        let spin = sub
            .loading_animation
            .unwrap_or(self.config.loading_animation);
        Ok((sub.on_open.clone(), spin))
    }

    fn push_submenu(&mut self, key: String) -> Result<(), String> {
        if !self.config.submenus.contains_key(&key) {
            return Err(format!("Unknown submenu {key:?}."));
        }
        self.stack.push(CustomLevel::Submenu(key));
        Ok(())
    }

    fn push_inline_list(&mut self, sub: config::SubmenuConfig) {
        self.stack.push(CustomLevel::InlineList(sub));
    }

    fn pop_submenu(&mut self) -> bool {
        if self.stack.len() > 1 {
            self.stack.pop();
            true
        } else {
            false
        }
    }
}

enum InputCallback {
    BluetoothAlias,
    BluetoothPin {
        reply: mpsc::Sender<Option<String>>,
    },
    WifiPassword {
        ssid: String,
        saved_uuid: Option<String>,
    },
    WifiHiddenSsid,
    WifiHiddenPassword {
        ssid: String,
    },
    CustomAction {
        command: Option<String>,
        submenu: Option<String>,
        reload: bool,
        loading_animation: Option<bool>,
        pop_on_exit: bool,
    },
    SettingsColorHex(ColorField),
    SettingsKeybind(KeybindField),
    SettingsMainMenuLabel(usize),
    SettingsMainMenuIcon(usize),
}

struct ActiveInputPrompt {
    title: String,
    text: String,
    password: bool,
    allow_empty: bool,
    selected_button: Option<usize>,
    pressed_button: Option<usize>,
    saved_query: String,
    color: Option<ring::explorer::ColorPickerState>,
    callback: InputCallback,
}

enum CustomRefreshTarget {
    /// Enter a custom menu after its `on_open` command finishes (keeps the previous screen visible).
    Enter,
    /// Push a submenu after its refresh commands finish (keeps the parent screen visible).
    Submenu(String),
    /// Swap the reloaded config into the currently active custom menu/submenu.
    Current,
}

#[derive(Clone, Copy)]
enum ColorDrag {
    Sv,
    Hue,
}

struct Picker {
    window: gtk::Window,
    area: gtk::DrawingArea,
    input: gtk::Entry,
    composing: Cell<bool>,
    programmatic_input: Cell<bool>,
    typing_in_search: Cell<bool>,
    back_stack: RefCell<Vec<String>>,
    anchor: Cell<Option<(f64, f64)>>,
    store: ConfigStore,
    config: RefCell<AppConfig>,
    palette: Cell<Palette>,
    keybinds: RefCell<CompiledKeybinds>,
    start: Option<StartTarget>,
    standalone: bool,
    custom_menu: RefCell<Option<ActiveCustomMenu>>,
    custom_loading: Cell<bool>,
    custom_loading_spin: Cell<bool>,
    custom_request_id: Cell<u64>,
    input_prompt: RefCell<Option<ActiveInputPrompt>>,
    last_input_state: RefCell<Option<ring::explorer::InputState>>,
    menu: RefCell<Menu>,
    history: RefCell<History>,
    apps: RefCell<Option<Vec<apps::App>>>,
    windows: RefCell<Option<Vec<windows::Window>>>,
    wifi_state: RefCell<Option<WifiState>>,
    wifi_stack: RefCell<Vec<WifiView>>,
    bt_state: RefCell<Option<BluetoothState>>,
    bt_stack: RefCell<Vec<BluetoothView>>,
    settings_stack: RefCell<Vec<SettingsView>>,
    key_capture: Cell<Option<KeybindField>>,
    network_scanning: Cell<bool>,
    busy_message: RefCell<Option<String>>,
    op_gen: Cell<u64>,
    icons: RefCell<HashMap<String, Option<gtk::gdk_pixbuf::Pixbuf>>>,
    provider_error: RefCell<Option<String>>,
    file_worker: Cell<bool>,
    motion: RefCell<Motion>,
    loading_accent: Cell<Option<ring::Rgb>>,
    scroll: RefCell<ScrollPager>,
    thumb_grab: Cell<Option<f64>>,
    color_drag: Cell<Option<ColorDrag>>,
    ticking: Cell<bool>,
    painted_bounds: Cell<Option<cairo::RectangleInt>>,
    hover_position: Cell<Option<(f64, f64)>>,
    animate: Cell<bool>,
    animation_3d: Cell<bool>,
    high_contrast: Cell<bool>,
    solid: Cell<bool>,
    blur_enabled: Cell<bool>,
}

fn compute_appearance_flags(appearance: &config::AppearanceConfig) -> (bool, bool, bool, bool, bool) {
    let settings = gtk::Settings::default();
    let animate = appearance.animations
        && settings
            .as_ref()
            .is_none_or(|s| s.is_gtk_enable_animations())
        && std::env::var_os("RECIRCLES_REDUCED_MOTION").is_none();
    let animation_3d = appearance.animation_3d;
    let high_contrast = appearance.high_contrast
        || settings
            .and_then(|s| s.gtk_theme_name())
            .is_some_and(|s| s.to_ascii_lowercase().contains("highcontrast"));
    let solid = !appearance.translucent
        || high_contrast
        || std::env::var_os("RECIRCLES_REDUCED_TRANSPARENCY").is_some();
    let blur_enabled = appearance.blur && !solid;
    (animate, animation_3d, high_contrast, solid, blur_enabled)
}

#[derive(Default)]
struct WifiTaskOptions {
    rescan_after: bool,
    spin: bool,
    pop_view_on_success: bool,
    pop_to_root_on_success: bool,
}

impl Picker {
    fn new(
        monitor: &gdk::Monitor,
        history: History,
        store: ConfigStore,
        config: AppConfig,
        palette: Palette,
        keybinds: CompiledKeybinds,
        start: Option<StartTarget>,
    ) -> Rc<Self> {
        let window = gtk::Window::new(gtk::WindowType::Toplevel);
        window.set_title("ReCircles");
        window.set_app_paintable(true);
        window.set_decorated(false);
        if let Some(visual) = gtk::prelude::WidgetExt::screen(&window).and_then(|s| s.rgba_visual())
        {
            window.set_visual(Some(&visual));
        }
        window.init_layer_shell();
        window.set_namespace("recircles");
        window.set_layer(Layer::Overlay);
        window.set_monitor(monitor);
        window.set_exclusive_zone(-1);
        window.set_keyboard_mode(KeyboardMode::None);
        for edge in [Edge::Top, Edge::Right, Edge::Bottom, Edge::Left] {
            window.set_anchor(edge, true);
        }

        let area = gtk::DrawingArea::new();
        area.add_events(
            gdk::EventMask::ENTER_NOTIFY_MASK
                | gdk::EventMask::LEAVE_NOTIFY_MASK
                | gdk::EventMask::POINTER_MOTION_MASK
                | gdk::EventMask::BUTTON_PRESS_MASK
                | gdk::EventMask::BUTTON_RELEASE_MASK
                | gdk::EventMask::SCROLL_MASK
                | gdk::EventMask::SMOOTH_SCROLL_MASK,
        );
        let overlay = gtk::Overlay::new();
        overlay.add(&area);
        // Let GTK handle input methods, composed characters, clipboard and text
        // editing. The ring draws this entry's text in its own material.
        let input = gtk::Entry::new();
        input.set_width_chars(1);
        input.set_opacity(0.);
        input.set_halign(gtk::Align::Start);
        input.set_valign(gtk::Align::Start);
        if let Some(a) = input.accessible() {
            a.set_name("Search ReCircles");
        }
        overlay.add_overlay(&input);
        overlay.set_overlay_pass_through(&input, true);
        window.add(&overlay);

        let (animate, animation_3d, high_contrast, solid, blur_enabled) =
            compute_appearance_flags(&config.appearance);
        let home_targets = store.home_targets(&config);
        let menu = Menu::home_with(home_targets, config.appearance.overflow_layout);
        let standalone = start.is_some();
        let picker = Rc::new(Self {
            window,
            area,
            input,
            composing: Cell::new(false),
            programmatic_input: Cell::new(false),
            typing_in_search: Cell::new(false),
            back_stack: RefCell::new(Vec::new()),
            anchor: Cell::new(None),
            store,
            config: RefCell::new(config),
            palette: Cell::new(palette),
            keybinds: RefCell::new(keybinds),
            start,
            standalone,
            custom_menu: RefCell::new(None),
            custom_loading: Cell::new(false),
            custom_loading_spin: Cell::new(false),
            custom_request_id: Cell::new(0),
            input_prompt: RefCell::new(None),
            last_input_state: RefCell::new(None),
            menu: RefCell::new(menu),
            history: RefCell::new(history),
            apps: RefCell::new(None),
            windows: RefCell::new(None),
            wifi_state: RefCell::new(None),
            wifi_stack: RefCell::new(Vec::new()),
            bt_state: RefCell::new(None),
            bt_stack: RefCell::new(Vec::new()),
            settings_stack: RefCell::new(Vec::new()),
            key_capture: Cell::new(None),
            network_scanning: Cell::new(false),
            busy_message: RefCell::new(None),
            op_gen: Cell::new(0),
            icons: RefCell::new(HashMap::new()),
            provider_error: RefCell::new(None),
            file_worker: Cell::new(false),
            motion: RefCell::new(Motion::default()),
            loading_accent: Cell::new(None),
            scroll: RefCell::new(ScrollPager::default()),
            thumb_grab: Cell::new(None),
            color_drag: Cell::new(None),
            ticking: Cell::new(false),
            painted_bounds: Cell::new(None),
            hover_position: Cell::new(None),
            animate: Cell::new(animate),
            animation_3d: Cell::new(animation_3d),
            high_contrast: Cell::new(high_contrast),
            solid: Cell::new(solid),
            blur_enabled: Cell::new(blur_enabled),
        });
        picker.connect();
        picker
    }

    fn apply_config(&self, new_config: AppConfig) -> Result<(), String> {
        let palette = new_config.colors.to_palette()?;
        let keybinds = new_config.keybinds.compile()?;
        self.store.save_config(&new_config)?;
        let (animate, animation_3d, high_contrast, solid, blur_enabled) =
            compute_appearance_flags(&new_config.appearance);
        let overflow_layout = new_config.appearance.overflow_layout;
        self.config.replace(new_config);
        self.palette.set(palette);
        self.keybinds.replace(keybinds);
        self.animate.set(animate);
        self.animation_3d.set(animation_3d);
        self.high_contrast.set(high_contrast);
        self.solid.set(solid);
        self.blur_enabled.set(blur_enabled);
        self.menu.borrow_mut().set_overflow_layout(overflow_layout);
        self.painted_bounds.set(None);
        Ok(())
    }

    fn load_icon(&self, name: &str) -> Option<gtk::gdk_pixbuf::Pixbuf> {
        if let Some(cached) = self.icons.borrow().get(name) {
            return cached.clone();
        }
        let size = 48 * self.area.scale_factor();
        let path = if let Some(rest) = name.strip_prefix("~/") {
            std::env::var_os("HOME").map(|home| std::path::PathBuf::from(home).join(rest))
        } else {
            let p = std::path::Path::new(name);
            p.is_absolute().then(|| p.to_path_buf())
        };
        let loaded = if let Some(path) = path {
            gtk::gdk_pixbuf::Pixbuf::from_file_at_scale(path, size, size, true).ok()
        } else {
            gtk::IconTheme::default()
                .and_then(|theme| {
                    theme
                        .load_icon(name, size, gtk::IconLookupFlags::FORCE_SIZE)
                        .ok()
                })
                .flatten()
        };
        self.icons
            .borrow_mut()
            .insert(name.to_string(), loaded.clone());
        loaded
    }

    fn place(self: &Rc<Self>, position: (f64, f64)) {
        if self.animate.get() {
            self.motion.borrow_mut().open();
        }
        self.anchor.set(Some(position));
        self.window.set_keyboard_mode(KeyboardMode::Exclusive);
        self.input.grab_focus();
        if let Some(start) = self.start.clone() {
            match start {
                StartTarget::Builtin(mode) => self.enter(mode),
                StartTarget::Custom { name, config } => self.enter_custom_loaded(name, config),
            }
        } else {
            self.refresh();
        }
    }

    fn refresh(self: &Rc<Self>) {
        let has_input = self.input_prompt.borrow().is_some();
        let selected_btn = self
            .input_prompt
            .borrow()
            .as_ref()
            .and_then(|pr| pr.selected_button);
        let menu = self.menu.borrow();
        let items = menu
            .visible()
            .iter()
            .map(|target| ring::Item {
                label: target.label(),
                icon: target.icon(),
                pixbuf: match target {
                    Target::App(app) if !app.icon.is_empty() => self.load_icon(&app.icon),
                    Target::HomeEntry(entry) if !entry.icon.is_empty() => {
                        self.load_icon(&entry.icon)
                    }
                    Target::CustomItem(item) if !item.icon.is_empty() => {
                        self.load_icon(&item.icon)
                    }
                    Target::Bluetooth(item) if !item.theme_icon.is_empty() => {
                        self.load_icon(&item.theme_icon)
                    }
                    Target::Settings(item) if !item.theme_icon.is_empty() => {
                        self.load_icon(&item.theme_icon)
                    }
                    _ => None,
                },
                active: target.is_active(),
            })
            .collect();
        let mode = menu.mode;
        let loading = !has_input && self.is_loading(mode);
        let loading_3d = loading && self.animation_3d.get();
        let loading_2d = loading && !self.animation_3d.get();
        let list_active = !has_input
            && menu.uses_list()
            && !loading
            && (!menu.entries.is_empty() || !menu.query.is_empty());
        let list_rows = menu.visible().len().max(1);
        let box_active = !has_input && !list_active && menu.uses_box() && !loading;
        let empty = !has_input
            && !list_active
            && !box_active
            && !loading
            && !self.is_catalog_pending(mode)
            && menu.entries.is_empty();
        let selected = menu
            .selected
            .filter(|_| !has_input && !self.file_search_pending(mode));
        drop(menu);
        if loading {
            self.loading_accent.set(self.loading_accent_for(mode));
        }
        if has_input {
            self.sync_input_snapshot();
        }
        let moving = {
            let mut motion = self.motion.borrow_mut();
            if has_input {
                motion.set_input(true);
                motion.select_input_button(selected_btn);
            } else {
                if list_active {
                    motion.set_list(true, list_rows);
                } else {
                    motion.set_box(box_active);
                    motion.set_list(false, 1);
                }
                motion.set_input(false);
            }
            let animate = self.animate.get();
            motion.set_loading(loading_3d, animate)
                | motion.set_empty(empty || loading_2d, loading_2d, animate)
                | motion.update(items, selected, animate)
        };
        self.redraw(moving);
    }

    fn current_input_state(&self) -> Option<ring::explorer::InputState> {
        let prompt = self.input_prompt.borrow();
        let pr = prompt.as_ref()?;
        let char_count = pr.text.chars().count();
        let raw_pos = self.input.position();
        let cursor = if raw_pos < 0 {
            char_count
        } else {
            (raw_pos as usize).min(char_count)
        };
        let selection = self
            .input
            .selection_bounds()
            .map(|(s, e)| ((s.max(0) as usize).min(char_count), (e.max(0) as usize).min(char_count)));
        Some(ring::explorer::InputState {
            title: pr.title.clone(),
            text: pr.text.clone(),
            password: pr.password,
            cursor,
            selection,
            selected_button: pr.selected_button,
            pressed_button: pr.pressed_button,
            shortcuts: self.keybinds.borrow().number_shortcuts,
            accent: self.palette.get().accent_action,
            color: pr.color,
        })
    }

    fn sync_input_snapshot(&self) {
        if let Some(state) = self.current_input_state() {
            self.last_input_state.replace(Some(state));
        }
    }

    fn damage_rect(&self) -> Option<cairo::RectangleInt> {
        let raw = if let Some(bounds) = self.box_bounds() {
            bounds.damage_rect()
        } else {
            self.geometry()?.damage_rect(&self.motion.borrow())
        };
        clamp_rect(
            raw,
            self.area.allocated_width(),
            self.area.allocated_height(),
        )
    }

    fn queue_dirty(&self) {
        let w = self.area.allocated_width();
        let h = self.area.allocated_height();
        if w <= 0 || h <= 0 {
            self.area.queue_draw();
            return;
        }
        match (self.painted_bounds.get(), self.damage_rect()) {
            (Some(prev), Some(curr)) => {
                if let Some(dirty) = clamp_rect(union_rect(prev, curr), w, h) {
                    self.area
                        .queue_draw_area(dirty.x(), dirty.y(), dirty.width(), dirty.height());
                }
            }
            _ => self.area.queue_draw(),
        }
    }

    fn redraw(self: &Rc<Self>, moving: bool) {
        self.queue_dirty();
        if moving && !self.ticking.replace(true) {
            let p = self.clone();
            let previous = Cell::new(None);
            let started = glib::monotonic_time();
            let mapped_at_start = self.window.is_mapped();
            self.area.add_tick_callback(move |_, clock| {
                let now = clock.frame_time();
                let refresh_us = clock
                    .current_timings()
                    .and_then(|timings| timings.refresh_interval())
                    .map_or(0, |interval| interval.get() as i64);
                let dt = tick_dt(&previous, now, started, mapped_at_start, refresh_us);
                let mut moving = p.motion.borrow_mut().advance(dt);
                if let Some(position) = p.hover_position.get() {
                    if p.input_prompt.borrow().is_some() {
                        let hit_btn = p.box_bounds().and_then(|b| b.hit_input_button(position));
                        let mut prompt = p.input_prompt.borrow_mut();
                        if let Some(pr) = prompt.as_mut()
                            && pr.selected_button != hit_btn
                        {
                            pr.selected_button = hit_btn;
                            moving |= p.motion.borrow_mut().select_input_button(hit_btn);
                        }
                    } else {
                        let hit = p.hit(position);
                        let mut menu = p.menu.borrow_mut();
                        if menu.selected != hit && !p.file_search_pending(menu.mode) {
                            menu.selected = hit;
                            moving |= p.motion.borrow_mut().select(hit);
                        }
                    }
                }
                p.queue_dirty();
                if moving {
                    glib::ControlFlow::Continue
                } else {
                    p.ticking.set(false);
                    glib::ControlFlow::Break
                }
            });
        }
    }

    fn geometry(&self) -> Option<Geometry> {
        let (x, y) = self.anchor.get()?;
        Some(Geometry::new(
            self.area.allocated_width() as f64,
            self.area.allocated_height() as f64,
            x,
            y,
        ))
    }

    fn box_bounds(&self) -> Option<ring::explorer::Bounds> {
        let menu = self.menu.borrow();
        let motion = self.motion.borrow();
        if motion.loading_pose().is_some() {
            return None;
        }
        let has_input = self.input_prompt.borrow().is_some();
        let uses_list = menu.uses_list() && (!menu.entries.is_empty() || !menu.query.is_empty());
        if !has_input && !menu.uses_box() && !uses_list && motion.panel_mix() <= 0.001 {
            return None;
        }
        let has_query = if let Some(pr) = self.input_prompt.borrow().as_ref() {
            !pr.saved_query.is_empty()
        } else {
            !menu.query.is_empty()
        };
        let is_color_picker = self
            .input_prompt
            .borrow()
            .as_ref()
            .map_or_else(
                || {
                    self.last_input_state
                        .borrow()
                        .as_ref()
                        .is_some_and(|s| s.color.is_some())
                },
                |pr| pr.color.is_some(),
            );
        let size = (
            self.area.allocated_width() as f64,
            self.area.allocated_height() as f64,
        );
        let anchor = self.anchor.get()?;
        Some(if is_color_picker {
            ring::explorer::Bounds::new_with_input_height(
                size,
                anchor,
                has_query,
                &motion,
                ring::explorer::COLOR_INPUT_H,
            )
        } else {
            ring::explorer::Bounds::new(size, anchor, has_query, &motion)
        })
    }

    fn pointer_angle(&self, (x, y): (f64, f64)) -> Option<f64> {
        self.geometry()?.angle(x, y, self.motion.borrow().scale())
    }

    fn hit(&self, position: (f64, f64)) -> Option<usize> {
        if self.input_prompt.borrow().is_some() {
            return None;
        }
        let (uses_list, uses_box, total) = {
            let menu = self.menu.borrow();
            let uses_list =
                menu.uses_list() && (!menu.entries.is_empty() || !menu.query.is_empty());
            (uses_list, menu.uses_box(), menu.entries.len())
        };
        let (box_mix, list_mix) = {
            let motion = self.motion.borrow();
            (motion.box_mix(), motion.list_mix())
        };
        if uses_list || (uses_box && list_mix > box_mix) {
            return self
                .box_bounds()?
                .hit_list(position, total, &self.motion.borrow());
        }
        if uses_box {
            return self.box_bounds()?.hit(position, &self.motion.borrow());
        }
        if (box_mix > 0.12 || list_mix > 0.12)
            && let Some(bounds) = self.box_bounds()
        {
            let motion = self.motion.borrow();
            if list_mix > box_mix {
                return bounds.hit_list(position, total, &motion);
            }
            return bounds.hit(position, &motion);
        }
        self.motion.borrow().hit(self.pointer_angle(position)?)
    }

    fn dismiss_at(&self, position: (f64, f64)) -> bool {
        if let Some(bounds) = self.box_bounds() {
            return !bounds.contains(position);
        }
        self.pointer_angle(position).is_none()
            || (self.menu.borrow().visible().is_empty()
                && self.motion.borrow().sectors().next().is_none())
    }

    fn hover(self: &Rc<Self>, position: Option<(f64, f64)>) {
        let previous = self.hover_position.replace(position);
        if position.is_none() && previous.is_none() {
            return;
        }
        if self.input_prompt.borrow().is_some() {
            let hit_btn = position.and_then(|pos| self.box_bounds()?.hit_input_button(pos));
            let changed = {
                let mut prompt = self.input_prompt.borrow_mut();
                if let Some(pr) = prompt.as_mut()
                    && pr.selected_button != hit_btn
                {
                    pr.selected_button = hit_btn;
                    true
                } else {
                    false
                }
            };
            if !changed {
                return;
            }
            self.sync_input_snapshot();
            let mut motion = self.motion.borrow_mut();
            let moving = motion.select_input_button(hit_btn);
            let animate = self.animate.get();
            if !animate {
                motion.advance(10.0);
            }
            drop(motion);
            self.redraw(moving && animate);
            return;
        }
        let selected = position.and_then(|pos| self.hit(pos));
        let mut menu = self.menu.borrow_mut();
        if menu.selected == selected {
            return;
        }
        menu.selected = selected;
        if selected.is_some()
            && matches!(menu.mode, Mode::Wifi | Mode::Bluetooth | Mode::Settings)
            && self.provider_error.borrow().is_none()
        {
            menu.message.clear();
        }
        let focus = selected.filter(|_| !self.file_search_pending(menu.mode));
        drop(menu);
        let mut motion = self.motion.borrow_mut();
        let moving = motion.select(focus);
        let animate = self.animate.get();
        if !animate {
            motion.advance(10.0);
        }
        drop(motion);
        self.redraw(moving && animate);
    }

    fn drag_thumb(self: &Rc<Self>, position: (f64, f64), grab: f64) {
        if let Some(bounds) = self.box_bounds() {
            let mut menu = self.menu.borrow_mut();
            let before = menu.first_visible;
            if menu.uses_list() {
                let first = bounds.dragged_list_first(position, grab, menu.entries.len());
                menu.scroll_list_to(first);
            } else {
                let first = bounds.dragged_first(position, grab, menu.entries.len());
                menu.scroll_box_to(first);
            }
            let changed = menu.first_visible != before;
            drop(menu);
            if changed {
                self.refresh();
            }
        }
    }

    fn set_input(&self, text: &str) {
        self.programmatic_input.set(true);
        self.typing_in_search.set(false);
        self.input.set_text(text);
        self.input.set_position(-1);
        self.programmatic_input.set(false);
    }

    fn push_back(&self, query: &str) {
        let mut stack = self.back_stack.borrow_mut();
        if stack.last().map(String::as_str) != Some(query) {
            stack.push(query.to_owned());
        }
    }

    fn can_suggest(&self, query: &str) -> bool {
        !self.composing.get()
            && self.input.position() as usize == query.chars().count()
            && self.input.selection_bounds().is_none()
    }

    fn file_search_pending(&self, mode: Mode) -> bool {
        mode == Mode::Files && self.file_worker.get()
    }

    fn is_loading(&self, mode: Mode) -> bool {
        if self.provider_error.borrow().is_some() {
            return false;
        }
        match mode {
            Mode::Wifi => self.wifi_state.borrow().is_none() || self.network_scanning.get(),
            Mode::Bluetooth => self.bt_state.borrow().is_none() || self.network_scanning.get(),
            Mode::Custom => self.custom_loading.get() && self.custom_loading_spin.get(),
            _ => false,
        }
    }

    fn is_catalog_pending(&self, mode: Mode) -> bool {
        if self.provider_error.borrow().is_some() {
            return false;
        }
        match mode {
            Mode::Home | Mode::Settings => false,
            Mode::Files => self.file_worker.get(),
            Mode::Apps => self.apps.borrow().is_none(),
            Mode::Windows => self.windows.borrow().is_none(),
            Mode::Wifi => self.wifi_state.borrow().is_none() || self.network_scanning.get(),
            Mode::Bluetooth => self.bt_state.borrow().is_none() || self.network_scanning.get(),
            Mode::Custom => self.custom_loading.get(),
        }
    }

    fn loading_accent_for(&self, mode: Mode) -> Option<ring::Rgb> {
        let palette = self.palette.get();
        match mode {
            Mode::Wifi => Some(palette.accent_wifi),
            Mode::Bluetooth => Some(palette.accent_bluetooth),
            Mode::Custom | Mode::Settings => Some(palette.accent_custom),
            _ => None,
        }
    }

    fn connect(self: &Rc<Self>) {
        // GTK can give a remapped window a new wl_surface. Bind the effect
        // once mapped (falling back to the first painted frame) and release it on unmap.
        let blur: Rc<RefCell<Option<Option<Blur>>>> = Rc::new(RefCell::new(None));
        let mapped_blur = blur.clone();
        let p = self.clone();
        self.window.connect_unmap(move |_| {
            mapped_blur.borrow_mut().take();
            p.painted_bounds.set(None);
        });
        let pre_blur = blur.clone();
        let p = self.clone();
        // GTK3 on Wayland emits `map` (not X11's `map-event`) once `wl_surface` exists.
        self.window.connect_map(move |_| {
            if p.blur_enabled.get() {
                let mut binding = pre_blur.borrow_mut();
                if binding.is_none()
                    && let Some(effect) = Blur::attach(&p.window)
                {
                    *binding = Some(Some(effect));
                }
            }
        });
        let size_alloc = Cell::new((0, 0));
        let p = self.clone();
        self.area.connect_size_allocate(move |_, alloc| {
            let size = (alloc.width(), alloc.height());
            if size_alloc.replace(size) != size {
                p.painted_bounds.set(None);
            }
        });
        let p = self.clone();
        self.area.connect_draw(move |_, cr| {
            cr.set_operator(cairo::Operator::Source);
            cr.set_source_rgba(0., 0., 0., 0.);
            let _ = cr.paint();
            cr.set_operator(cairo::Operator::Over);
            if let Some(g) = p.geometry() {
                let bounds = p.box_bounds();
                let current_rect = clamp_rect(
                    match bounds {
                        Some(bounds) => bounds.damage_rect(),
                        None => g.damage_rect(&p.motion.borrow()),
                    },
                    p.area.allocated_width(),
                    p.area.allocated_height(),
                );
                let clip_rects = clip_rects_from_context(cr);
                // Skip unrelated partial exposes (such as the invisible Entry
                // at (0, 0)) that do not touch the picker at all.
                if let (Some(painted), Some(curr), Some(rects)) =
                    (p.painted_bounds.get(), current_rect, clip_rects.as_deref())
                    && !clip_rects_intersect(rects, curr)
                    && !clip_rects_intersect(rects, painted)
                {
                    return glib::Propagation::Stop;
                }
                let blur_wanted = p.blur_enabled.get();
                let mut binding = blur.borrow_mut();
                match (blur_wanted, binding.as_ref()) {
                    (false, Some(Some(_))) | (true, Some(None)) => {
                        *binding = None;
                    }
                    _ => {}
                }
                let effect = binding
                    .get_or_insert_with(|| {
                        if !blur_wanted {
                            None
                        } else {
                            Blur::attach(&p.window)
                        }
                    })
                    .as_mut();
                let blur_available = effect.as_deref().is_some_and(Blur::available);
                let menu = p.menu.borrow();
                let pending = p.file_search_pending(menu.mode);
                let loading = p.is_loading(menu.mode);
                let palette = p.palette.get();
                let view = ring::View {
                    motion: &p.motion.borrow(),
                    palette: &palette,
                    count: menu.visible().len(),
                    pressed: menu.pressed.filter(|_| !pending).map(|(i, _)| i),
                    title: menu.title(),
                    query: &menu.query,
                    suggestion: if !pending && p.can_suggest(&menu.query) {
                        menu.suggestion().map_or("", |(_, suffix)| suffix)
                    } else {
                        ""
                    },
                    status: &if let Some(field) = p.key_capture.get() {
                        format!(
                            "Press a key combination for {}… (Esc to cancel)",
                            field.label()
                        )
                    } else if let Some(busy) = p.busy_message.borrow().as_ref() {
                        busy.clone()
                    } else if pending || loading {
                        String::new()
                    } else {
                        menu.status()
                    },
                    page: (menu.pages() > 1).then(|| (menu.page() + 1, menu.pages())),
                    high_contrast: p.high_contrast.get(),
                    solid: p.solid.get(),
                    blur_available,
                    loading_accent: p.loading_accent.get(),
                };
                let result = match bounds {
                    Some(bounds) => {
                        let motion_ref = p.motion.borrow();
                        let has_input = p.input_prompt.borrow().is_some();
                        let uses_list = menu.uses_list()
                            && (!menu.entries.is_empty() || !menu.query.is_empty());
                        let draw_as_input = has_input
                            || (!menu.uses_box()
                                && !uses_list
                                && motion_ref.input_mix() > 0.001
                                && p.last_input_state.borrow().is_some())
                            || (motion_ref.input_mix()
                                > motion_ref.box_mix().max(motion_ref.list_mix())
                                && p.last_input_state.borrow().is_some());
                        let draw_as_list = !draw_as_input
                            && (uses_list || motion_ref.list_mix() > motion_ref.box_mix());
                        drop(motion_ref);
                        if draw_as_input {
                            let state = p
                                .current_input_state()
                                .or_else(|| p.last_input_state.borrow().clone())
                                .unwrap();
                            ring::explorer::draw_input(cr, bounds, &view, &state)
                        } else if draw_as_list {
                            ring::explorer::draw_list(
                                cr,
                                bounds,
                                &view,
                                &ring::explorer::ListState {
                                    first: menu.first_visible,
                                    total: menu.entries.len(),
                                    selected: menu.selected.filter(|_| !pending),
                                    dragging_thumb: p.thumb_grab.get().is_some(),
                                },
                            )
                        } else {
                            ring::explorer::draw(
                                cr,
                                bounds,
                                &view,
                                &ring::explorer::GridState {
                                    first: menu.first_visible,
                                    total: menu.entries.len(),
                                    selected: menu.selected.filter(|_| !pending),
                                    shortcuts: p.keybinds.borrow().number_shortcuts,
                                    dragging_thumb: p.thumb_grab.get().is_some(),
                                },
                            )
                        }
                    }
                    None => Ok(ring::draw(cr, &g, &view)),
                };
                let region = result.unwrap_or_else(|err| {
                    eprintln!("ReCircles could not draw the picker: {err}");
                    cairo::Region::create()
                });
                if let Some(effect) = effect {
                    effect.update(&region);
                }
                let full_surface = cairo::RectangleInt::new(
                    0,
                    0,
                    p.area.allocated_width(),
                    p.area.allocated_height(),
                );
                p.painted_bounds.set(next_painted_bounds(
                    p.painted_bounds.get(),
                    current_rect,
                    clip_rects.as_deref(),
                    full_surface,
                ));
            } else {
                p.painted_bounds.set(None);
            }
            glib::Propagation::Stop
        });
        let p = self.clone();
        self.area.connect_motion_notify_event(move |_, event| {
            if let Some(drag) = p.color_drag.get() {
                if let Some(bounds) = p.box_bounds() {
                    match drag {
                        ColorDrag::Sv => {
                            let (s, v) = bounds.color_sv_at(event.position());
                            p.set_color_sv(s, v);
                        }
                        ColorDrag::Hue => {
                            let h = bounds.color_hue_at(event.position());
                            p.set_color_hue(h);
                        }
                    }
                }
                return glib::Propagation::Stop;
            }
            if let Some(grab) = p.thumb_grab.get() {
                p.drag_thumb(event.position(), grab);
                return glib::Propagation::Stop;
            }
            p.hover(Some(event.position()));
            glib::Propagation::Stop
        });
        let p = self.clone();
        self.area.connect_leave_notify_event(move |_, _| {
            p.hover(None);
            glib::Propagation::Stop
        });
        let p = self.clone();
        self.area.connect_button_press_event(move |_, event| {
            if p.input_prompt.borrow().is_some() {
                if event.button() == 1 {
                    let is_color = p
                        .input_prompt
                        .borrow()
                        .as_ref()
                        .is_some_and(|pr| pr.color.is_some());
                    if is_color
                        && let Some(bounds) = p.box_bounds()
                    {
                        if let Some((s, v)) = bounds.hit_color_sv(event.position()) {
                            p.color_drag.set(Some(ColorDrag::Sv));
                            p.set_color_sv(s, v);
                            p.input.grab_focus();
                            return glib::Propagation::Stop;
                        }
                        if let Some(h) = bounds.hit_color_hue(event.position()) {
                            p.color_drag.set(Some(ColorDrag::Hue));
                            p.set_color_hue(h);
                            p.input.grab_focus();
                            return glib::Propagation::Stop;
                        }
                    }
                    let hit_btn = p
                        .box_bounds()
                        .and_then(|b| b.hit_input_button(event.position()));
                    let mut prompt = p.input_prompt.borrow_mut();
                    if let Some(pr) = prompt.as_mut() {
                        pr.selected_button = hit_btn;
                        pr.pressed_button = hit_btn;
                    }
                    drop(prompt);
                    p.input.grab_focus();
                    p.refresh();
                }
                return glib::Propagation::Stop;
            }
            if event.button() == 1
                && let Some(bounds) = p.box_bounds()
            {
                let menu = p.menu.borrow();
                let grab = if menu.uses_list() {
                    bounds.grab_list_thumb(event.position(), menu.first_visible, menu.entries.len())
                } else {
                    bounds.grab_thumb(event.position(), menu.first_visible, menu.entries.len())
                };
                drop(menu);
                if let Some(grab) = grab {
                    p.thumb_grab.set(Some(grab));
                    p.hover_position.set(None);
                    p.drag_thumb(event.position(), grab);
                    p.queue_dirty();
                    return glib::Propagation::Stop;
                }
            }
            if matches!(event.button(), 1 | 3) {
                let hit = p.hit(event.position());
                let mut m = p.menu.borrow_mut();
                m.selected = hit;
                m.pressed = hit.map(|i| (i, event.button()));
                drop(m);
                p.refresh();
            }
            glib::Propagation::Stop
        });
        let p = self.clone();
        self.area.connect_button_release_event(move |_, event| {
            if p.input_prompt.borrow().is_some() {
                if event.button() == 1 {
                    if p.color_drag.take().is_some() {
                        p.sync_input_snapshot();
                        p.queue_dirty();
                        return glib::Propagation::Stop;
                    }
                    let hit_btn = p
                        .box_bounds()
                        .and_then(|b| b.hit_input_button(event.position()));
                    let pressed_btn = p
                        .input_prompt
                        .borrow_mut()
                        .as_mut()
                        .and_then(|pr| pr.pressed_button.take());
                    if let Some(btn) = pressed_btn {
                        if hit_btn == Some(btn) {
                            if btn == 0 {
                                p.submit_input_prompt();
                            } else {
                                p.cancel_input_prompt();
                            }
                        } else {
                            p.refresh();
                        }
                    } else if p.dismiss_at(event.position()) {
                        p.cancel_input_prompt();
                        gtk::main_quit();
                    }
                }
                return glib::Propagation::Stop;
            }
            if event.button() == 1 && p.thumb_grab.take().is_some() {
                p.queue_dirty();
                return glib::Propagation::Stop;
            }
            let hit = p.hit(event.position());
            let pressed = p.menu.borrow_mut().pressed.take();
            if pressed.is_some() {
                p.refresh();
            }
            if let Some(i) = hit {
                if pressed == Some((i, event.button())) {
                    p.activate(i, event.button() == 3);
                }
            } else if pressed.is_none()
                && event.button() == 1
                && p.dismiss_at(event.position())
            {
                gtk::main_quit();
            }
            glib::Propagation::Stop
        });
        let p = self.clone();
        self.area.connect_scroll_event(move |_, event| {
            if p.input_prompt.borrow().is_some() {
                let is_color = p
                    .input_prompt
                    .borrow()
                    .as_ref()
                    .is_some_and(|pr| pr.color.is_some());
                if is_color {
                    let dir = match event.direction() {
                        gdk::ScrollDirection::Up | gdk::ScrollDirection::Right => 1.0,
                        gdk::ScrollDirection::Down | gdk::ScrollDirection::Left => -1.0,
                        gdk::ScrollDirection::Smooth => {
                            let (_, dy) = event.delta();
                            if dy < 0.0 { 1.0 } else if dy > 0.0 { -1.0 } else { 0.0 }
                        }
                        _ => 0.0,
                    };
                    if dir != 0.0 {
                        let step = if event.state().contains(gdk::ModifierType::SHIFT_MASK) {
                            24.0
                        } else {
                            6.0
                        };
                        let cur_h = p
                            .input_prompt
                            .borrow()
                            .as_ref()
                            .and_then(|pr| pr.color.map(|cp| cp.h))
                            .unwrap_or(0.0);
                        p.set_color_hue((cur_h + dir * step).rem_euclid(360.0));
                    }
                }
                return glib::Propagation::Stop;
            }
            let step = match event.direction() {
                gdk::ScrollDirection::Up | gdk::ScrollDirection::Left => {
                    p.scroll.borrow_mut().reset();
                    -1
                }
                gdk::ScrollDirection::Down | gdk::ScrollDirection::Right => {
                    p.scroll.borrow_mut().reset();
                    1
                }
                gdk::ScrollDirection::Smooth => {
                    let (dx, dy) = event.delta();
                    let touchpad = event
                        .source_device()
                        .is_some_and(|d| d.source() == gdk::InputSource::Touchpad);
                    p.scroll
                        .borrow_mut()
                        .smooth(dx, dy, event.is_stop(), touchpad)
                }
                _ => 0,
            };
            if step != 0 && !p.file_search_pending(p.menu.borrow().mode) {
                p.hover_position.set(Some(event.position()));
                let mut menu = p.menu.borrow_mut();
                let before = menu.first_visible;
                if menu.uses_list() {
                    let first = before.saturating_add_signed(step);
                    menu.scroll_list_to(first);
                } else if menu.uses_box() {
                    let first = before.saturating_add_signed(step * menu::GRID_COLUMNS as isize);
                    menu.scroll_box_to(first);
                } else {
                    menu.turn_page(step);
                }
                let changed = menu.first_visible != before;
                drop(menu);
                if changed {
                    p.refresh();
                    if !p.animate.get() {
                        let hit = p.hit(event.position());
                        p.menu.borrow_mut().selected = hit;
                        p.refresh();
                    }
                }
            }
            glib::Propagation::Stop
        });
        let p = self.clone();
        self.window
            .connect_key_press_event(move |_, event| p.key(event));
        let p = self.clone();
        self.input.connect_changed(move |entry| {
            let text = entry.text();
            let programmatic = p.programmatic_input.replace(false);
            if p.input_prompt.borrow().is_some() {
                let mut prompt = p.input_prompt.borrow_mut();
                if let Some(pr) = prompt.as_mut() {
                    if pr.text == text.as_str() {
                        return;
                    }
                    pr.text = text.into();
                    if let Some(cp) = pr.color.as_mut()
                        && let Some((r, g, b)) = config::parse_hex_rgb_u8(&pr.text)
                    {
                        let next = ring::explorer::ColorPickerState::from_rgb(r, g, b);
                        if next.s > 1e-4 {
                            cp.h = next.h;
                        }
                        cp.s = next.s;
                        cp.v = next.v;
                    }
                }
                drop(prompt);
                p.sync_input_snapshot();
                p.queue_dirty();
                return;
            }
            let was_typing = p.typing_in_search.replace(!programmatic && !text.is_empty());
            {
                let mut menu = p.menu.borrow_mut();
                if menu.mode == Mode::Home || menu.query == text.as_str() {
                    return;
                }
                if !programmatic && !text.is_empty() && !was_typing {
                    p.push_back(&menu.query);
                }
                menu.query = text.into();
            }
            p.filter();
        });
        let p = self.clone();
        self.input.connect_cursor_position_notify(move |_| {
            if p.input_prompt.borrow().is_some() {
                p.sync_input_snapshot();
                p.queue_dirty();
            } else if !p.menu.borrow().query.is_empty() {
                p.queue_dirty();
            }
        });
        let p = self.clone();
        self.input.connect_selection_bound_notify(move |_| {
            if p.input_prompt.borrow().is_some() {
                p.sync_input_snapshot();
                p.queue_dirty();
            } else if !p.menu.borrow().query.is_empty() {
                p.queue_dirty();
            }
        });
        let p = self.clone();
        self.input.connect_preedit_changed(move |_, text| {
            p.composing.set(!text.is_empty());
            p.refresh();
        });
        self.window.connect_delete_event(|_, _| {
            gtk::main_quit();
            glib::Propagation::Stop
        });
    }

    fn key(self: &Rc<Self>, event: &gdk::EventKey) -> glib::Propagation {
        use gdk::ModifierType as Mod;
        self.hover_position.set(None);
        if let Some(field) = self.key_capture.get() {
            return self.handle_key_capture(field, event);
        }
        if self.input_prompt.borrow().is_some() {
            return self.key_in_input_prompt(event);
        }
        let pressed = event.keyval();
        let state = event.state();
        let kb = self.keybinds.borrow().clone();
        let accept_completion = kb.accept_completion.matches(event);
        let undo_completion = kb.undo_completion.matches(event);
        let tab = matches!(pressed, key::Tab | key::ISO_Left_Tab);
        if self.composing.get() && (tab || accept_completion || undo_completion) {
            return glib::Propagation::Proceed;
        }
        let (mode, query, completed) = {
            let mut menu = self.menu.borrow_mut();
            let completed = if self.file_search_pending(menu.mode) {
                None
            } else if accept_completion && self.can_suggest(&menu.query) {
                let index = menu.suggestion().map(|(index, _)| index);
                index.and_then(|index| menu.accept_completion(index))
            } else {
                None
            };
            if !event.is_modifier() {
                menu.end_completion();
            }
            (menu.mode, menu.query.clone(), completed)
        };
        if let Some(text) = completed {
            // The menu already holds the completed query and its selected result.
            if text != query {
                self.push_back(&query);
            }
            self.set_input(&text);
            let completed_folder = {
                let menu = self.menu.borrow();
                menu.selected.is_some_and(|index| {
                    matches!(menu.visible().get(index), Some(Target::File(file)) if file.is_dir)
                })
            };
            if completed_folder {
                self.filter();
            } else {
                self.refresh();
            }
            return glib::Propagation::Stop;
        }
        if undo_completion && self.can_suggest(&query) {
            if mode == Mode::Files {
                if let Some(parent) = files::parent_query(&query) {
                    self.push_back(&query);
                    self.set_input(&parent);
                    return glib::Propagation::Stop;
                }
            } else if mode != Mode::Home && !self.file_search_pending(mode) {
                let restored = self.menu.borrow_mut().undo_completion();
                if let Some(text) = restored {
                    if self.back_stack.borrow().last().map(String::as_str) == Some(text.as_str()) {
                        self.back_stack.borrow_mut().pop();
                    }
                    self.set_input(&text);
                    self.typing_in_search.set(!text.is_empty());
                    self.refresh();
                    return glib::Propagation::Stop;
                }
            }
        }
        if kb.number_shortcuts
            && !state.intersects(Mod::SUPER_MASK | Mod::META_MASK)
            && let Some(index) = number_key(event)
        {
            let ctrl = state.contains(Mod::CONTROL_MASK);
            let alt = state.contains(Mod::MOD1_MASK);
            let shift = state.contains(Mod::SHIFT_MASK);
            let plain_number = !shift || number_from_keyval(pressed).is_some();
            if ctrl
                || alt
                || (plain_number
                    && !self.composing.get()
                    && (mode == Mode::Home || !self.typing_in_search.get()))
            {
                if !self.file_search_pending(mode) {
                    let valid = {
                        let mut menu = self.menu.borrow_mut();
                        if index < menu.visible().len() {
                            menu.selected = Some(index);
                            menu.pressed = None;
                            true
                        } else {
                            false
                        }
                    };
                    if valid {
                        if ctrl {
                            self.activate(index, shift);
                        }
                        self.refresh();
                    }
                }
                return glib::Propagation::Stop;
            }
        }
        if kb.home.matches(event) {
            if mode == Mode::Home || self.standalone {
                gtk::main_quit();
            } else {
                self.enter(Mode::Home);
            }
        } else if kb.back.matches(event) {
            if mode == Mode::Home {
                gtk::main_quit();
            } else {
                self.step_back(mode, &query);
            }
        } else if kb.open_current_folder.matches(event) {
            if mode == Mode::Files {
                self.open_current_folder(&query);
            }
        } else if kb.secondary_activate.matches(event) {
            let i = self.menu.borrow().selected.unwrap_or(0);
            self.activate(i, true);
        } else if kb.activate.matches(event) {
            let i = self.menu.borrow().selected.unwrap_or(0);
            self.activate(i, false);
        } else if kb.page_previous.matches(event) {
            if !self.file_search_pending(mode) {
                self.menu.borrow_mut().turn_page(-1);
            }
        } else if kb.page_next.matches(event) {
            if !self.file_search_pending(mode) {
                self.menu.borrow_mut().turn_page(1);
            }
        } else if kb.previous.matches(event) {
            self.menu.borrow_mut().step(-1);
        } else if kb.next.matches(event) {
            self.menu.borrow_mut().step(1);
        } else if kb.clear_query.matches(event) {
            if !query.is_empty() {
                self.push_back(&query);
            }
            self.set_input("");
        } else if mode == Mode::Home {
            return glib::Propagation::Stop;
        } else {
            return glib::Propagation::Proceed;
        }
        self.refresh();
        glib::Propagation::Stop
    }

    fn key_in_input_prompt(self: &Rc<Self>, event: &gdk::EventKey) -> glib::Propagation {
        use gdk::ModifierType as Mod;
        let state = event.state();
        let kb = self.keybinds.borrow().clone();
        if self.composing.get() {
            return glib::Propagation::Proceed;
        }
        // In string input mode, Ctrl+1 / Alt+1 triggers Done (1) and Ctrl+2 / Alt+2
        // triggers Cancel (2), leaving plain digits 1 and 2 free to be typed directly.
        if kb.number_shortcuts
            && !state.intersects(Mod::SUPER_MASK | Mod::META_MASK)
            && (state.contains(Mod::CONTROL_MASK) || state.contains(Mod::MOD1_MASK))
            && let Some(index) = number_key(event)
        {
            match index {
                0 => {
                    self.submit_input_prompt();
                    return glib::Propagation::Stop;
                }
                1 => {
                    self.cancel_input_prompt();
                    return glib::Propagation::Stop;
                }
                _ => {}
            }
        }
        if kb.home.matches(event) {
            self.cancel_input_prompt();
            if self.standalone {
                gtk::main_quit();
            } else {
                self.enter(Mode::Home);
            }
            return glib::Propagation::Stop;
        }
        if kb.back.matches(event) {
            self.cancel_input_prompt();
            return glib::Propagation::Stop;
        }
        if kb.activate.matches(event) {
            let selected_btn = self
                .input_prompt
                .borrow()
                .as_ref()
                .and_then(|pr| pr.selected_button);
            if selected_btn == Some(1) {
                self.cancel_input_prompt();
            } else {
                self.submit_input_prompt();
            }
            return glib::Propagation::Stop;
        }
        if kb.clear_query.matches(event) {
            self.set_input("");
            return glib::Propagation::Stop;
        }
        let pressed = event.keyval();
        if matches!(pressed, key::Tab | key::ISO_Left_Tab) || kb.next.matches(event) || kb.previous.matches(event) {
            let reverse = pressed == key::ISO_Left_Tab
                || state.contains(Mod::SHIFT_MASK)
                || (kb.previous.matches(event) && !kb.next.matches(event));
            let next_btn = {
                let mut prompt = self.input_prompt.borrow_mut();
                let Some(pr) = prompt.as_mut() else {
                    return glib::Propagation::Stop;
                };
                pr.selected_button = match (pr.selected_button, reverse) {
                    (None, false) => Some(0),
                    (Some(0), false) => Some(1),
                    (Some(1), false) => None,
                    (None, true) => Some(1),
                    (Some(1), true) => Some(0),
                    (Some(0), true) => None,
                    _ => None,
                };
                pr.selected_button
            };
            self.sync_input_snapshot();
            let moving = self.motion.borrow_mut().select_input_button(next_btn);
            let animate = self.animate.get();
            if !animate {
                self.motion.borrow_mut().advance(10.0);
            }
            self.redraw(moving && animate);
            return glib::Propagation::Stop;
        }
        if !event.is_modifier() {
            let cleared = {
                let mut prompt = self.input_prompt.borrow_mut();
                if let Some(pr) = prompt.as_mut()
                    && pr.selected_button.is_some()
                {
                    pr.selected_button = None;
                    true
                } else {
                    false
                }
            };
            if cleared {
                let _ = self.motion.borrow_mut().select_input_button(None);
            }
        }
        glib::Propagation::Proceed
    }

    fn open_input_prompt(
        self: &Rc<Self>,
        cfg: config::InputPromptConfig,
        callback: InputCallback,
    ) {
        if let Some(prev) = self.input_prompt.borrow_mut().take()
            && let InputCallback::BluetoothPin { reply } = prev.callback
        {
            let _ = reply.send(None);
        }
        let saved_query = {
            let mut menu = self.menu.borrow_mut();
            menu.selected = None;
            menu.pressed = None;
            menu.query.clone()
        };
        let fallback_title = self.menu.borrow().title().to_owned();
        let title = cfg
            .title
            .filter(|t| !t.trim().is_empty())
            .unwrap_or(fallback_title);
        let default_text = cfg.default.unwrap_or_default();
        self.hover_position.set(None);
        self.thumb_grab.set(None);
        self.color_drag.set(None);
        self.input_prompt.replace(Some(ActiveInputPrompt {
            title,
            text: default_text.clone(),
            password: cfg.password,
            allow_empty: cfg.allow_empty,
            selected_button: None,
            pressed_button: None,
            saved_query,
            color: None,
            callback,
        }));
        self.set_input(&default_text);
        self.refresh();
    }

    fn open_color_picker(self: &Rc<Self>, field: ColorField) {
        let current_hex = field.get(&self.config.borrow().colors).to_owned();
        let (r, g, b) = config::parse_hex_rgb_u8(&current_hex).unwrap_or((128, 128, 128));
        let normalized = config::format_hex_rgb(r, g, b);
        self.open_input_prompt(
            config::InputPromptConfig {
                title: Some(field.label().to_owned()),
                default: Some(normalized),
                ..Default::default()
            },
            InputCallback::SettingsColorHex(field),
        );
        if let Some(pr) = self.input_prompt.borrow_mut().as_mut() {
            pr.color = Some(ring::explorer::ColorPickerState::from_rgb(r, g, b));
        }
        self.sync_input_snapshot();
        self.refresh();
    }

    fn set_color_sv(&self, s: f64, v: f64) {
        let hex = {
            let mut guard = self.input_prompt.borrow_mut();
            let Some(pr) = guard.as_mut() else {
                return;
            };
            let Some(cp) = pr.color.as_mut() else {
                return;
            };
            cp.s = s.clamp(0.0, 1.0);
            cp.v = v.clamp(0.0, 1.0);
            let (r, g, b) = cp.to_rgb();
            let hex = config::format_hex_rgb(r, g, b);
            pr.text = hex.clone();
            hex
        };
        self.set_input(&hex);
        self.sync_input_snapshot();
        self.queue_dirty();
    }

    fn set_color_hue(&self, h: f64) {
        let hex = {
            let mut guard = self.input_prompt.borrow_mut();
            let Some(pr) = guard.as_mut() else {
                return;
            };
            let Some(cp) = pr.color.as_mut() else {
                return;
            };
            cp.h = h.rem_euclid(360.0);
            let (r, g, b) = cp.to_rgb();
            let hex = config::format_hex_rgb(r, g, b);
            pr.text = hex.clone();
            hex
        };
        self.set_input(&hex);
        self.sync_input_snapshot();
        self.queue_dirty();
    }

    fn cancel_input_prompt(self: &Rc<Self>) {
        self.color_drag.set(None);
        self.sync_input_snapshot();
        let Some(active) = self.input_prompt.borrow_mut().take() else {
            return;
        };
        self.menu.borrow_mut().query = active.saved_query.clone();
        self.set_input(&active.saved_query);
        match active.callback {
            InputCallback::BluetoothPin { reply } => {
                let _ = reply.send(None);
                self.refresh();
            }
            InputCallback::CustomAction {
                pop_on_exit: true, ..
            } => {
                self.step_back(Mode::Custom, "");
                self.refresh();
            }
            _ => {
                self.refresh();
            }
        }
    }

    fn submit_input_prompt(self: &Rc<Self>) {
        let check = {
            let mut guard = self.input_prompt.borrow_mut();
            let Some(pr) = guard.as_mut() else {
                return;
            };
            let value = if pr.password {
                pr.text.clone()
            } else {
                pr.text.trim().to_owned()
            };
            if !pr.allow_empty && value.is_empty() {
                Err(())
            } else if matches!(pr.callback, InputCallback::WifiHiddenSsid) {
                let ssid = value.clone();
                pr.title = format!("{ssid} Password");
                pr.text.clear();
                pr.password = true;
                pr.allow_empty = true;
                pr.selected_button = None;
                pr.pressed_button = None;
                pr.callback = InputCallback::WifiHiddenPassword { ssid };
                Ok(None)
            } else {
                Ok(Some(value))
            }
        };
        let value = match check {
            Err(()) => {
                self.sync_input_snapshot();
                self.queue_dirty();
                return;
            }
            Ok(None) => {
                self.set_input("");
                self.refresh();
                return;
            }
            Ok(Some(val)) => val,
        };

        self.color_drag.set(None);
        self.sync_input_snapshot();
        let Some(active) = self.input_prompt.borrow_mut().take() else {
            return;
        };
        self.menu.borrow_mut().query = active.saved_query.clone();
        self.set_input(&active.saved_query);

        match active.callback {
            InputCallback::BluetoothAlias => {
                self.refresh();
                let notice = format!("Adapter renamed to {value}");
                self.run_bt_task(
                    "Updating adapter name…".into(),
                    notice,
                    false,
                    false,
                    false,
                    move || bluetooth::set_alias(&value),
                );
            }
            InputCallback::BluetoothPin { reply } => {
                let _ = reply.send(Some(value));
                self.refresh();
            }
            InputCallback::WifiPassword { ssid, saved_uuid } => {
                self.refresh();
                let notice = format!("Connected to {ssid}");
                self.run_wifi_task(
                    format!("Connecting to {ssid}…"),
                    notice,
                    WifiTaskOptions::default(),
                    move || wifi::connect_with_password(&ssid, saved_uuid.as_deref(), &value),
                );
            }
            InputCallback::WifiHiddenPassword { ssid } => {
                self.refresh();
                let notice = format!("Connected to {ssid}");
                let pw = (!value.is_empty()).then_some(value);
                self.run_wifi_task(
                    format!("Connecting to {ssid}…"),
                    notice,
                    WifiTaskOptions {
                        pop_to_root_on_success: true,
                        ..Default::default()
                    },
                    move || wifi::connect_hidden(&ssid, pw.as_deref()),
                );
            }
            InputCallback::CustomAction {
                command,
                submenu,
                reload,
                loading_animation,
                pop_on_exit,
            } => {
                if pop_on_exit && submenu.is_none() {
                    let mut guard = self.custom_menu.borrow_mut();
                    if let Some(cm) = guard.as_mut() {
                        cm.pop_submenu();
                    }
                }
                self.refresh();
                self.execute_custom_action(
                    command,
                    submenu,
                    reload,
                    loading_animation,
                    Some(value),
                );
            }
            InputCallback::SettingsColorHex(field) => {
                if let Err(err) = config::parse_hex_color(&value, field.label()) {
                    self.show_notice(err);
                    return;
                }
                let normalized = if value.starts_with('#') {
                    value.to_ascii_lowercase()
                } else {
                    format!("#{}", value.to_ascii_lowercase())
                };
                self.mutate_settings_config(
                    |cfg| {
                        field.set(&mut cfg.colors, normalized.clone());
                        cfg.colors.theme = None;
                    },
                    format!("{} set to {normalized}", field.label()),
                );
            }
            InputCallback::SettingsKeybind(field) => {
                self.add_keybind_spec(field, value);
            }
            InputCallback::SettingsMainMenuLabel(index) => {
                let label = (!value.is_empty()).then_some(value);
                let msg = match &label {
                    Some(text) => format!("Label set to {text}"),
                    None => "Label reset to default".into(),
                };
                self.mutate_settings_config(
                    |cfg| {
                        if let Some(entry) = cfg.main_menu.get_mut(index) {
                            entry.label = label.clone();
                        }
                    },
                    msg,
                );
            }
            InputCallback::SettingsMainMenuIcon(index) => {
                let icon = (!value.is_empty()).then_some(value);
                let msg = match &icon {
                    Some(text) => format!("Icon set to {text}"),
                    None => "Icon reset to default".into(),
                };
                self.mutate_settings_config(
                    |cfg| {
                        if let Some(entry) = cfg.main_menu.get_mut(index) {
                            entry.icon = icon.clone();
                        }
                    },
                    msg,
                );
            }
            InputCallback::WifiHiddenSsid => unreachable!(),
        }
    }

    fn execute_custom_action(
        self: &Rc<Self>,
        command: Option<String>,
        submenu: Option<String>,
        reload: bool,
        loading_animation: Option<bool>,
        input_value: Option<String>,
    ) {
        let pre_cmd = command.map(|cmd| config::expand_command_input(&cmd, input_value.as_deref()));
        if let Some(sub) = submenu {
            self.open_custom_submenu(sub, pre_cmd, loading_animation, input_value);
            return;
        }
        if reload {
            self.reload_custom_menu(pre_cmd, loading_animation, input_value);
            return;
        }
        if let Some(cmd) = pre_cmd {
            self.window.hide();
            if let Some(display) = gdk::Display::default() {
                display.sync();
            }
            match config::run_command_with_input(&cmd, input_value.as_deref()) {
                Ok(()) => gtk::main_quit(),
                Err(message) => {
                    self.error(message);
                    self.window.show();
                    self.input.grab_focus();
                }
            }
        }
    }

    fn step_back(self: &Rc<Self>, mode: Mode, query: &str) {
        if mode != Mode::Files
            && !self.file_search_pending(mode)
            && let Some(text) = self.menu.borrow_mut().undo_completion()
        {
            if self.back_stack.borrow().last().map(String::as_str) == Some(text.as_str()) {
                self.back_stack.borrow_mut().pop();
            }
            self.set_input(&text);
            self.typing_in_search.set(!text.is_empty());
            return;
        }
        let prev = self.back_stack.borrow_mut().pop();
        if let Some(prev) = prev {
            if prev != query {
                self.set_input(&prev);
            }
            return;
        }
        if mode == Mode::Wifi {
            let popped = {
                let mut stack = self.wifi_stack.borrow_mut();
                if stack.len() > 1 {
                    stack.pop();
                    true
                } else {
                    false
                }
            };
            if popped {
                self.op_gen.set(self.op_gen.get().wrapping_add(1));
                self.network_scanning.set(false);
                self.busy_message.replace(None);
                self.hover_position.set(None);
                self.scroll.borrow_mut().reset();
                self.back_stack.borrow_mut().clear();
                self.menu.borrow_mut().query.clear();
                self.set_input("");
                self.filter();
                return;
            }
        }
        if mode == Mode::Bluetooth {
            let popped = {
                let mut stack = self.bt_stack.borrow_mut();
                if stack.len() > 1 {
                    stack.pop();
                    true
                } else {
                    false
                }
            };
            if popped {
                self.op_gen.set(self.op_gen.get().wrapping_add(1));
                self.network_scanning.set(false);
                self.busy_message.replace(None);
                self.hover_position.set(None);
                self.scroll.borrow_mut().reset();
                self.back_stack.borrow_mut().clear();
                self.menu.borrow_mut().query.clear();
                self.set_input("");
                self.filter();
                return;
            }
        }
        if mode == Mode::Settings {
            self.key_capture.set(None);
            if self.pop_settings_view() {
                return;
            }
        }
        if mode == Mode::Custom {
            let popped_title = {
                let mut active = self.custom_menu.borrow_mut();
                active.as_mut().and_then(|menu| {
                    menu.pop_submenu()
                        .then(|| menu.current_title().to_owned())
                })
            };
            if let Some(title) = popped_title {
                self.custom_request_id
                    .set(self.custom_request_id.get().wrapping_add(1));
                self.custom_loading.set(false);
                self.custom_loading_spin.set(false);
                self.provider_error.take();
                self.hover_position.set(None);
                self.scroll.borrow_mut().reset();
                self.back_stack.borrow_mut().clear();
                {
                    let mut menu = self.menu.borrow_mut();
                    menu.title = title;
                    menu.query.clear();
                }
                self.set_input("");
                self.filter();
                return;
            }
        }
        if self.standalone {
            gtk::main_quit();
        } else {
            self.enter(Mode::Home);
        }
    }

    fn enter_custom(self: &Rc<Self>, name: &str) {
        match self.store.load_menu(name) {
            Ok(config) => self.enter_custom_loaded(name.to_owned(), config),
            Err(err) => self.error(err),
        }
    }

    fn apply_custom_menu(self: &Rc<Self>, name: String, config: config::MenuConfig) {
        let active = ActiveCustomMenu::new(name, config);
        let title = active.current_title().to_owned();
        let layout = active.current_layout();
        self.custom_request_id
            .set(self.custom_request_id.get().wrapping_add(1));
        self.custom_loading.set(false);
        self.custom_loading_spin.set(false);
        self.network_scanning.set(false);
        self.hover_position.set(None);
        self.scroll.borrow_mut().reset();
        self.provider_error.take();
        self.custom_menu.replace(Some(active));
        *self.menu.borrow_mut() =
            Menu::home_with(Vec::new(), self.config.borrow().appearance.overflow_layout);
        self.back_stack.borrow_mut().clear();
        self.set_input("");
        {
            let mut menu = self.menu.borrow_mut();
            menu.mode = Mode::Custom;
            menu.layout = layout;
            menu.title = title;
        }
    }

    fn enter_custom_loaded(self: &Rc<Self>, name: String, config: config::MenuConfig) {
        let on_open = config.on_open.clone();
        let spin = config.loading_animation;
        if !spin
            && !self.standalone
            && let Some(cmd) = on_open
        {
            // Keep the previous screen visible until `on_open` finishes.
            self.run_custom_refresh(name, vec![cmd], None, false, CustomRefreshTarget::Enter);
            return;
        }
        self.apply_custom_menu(name.clone(), config);
        if let Some(cmd) = on_open {
            self.custom_loading.set(true);
            self.custom_loading_spin.set(spin);
            if !spin {
                self.filter();
            }
            self.run_custom_refresh(name, vec![cmd], None, spin, CustomRefreshTarget::Current);
        } else {
            self.filter();
        }
    }

    fn open_custom_submenu(
        self: &Rc<Self>,
        key: String,
        pre_command: Option<String>,
        action_spin: Option<bool>,
        input_value: Option<String>,
    ) {
        let peek = {
            let guard = self.custom_menu.borrow();
            let Some(active) = guard.as_ref() else {
                return;
            };
            active.submenu_refresh_info(&key).map(|(on_open, sub_spin)| {
                (
                    active.name.clone(),
                    on_open,
                    action_spin.unwrap_or(sub_spin),
                )
            })
        };
        let (name, on_open, spin) = match peek {
            Ok(data) => data,
            Err(err) => {
                self.error(err);
                return;
            }
        };
        let mut commands = Vec::new();
        if let Some(cmd) = pre_command {
            commands.push(cmd);
        }
        if let Some(cmd) = on_open {
            commands.push(cmd);
        }
        if !commands.is_empty() && !spin {
            // Keep the parent screen visible until the refresh commands finish.
            self.run_custom_refresh(
                name,
                commands,
                input_value,
                false,
                CustomRefreshTarget::Submenu(key),
            );
            return;
        }
        let next_title = {
            let mut guard = self.custom_menu.borrow_mut();
            let Some(active) = guard.as_mut() else {
                return;
            };
            match active.push_submenu(key) {
                Ok(()) => active.current_title().to_owned(),
                Err(err) => {
                    drop(guard);
                    self.error(err);
                    return;
                }
            }
        };
        self.custom_request_id
            .set(self.custom_request_id.get().wrapping_add(1));
        self.custom_loading.set(false);
        self.custom_loading_spin.set(false);
        self.provider_error.take();
        self.hover_position.set(None);
        self.scroll.borrow_mut().reset();
        self.back_stack.borrow_mut().clear();
        {
            let mut menu = self.menu.borrow_mut();
            menu.title = next_title;
            menu.query.clear();
        }
        self.set_input("");
        if !commands.is_empty() {
            self.run_custom_refresh(
                name,
                commands,
                input_value,
                spin,
                CustomRefreshTarget::Current,
            );
        } else {
            self.filter();
        }
    }

    fn open_inline_custom_list(self: &Rc<Self>, sub: config::SubmenuConfig) {
        let next_title = {
            let mut guard = self.custom_menu.borrow_mut();
            let Some(active) = guard.as_mut() else {
                return;
            };
            active.push_inline_list(sub);
            active.current_title().to_owned()
        };
        self.custom_request_id
            .set(self.custom_request_id.get().wrapping_add(1));
        self.custom_loading.set(false);
        self.custom_loading_spin.set(false);
        self.provider_error.take();
        self.hover_position.set(None);
        self.scroll.borrow_mut().reset();
        self.back_stack.borrow_mut().clear();
        {
            let mut menu = self.menu.borrow_mut();
            menu.title = next_title;
            menu.query.clear();
        }
        self.set_input("");
        self.filter();
    }

    fn reload_custom_menu(
        self: &Rc<Self>,
        pre_command: Option<String>,
        action_spin: Option<bool>,
        input_value: Option<String>,
    ) {
        let current = {
            let guard = self.custom_menu.borrow();
            let Some(active) = guard.as_ref() else {
                return;
            };
            (
                active.name.clone(),
                active.current_on_open().map(str::to_owned),
                action_spin.unwrap_or_else(|| active.current_loading_animation()),
            )
        };
        let (name, on_open, spin) = current;
        if spin {
            self.hover_position.set(None);
            self.scroll.borrow_mut().reset();
            self.back_stack.borrow_mut().clear();
            self.menu.borrow_mut().query.clear();
            self.set_input("");
        }
        let mut commands = Vec::new();
        if let Some(cmd) = pre_command {
            commands.push(cmd);
        }
        if let Some(cmd) = on_open {
            commands.push(cmd);
        }
        self.run_custom_refresh(
            name,
            commands,
            input_value,
            spin,
            CustomRefreshTarget::Current,
        );
    }

    fn run_custom_refresh(
        self: &Rc<Self>,
        name: String,
        commands: Vec<String>,
        input_value: Option<String>,
        spin: bool,
        target: CustomRefreshTarget,
    ) {
        let req_id = self.custom_request_id.get().wrapping_add(1);
        self.custom_request_id.set(req_id);
        self.custom_loading.set(true);
        self.custom_loading_spin.set(spin);
        self.provider_error.take();
        if spin {
            self.filter();
        }
        let store = self.store.clone();
        let p = self.clone();
        let load_name = name.clone();
        glib::MainContext::default().spawn_local(async move {
            let result = gio::spawn_blocking(move || {
                for cmd in commands {
                    config::run_command_wait_with_input(&cmd, input_value.as_deref())?;
                }
                store.load_menu(&load_name)
            })
            .await
            .expect("custom menu worker");
            if p.custom_request_id.get() != req_id
                || (!matches!(target, CustomRefreshTarget::Enter)
                    && p.menu.borrow().mode != Mode::Custom)
            {
                return;
            }
            p.custom_loading.set(false);
            p.custom_loading_spin.set(false);
            match result {
                Ok(reloaded) => match target {
                    CustomRefreshTarget::Enter => {
                        p.apply_custom_menu(name, reloaded);
                        p.filter();
                    }
                    CustomRefreshTarget::Submenu(key) => {
                        let next_title = {
                            let mut guard = p.custom_menu.borrow_mut();
                            let Some(active) = guard.as_mut() else {
                                return;
                            };
                            active.config = reloaded;
                            match active.push_submenu(key) {
                                Ok(()) => active.current_title().to_owned(),
                                Err(err) => {
                                    drop(guard);
                                    p.error(err);
                                    return;
                                }
                            }
                        };
                        p.hover_position.set(None);
                        p.scroll.borrow_mut().reset();
                        p.back_stack.borrow_mut().clear();
                        {
                            let mut menu = p.menu.borrow_mut();
                            menu.title = next_title;
                            menu.query.clear();
                        }
                        p.set_input("");
                        p.filter();
                    }
                    CustomRefreshTarget::Current => {
                        let title = {
                            let mut guard = p.custom_menu.borrow_mut();
                            let Some(active) = guard.as_mut() else {
                                return;
                            };
                            active.config = reloaded;
                            active.current_title().to_owned()
                        };
                        p.hover_position.set(None);
                        p.scroll.borrow_mut().reset();
                        p.menu.borrow_mut().title = title;
                        p.filter();
                    }
                },
                Err(err) => p.provider_failed(err),
            }
        });
    }

    fn enter(self: &Rc<Self>, mode: Mode) {
        if let Some(prev) = self.input_prompt.borrow_mut().take()
            && let InputCallback::BluetoothPin { reply } = prev.callback
        {
            let _ = reply.send(None);
        }
        self.custom_request_id
            .set(self.custom_request_id.get().wrapping_add(1));
        self.custom_loading.set(false);
        self.custom_loading_spin.set(false);
        self.op_gen.set(self.op_gen.get().wrapping_add(1));
        self.network_scanning.set(false);
        self.busy_message.replace(None);
        self.wifi_stack.borrow_mut().clear();
        self.bt_stack.borrow_mut().clear();
        self.settings_stack.borrow_mut().clear();
        self.key_capture.set(None);
        self.hover_position.set(None);
        self.scroll.borrow_mut().reset();
        self.provider_error.take();
        self.custom_menu.replace(None);
        {
            let cfg = self.config.borrow();
            *self.menu.borrow_mut() =
                Menu::home_with(self.store.home_targets(&cfg), cfg.appearance.overflow_layout);
        }
        self.back_stack.borrow_mut().clear();
        self.set_input("");
        self.menu.borrow_mut().mode = mode;
        if mode != Mode::Home {
            self.menu.borrow_mut().results(Vec::new());
            match mode {
                Mode::Wifi => {
                    self.wifi_stack.borrow_mut().push(WifiView::Root);
                    self.wifi_state.replace(None);
                }
                Mode::Bluetooth => {
                    self.bt_stack.borrow_mut().push(BluetoothView::Root);
                    self.bt_state.replace(None);
                }
                Mode::Settings => {
                    self.settings_stack.borrow_mut().push(SettingsView::Root);
                }
                _ => (),
            }
            self.filter();
            match mode {
                Mode::Apps if self.apps.borrow().is_none() => {
                    let p = self.clone();
                    glib::MainContext::default().spawn_local(async move {
                        match gio::spawn_blocking(apps::list)
                            .await
                            .expect("application worker")
                        {
                            Ok(apps) => {
                                p.apps.replace(Some(apps));
                                if p.menu.borrow().mode == Mode::Apps {
                                    p.filter();
                                }
                            }
                            Err(err) if p.menu.borrow().mode == Mode::Apps => {
                                p.provider_failed(err)
                            }
                            _ => (),
                        }
                    });
                }
                Mode::Windows => {
                    let p = self.clone();
                    glib::MainContext::default().spawn_local(async move {
                        let result = gio::spawn_blocking(windows::list)
                            .await
                            .expect("window worker");
                        if p.menu.borrow().mode != Mode::Windows {
                            return;
                        }
                        match result {
                            Ok(windows) => {
                                p.windows.replace(Some(windows));
                                p.filter();
                            }
                            Err(err) => p.provider_failed(err),
                        }
                    });
                }
                Mode::Wifi => {
                    let op_id = self.op_gen.get();
                    let p = self.clone();
                    glib::MainContext::default().spawn_local(async move {
                        let result = gio::spawn_blocking(|| wifi::load_state(false))
                            .await
                            .expect("wifi worker");
                        if p.op_gen.get() != op_id || p.menu.borrow().mode != Mode::Wifi {
                            return;
                        }
                        match result {
                            Ok(state) => {
                                p.wifi_state.replace(Some(state));
                                p.filter();
                            }
                            Err(err) => p.provider_failed(err),
                        }
                    });
                }
                Mode::Bluetooth => {
                    let op_id = self.op_gen.get();
                    let p = self.clone();
                    glib::MainContext::default().spawn_local(async move {
                        let result = gio::spawn_blocking(bluetooth::load_state)
                            .await
                            .expect("bluetooth worker");
                        if p.op_gen.get() != op_id || p.menu.borrow().mode != Mode::Bluetooth {
                            return;
                        }
                        match result {
                            Ok(state) => {
                                p.bt_state.replace(Some(state));
                                p.filter();
                            }
                            Err(err) => p.provider_failed(err),
                        }
                    });
                }
                _ => (),
            }
        } else {
            self.windows.replace(None);
            self.wifi_state.replace(None);
            self.bt_state.replace(None);
        }
        self.refresh();
    }

    fn filter(self: &Rc<Self>) {
        let mode = self.menu.borrow().mode;
        let query = self.menu.borrow().query.clone();
        let mut custom_title: Option<String> = None;
        let mut idle_message = String::new();
        let mut settings_layout = config::MenuLayout::Ring;
        let entries = match mode {
            Mode::Home => return,
            Mode::Apps => self
                .apps
                .borrow()
                .as_ref()
                .map(|a| menu::app_matches(a, &query, &self.history.borrow())),
            Mode::Windows => self
                .windows
                .borrow()
                .as_ref()
                .map(|w| menu::window_matches(w, &query)),
            Mode::Wifi => {
                let view = self
                    .wifi_stack
                    .borrow()
                    .last()
                    .cloned()
                    .unwrap_or(WifiView::Root);
                custom_title = Some(view.title());
                if self.network_scanning.get() {
                    None
                } else {
                    self.wifi_state.borrow().as_ref().map(|state| {
                        idle_message = wifi::idle_status(state, &view);
                        let items = wifi::build_items(state, &view);
                        menu::wifi_matches(&items, &query)
                    })
                }
            }
            Mode::Bluetooth => {
                let view = self
                    .bt_stack
                    .borrow()
                    .last()
                    .cloned()
                    .unwrap_or(BluetoothView::Root);
                custom_title = Some(view.title());
                if self.network_scanning.get() {
                    None
                } else {
                    self.bt_state.borrow().as_ref().map(|state| {
                        idle_message = bluetooth::idle_status(state, &view);
                        let items = bluetooth::build_items(state, &view);
                        menu::bluetooth_matches(&items, &query)
                    })
                }
            }
            Mode::Settings => {
                let view = self
                    .settings_stack
                    .borrow()
                    .last()
                    .cloned()
                    .unwrap_or(SettingsView::Root);
                let cfg = self.config.borrow();
                custom_title = Some(view.title(&cfg));
                idle_message = settings::idle_status(&cfg, &view);
                settings_layout = view.layout();
                let items = settings::build_items(&cfg, &self.store, &view);
                Some(menu::settings_matches(&items, &query))
            }
            Mode::Custom if self.custom_loading.get() && self.custom_loading_spin.get() => None,
            Mode::Custom => {
                let custom_state = self.custom_menu.borrow().as_ref().map(|active| {
                    (
                        active.current_title().to_owned(),
                        active.current_layout(),
                        active.current_prompt().map(str::to_owned),
                        active.current_items(),
                        active.current_input_action(),
                    )
                });
                if let Some((title, layout, prompt, items, input_action)) = custom_state {
                    custom_title = Some(title);
                    if let Some(prompt_msg) = prompt {
                        idle_message = prompt_msg;
                    }
                    self.menu.borrow_mut().layout = layout;
                    if let Some((prompt_cfg, command, submenu, reload, loading_animation)) =
                        input_action
                    {
                        self.open_input_prompt(
                            prompt_cfg,
                            InputCallback::CustomAction {
                                command,
                                submenu,
                                reload,
                                loading_animation,
                                pop_on_exit: true,
                            },
                        );
                        return;
                    }
                    Some(menu::custom_matches(&items, &query))
                } else {
                    None
                }
            }
            Mode::Files => None,
        };
        if mode == Mode::Settings {
            self.menu.borrow_mut().layout = settings_layout;
        } else if mode != Mode::Custom {
            self.menu.borrow_mut().layout = config::MenuLayout::Ring;
        }
        let mut menu = self.menu.borrow_mut();
        if let Some(title) = custom_title {
            menu.title = title;
        }
        menu.idle_message = idle_message;
        if mode == Mode::Files {
            // Keep drawing the previous results until the directory worker has the next set.
            menu.selected = None;
            menu.pressed = None;
            menu.message.clear();
            menu.end_completion();
        } else if let Some(entries) = entries {
            menu.results(entries);
        } else {
            menu.results(Vec::new());
            menu.message = self.provider_error.borrow().clone().unwrap_or_default();
        }
        drop(menu);
        let start_file_worker = mode == Mode::Files && !self.file_worker.replace(true);
        self.refresh();
        if start_file_worker {
            let p = self.clone();
            glib::MainContext::default().spawn_local(async move {
                // Coalesce keystrokes while a slow directory is being read.
                while p.menu.borrow().mode == Mode::Files {
                    let current = p.menu.borrow().query.clone();
                    let query = current.clone();
                    let history = p.history.borrow().clone();
                    let result = gio::spawn_blocking(move || files::entries(&query, &history))
                        .await
                        .expect("directory worker");
                    if p.menu.borrow().mode != Mode::Files {
                        break;
                    }
                    if p.menu.borrow().query != current {
                        continue;
                    }
                    match result {
                        Ok(entries) => p
                            .menu
                            .borrow_mut()
                            .results(entries.into_iter().map(Target::File).collect()),
                        Err(err) => {
                            p.menu.borrow_mut().results(Vec::new());
                            p.error(err.to_string());
                        }
                    }
                    break;
                }
                p.file_worker.set(false);
                p.refresh();
            });
        }
    }

    fn provider_failed(self: &Rc<Self>, message: String) {
        self.provider_error.replace(Some(message.clone()));
        self.error(message);
        self.refresh();
    }

    fn error(&self, message: String) {
        eprintln!("ReCircles: {message}");
        self.menu.borrow_mut().message = message;
        self.queue_dirty();
    }

    fn launch_and_quit(&self, launch: impl FnOnce() -> Result<(), String>) {
        self.window.hide();
        if let Some(display) = gdk::Display::default() {
            display.sync();
        }
        match launch() {
            Ok(()) => gtk::main_quit(),
            Err(message) => {
                self.error(message);
                self.window.show();
                self.input.grab_focus();
            }
        }
    }

    fn open_current_folder(&self, query: &str) {
        match files::current_folder(query) {
            Ok(Some(entry)) => self.launch_and_quit(|| {
                files::open(&entry, false, &mut self.history.borrow_mut())
                    .map_err(|e| e.to_string())
            }),
            Ok(None) => {}
            Err(err) => self.error(err.to_string()),
        }
    }

    fn activate(self: &Rc<Self>, index: usize, secondary: bool) {
        if self.file_search_pending(self.menu.borrow().mode) || self.custom_loading.get() {
            return;
        }
        let Some(target) = self.menu.borrow().visible().get(index).cloned() else {
            return;
        };
        if let Target::Mode(mode) = target {
            if !secondary {
                self.enter(mode);
            }
            return;
        }
        if let Target::HomeEntry(entry) = target {
            if !secondary {
                if let Some(mode) = entry.builtin {
                    self.enter(mode);
                } else {
                    self.enter_custom(&entry.name);
                }
            }
            return;
        }
        if let Target::CustomItem(item) = target {
            let Some(action) = item.action(secondary) else {
                return;
            };
            if let Some(prompt_cfg) = action.input_prompt(&item.label) {
                self.open_input_prompt(
                    prompt_cfg,
                    InputCallback::CustomAction {
                        command: action.as_command().map(str::to_owned),
                        submenu: action.as_submenu().map(str::to_owned),
                        reload: action.reloads(),
                        loading_animation: action.loading_animation(),
                        pop_on_exit: false,
                    },
                );
                return;
            }
            if let Some(inline_sub) = action.inline_list_submenu(&item.label) {
                self.open_inline_custom_list(inline_sub);
                return;
            }
            self.execute_custom_action(
                action.as_command().map(str::to_owned),
                action.as_submenu().map(str::to_owned),
                action.reloads(),
                action.loading_animation(),
                Some(item.label.clone()),
            );
            return;
        }
        if let Target::Wifi(item) = target {
            if self.busy_message.borrow().is_some() {
                return;
            }
            let action = if secondary {
                item.secondary_action
            } else {
                Some(item.action)
            };
            if let Some(action) = action {
                self.handle_wifi_action(action);
            }
            return;
        }
        if let Target::Bluetooth(item) = target {
            if self.busy_message.borrow().is_some() {
                return;
            }
            let action = if secondary {
                item.secondary_action
            } else {
                Some(item.action)
            };
            if let Some(action) = action {
                self.handle_bluetooth_action(action);
            }
            return;
        }
        if let Target::Settings(item) = target {
            let action = if secondary {
                item.secondary_action
            } else {
                Some(item.action)
            };
            if let Some(action) = action {
                self.handle_settings_action(action);
            }
            return;
        }
        if secondary && !matches!(target, Target::File(_)) {
            return;
        }
        if let Target::File(ref file) = target
            && !secondary
            && file.is_dir
        {
            if let Some(path) = &file.completion {
                if self.input.text().as_str() == path {
                    self.typing_in_search.set(false);
                    self.filter();
                } else {
                    let current = self.input.text();
                    self.push_back(current.as_str());
                    self.set_input(path);
                }
            } else {
                self.error("This folder's path cannot be entered as text".into());
            }
            return;
        }
        self.launch_and_quit(|| match target {
            Target::File(f) => {
                files::open(&f, secondary && !f.is_dir, &mut self.history.borrow_mut())
                    .map_err(|e| e.to_string())
            }
            Target::App(a) => a.launch().map(|()| {
                if let Err(e) = self.history.borrow_mut().record_app(&a.id) {
                    eprintln!("ReCircles: could not save app usage: {e}");
                }
            }),
            Target::Window(w) => w.focus(),
            Target::Mode(_)
            | Target::HomeEntry(_)
            | Target::CustomItem(_)
            | Target::Wifi(_)
            | Target::Bluetooth(_)
            | Target::Settings(_) => unreachable!(),
        });
    }

    fn run_modal_dialog<T>(&self, f: impl FnOnce() -> T) -> T {
        self.window.hide();
        if let Some(display) = gdk::Display::default() {
            display.sync();
        }
        let result = f();
        self.window.show();
        self.input.grab_focus();
        self.queue_dirty();
        result
    }

    fn set_busy(&self, message: Option<String>) {
        self.busy_message.replace(message);
        self.queue_dirty();
    }

    fn show_notice(&self, message: String) {
        self.menu.borrow_mut().message = message;
        self.queue_dirty();
    }

    fn push_settings_view(self: &Rc<Self>, view: SettingsView) {
        self.settings_stack.borrow_mut().push(view);
        self.key_capture.set(None);
        self.hover_position.set(None);
        self.scroll.borrow_mut().reset();
        self.back_stack.borrow_mut().clear();
        self.menu.borrow_mut().query.clear();
        self.set_input("");
        self.filter();
    }

    fn pop_settings_view(self: &Rc<Self>) -> bool {
        let popped = {
            let mut stack = self.settings_stack.borrow_mut();
            if stack.len() > 1 {
                stack.pop();
                true
            } else {
                false
            }
        };
        if popped {
            self.key_capture.set(None);
            self.hover_position.set(None);
            self.scroll.borrow_mut().reset();
            self.back_stack.borrow_mut().clear();
            self.menu.borrow_mut().query.clear();
            self.set_input("");
            self.filter();
        }
        popped
    }

    fn mutate_settings_config(
        self: &Rc<Self>,
        mutate: impl FnOnce(&mut AppConfig),
        notice: String,
    ) {
        let mut next = self.config.borrow().clone();
        mutate(&mut next);
        match self.apply_config(next) {
            Ok(()) => {
                self.filter();
                self.show_notice(notice);
            }
            Err(err) => {
                self.show_notice(err);
            }
        }
    }

    fn add_keybind_spec(self: &Rc<Self>, field: KeybindField, spec: String) {
        let trimmed = spec.trim();
        if trimmed.is_empty() {
            self.show_notice("Keybind cannot be empty".into());
            return;
        }
        let mut next = self.config.borrow().clone();
        let list = &mut field.get_mut(&mut next.keybinds).0;
        if list.iter().any(|existing| existing.eq_ignore_ascii_case(trimmed)) {
            self.show_notice(format!("{trimmed} is already bound to {}", field.label()));
            return;
        }
        list.push(trimmed.to_owned());
        match self.apply_config(next) {
            Ok(()) => {
                self.filter();
                self.show_notice(format!("Added {trimmed} to {}", field.label()));
            }
            Err(err) => {
                self.show_notice(err);
            }
        }
    }

    fn handle_key_capture(
        self: &Rc<Self>,
        field: KeybindField,
        event: &gdk::EventKey,
    ) -> glib::Propagation {
        if event.is_modifier() {
            return glib::Propagation::Stop;
        }
        let mods = event.state()
            & (gdk::ModifierType::CONTROL_MASK
                | gdk::ModifierType::SHIFT_MASK
                | gdk::ModifierType::MOD1_MASK
                | gdk::ModifierType::SUPER_MASK
                | gdk::ModifierType::META_MASK);
        if event.keyval() == key::Escape && mods.is_empty() {
            self.key_capture.set(None);
            self.show_notice("Key capture cancelled".into());
            return glib::Propagation::Stop;
        }
        self.key_capture.set(None);
        if let Some(spec) = settings::format_key_event(event) {
            self.add_keybind_spec(field, spec);
        } else {
            self.show_notice("Unsupported key".into());
        }
        glib::Propagation::Stop
    }

    fn browse_icon_dialog(&self) -> Option<String> {
        self.run_modal_dialog(|| {
            let dialog = gtk::FileChooserDialog::with_buttons(
                Some("Choose Menu Icon"),
                None::<&gtk::Window>,
                gtk::FileChooserAction::Open,
                &[
                    ("Cancel", gtk::ResponseType::Cancel),
                    ("Open", gtk::ResponseType::Accept),
                ],
            );
            let filter = gtk::FileFilter::new();
            filter.set_name(Some("Images"));
            filter.add_pixbuf_formats();
            dialog.add_filter(filter);
            let response = dialog.run();
            let chosen = if response == gtk::ResponseType::Accept {
                dialog
                    .filename()
                    .and_then(|p| p.to_str().map(str::to_owned))
            } else {
                None
            };
            dialog.close();
            chosen
        })
    }

    fn handle_settings_action(self: &Rc<Self>, action: SettingsAction) {
        self.key_capture.set(None);
        match action {
            SettingsAction::OpenView(view) => self.push_settings_view(view),
            SettingsAction::GoBack => {
                self.pop_settings_view();
            }
            SettingsAction::ToggleAppearance(toggle) => {
                let notice = match toggle {
                    AppearanceToggle::OverflowLayout => {
                        let next_box = self.config.borrow().appearance.overflow_layout
                            != config::OverflowLayout::Box;
                        if next_box {
                            "Overflow layout: Box"
                        } else {
                            "Overflow layout: Rings"
                        }
                    }
                    AppearanceToggle::Animations => {
                        if !self.config.borrow().appearance.animations {
                            "Animations: On"
                        } else {
                            "Animations: Off"
                        }
                    }
                    AppearanceToggle::Animation3d => {
                        if !self.config.borrow().appearance.animation_3d {
                            "3D Loading: On"
                        } else {
                            "3D Loading: Off"
                        }
                    }
                    AppearanceToggle::Translucent => {
                        if !self.config.borrow().appearance.translucent {
                            "Translucent: On"
                        } else {
                            "Translucent: Off"
                        }
                    }
                    AppearanceToggle::Blur => {
                        if !self.config.borrow().appearance.blur {
                            "Blur: On"
                        } else {
                            "Blur: Off"
                        }
                    }
                    AppearanceToggle::HighContrast => {
                        if !self.config.borrow().appearance.high_contrast {
                            "High Contrast: On"
                        } else {
                            "High Contrast: Off"
                        }
                    }
                };
                self.mutate_settings_config(
                    |cfg| match toggle {
                        AppearanceToggle::OverflowLayout => {
                            cfg.appearance.overflow_layout = match cfg.appearance.overflow_layout {
                                config::OverflowLayout::Box => config::OverflowLayout::Rings,
                                config::OverflowLayout::Rings => config::OverflowLayout::Box,
                            };
                        }
                        AppearanceToggle::Animations => {
                            cfg.appearance.animations = !cfg.appearance.animations;
                        }
                        AppearanceToggle::Animation3d => {
                            cfg.appearance.animation_3d = !cfg.appearance.animation_3d;
                        }
                        AppearanceToggle::Translucent => {
                            cfg.appearance.translucent = !cfg.appearance.translucent;
                        }
                        AppearanceToggle::Blur => {
                            cfg.appearance.blur = !cfg.appearance.blur;
                        }
                        AppearanceToggle::HighContrast => {
                            cfg.appearance.high_contrast = !cfg.appearance.high_contrast;
                        }
                    },
                    notice.into(),
                );
            }
            SettingsAction::ResetAppearance => {
                self.mutate_settings_config(
                    |cfg| cfg.appearance = config::AppearanceConfig::default(),
                    "Appearance reset to defaults".into(),
                );
            }
            SettingsAction::ApplyTheme(id) => {
                let Some(theme) = self.store.find_theme(&id) else {
                    self.show_notice(format!("Theme {id:?} not found"));
                    return;
                };
                let (saved_first, saved_selected) = {
                    let menu = self.menu.borrow();
                    (menu.first_visible, menu.selected)
                };
                let notice = format!("Theme set to {}", theme.name);
                let mut next_colors = theme.colors;
                next_colors.theme = Some(theme.id);
                self.mutate_settings_config(
                    |cfg| cfg.colors = next_colors,
                    notice,
                );
                let restore = {
                    let mut menu = self.menu.borrow_mut();
                    if menu.uses_list() {
                        menu.scroll_list_to(saved_first);
                        menu.selected = saved_selected.filter(|&i| i < menu.visible().len());
                        true
                    } else {
                        false
                    }
                };
                if restore {
                    self.refresh();
                }
            }
            SettingsAction::ImportThemeFile(path) => match self.store.import_theme_file(&path) {
                Ok(ids) => {
                    let mut next = self.config.borrow().clone();
                    let mut display_names = Vec::with_capacity(ids.len());
                    for (idx, id) in ids.iter().enumerate() {
                        if let Some(theme) = self.store.find_theme(id) {
                            display_names.push(theme.name.clone());
                            if idx == 0 {
                                next.colors = theme.colors;
                                next.colors.theme = Some(theme.id);
                            }
                        } else {
                            display_names.push(id.clone());
                        }
                    }
                    match self.apply_config(next) {
                        Ok(()) => {
                            {
                                let mut stack = self.settings_stack.borrow_mut();
                                while matches!(stack.last(), Some(SettingsView::ThemeImport(_))) {
                                    stack.pop();
                                }
                                if stack.is_empty() {
                                    stack.push(SettingsView::Root);
                                }
                            }
                            self.hover_position.set(None);
                            self.scroll.borrow_mut().reset();
                            self.back_stack.borrow_mut().clear();
                            self.menu.borrow_mut().query.clear();
                            self.set_input("");
                            self.filter();
                            self.show_notice(format!(
                                "Installed theme: {}",
                                display_names.join(", ")
                            ));
                        }
                        Err(err) => self.show_notice(err),
                    }
                }
                Err(err) => self.show_notice(err),
            },
            SettingsAction::PickColor(field) => {
                self.open_color_picker(field);
            }
            SettingsAction::ResetColorGroup(group) => {
                self.mutate_settings_config(
                    |cfg| {
                        group.reset(&mut cfg.colors);
                        cfg.colors.theme = if cfg
                            .colors
                            .same_palette(&config::ColorsConfig::default())
                        {
                            Some("default".into())
                        } else {
                            None
                        };
                    },
                    format!("{} reset to defaults", group.title()),
                );
            }
            SettingsAction::ResetAllColors => {
                self.mutate_settings_config(
                    |cfg| cfg.colors = config::ColorsConfig::default(),
                    "All colors reset to defaults".into(),
                );
            }
            SettingsAction::ToggleNumberShortcuts => {
                let next_val = !self.config.borrow().keybinds.number_shortcuts;
                self.mutate_settings_config(
                    |cfg| cfg.keybinds.number_shortcuts = next_val,
                    if next_val {
                        "Number shortcuts: On".into()
                    } else {
                        "Number shortcuts: Off".into()
                    },
                );
            }
            SettingsAction::CaptureKeybind(field) => {
                self.key_capture.set(Some(field));
                self.queue_dirty();
            }
            SettingsAction::TypeKeybind(field) => {
                self.open_input_prompt(
                    config::InputPromptConfig {
                        title: Some(format!("Add {} Keybind", field.label())),
                        placeholder: Some("e.g. Ctrl+j, Shift+Return, Escape".into()),
                        ..Default::default()
                    },
                    InputCallback::SettingsKeybind(field),
                );
            }
            SettingsAction::RemoveKeybind { field, index } => {
                let removed = field
                    .get(&self.config.borrow().keybinds)
                    .0
                    .get(index)
                    .cloned();
                if let Some(spec) = removed {
                    self.mutate_settings_config(
                        |cfg| {
                            let list = &mut field.get_mut(&mut cfg.keybinds).0;
                            if index < list.len() {
                                list.remove(index);
                            }
                        },
                        format!("Removed {spec} from {}", field.label()),
                    );
                }
            }
            SettingsAction::ResetKeybind(field) => {
                self.mutate_settings_config(
                    |cfg| field.reset(&mut cfg.keybinds),
                    format!("{} reset to default", field.label()),
                );
            }
            SettingsAction::ResetAllKeybinds => {
                self.mutate_settings_config(
                    |cfg| cfg.keybinds = config::KeybindsConfig::default(),
                    "Keybinds reset to defaults".into(),
                );
            }
            SettingsAction::MoveMainMenu { index, delta } => {
                let len = self.config.borrow().main_menu.len();
                let Some(target) = index.checked_add_signed(delta).filter(|&t| t < len) else {
                    return;
                };
                if let Some(last) = self.settings_stack.borrow_mut().last_mut()
                    && *last == SettingsView::MainMenuEntry(index)
                {
                    *last = SettingsView::MainMenuEntry(target);
                }
                self.mutate_settings_config(
                    |cfg| {
                        if index < cfg.main_menu.len() && target < cfg.main_menu.len() {
                            cfg.main_menu.swap(index, target);
                        }
                    },
                    format!("Moved to position #{}", target + 1),
                );
            }
            SettingsAction::EditMainMenuLabel(index) => {
                let current = self
                    .config
                    .borrow()
                    .main_menu
                    .get(index)
                    .and_then(|e| e.label.clone())
                    .unwrap_or_default();
                self.open_input_prompt(
                    config::InputPromptConfig {
                        title: Some("Custom Label (empty = default)".into()),
                        default: Some(current),
                        allow_empty: true,
                        ..Default::default()
                    },
                    InputCallback::SettingsMainMenuLabel(index),
                );
            }
            SettingsAction::EditMainMenuIcon(index) => {
                let current = self
                    .config
                    .borrow()
                    .main_menu
                    .get(index)
                    .and_then(|e| e.icon.clone())
                    .unwrap_or_default();
                self.open_input_prompt(
                    config::InputPromptConfig {
                        title: Some("Icon Name or Path (empty = default)".into()),
                        default: Some(current),
                        allow_empty: true,
                        ..Default::default()
                    },
                    InputCallback::SettingsMainMenuIcon(index),
                );
            }
            SettingsAction::BrowseMainMenuIcon(index) => {
                if let Some(path) = self.browse_icon_dialog() {
                    self.mutate_settings_config(
                        |cfg| {
                            if let Some(entry) = cfg.main_menu.get_mut(index) {
                                entry.icon = Some(path.clone());
                            }
                        },
                        format!("Icon set to {path}"),
                    );
                }
            }
            SettingsAction::RemoveMainMenuEntry(index) => {
                let name = self
                    .config
                    .borrow()
                    .main_menu
                    .get(index)
                    .map(|e| e.name.clone());
                let Some(name) = name else {
                    return;
                };
                self.mutate_settings_config(
                    |cfg| {
                        if index < cfg.main_menu.len() {
                            cfg.main_menu.remove(index);
                        }
                    },
                    format!("Removed {name} from main menu"),
                );
                self.pop_settings_view();
            }
            SettingsAction::AddMainMenuEntry(name) => {
                let msg = format!("Added {name} to main menu");
                self.mutate_settings_config(
                    |cfg| {
                        cfg.main_menu.push(config::MainMenuEntry {
                            name: name.clone(),
                            label: None,
                            icon: None,
                        });
                    },
                    msg,
                );
                self.pop_settings_view();
            }
            SettingsAction::ImportMenuFile(path) => match self.store.import_menu_file(&path) {
                Ok(names) => match self.store.load_config().and_then(|c| self.apply_config(c)) {
                    Ok(()) => {
                        {
                            let mut stack = self.settings_stack.borrow_mut();
                            while matches!(
                                stack.last(),
                                Some(SettingsView::MainMenuImport(_) | SettingsView::MainMenuAdd)
                            ) {
                                stack.pop();
                            }
                            if stack.is_empty() {
                                stack.push(SettingsView::Root);
                            }
                        }
                        self.hover_position.set(None);
                        self.scroll.borrow_mut().reset();
                        self.back_stack.borrow_mut().clear();
                        self.menu.borrow_mut().query.clear();
                        self.set_input("");
                        self.filter();
                        self.show_notice(format!("Installed menu: {}", names.join(", ")));
                    }
                    Err(err) => self.show_notice(err),
                },
                Err(err) => self.show_notice(err),
            },
            SettingsAction::ResetMainMenu => {
                self.mutate_settings_config(
                    |cfg| cfg.main_menu = config::default_main_menu(),
                    "Main menu reset to defaults".into(),
                );
            }
            SettingsAction::ResetAll => {
                self.mutate_settings_config(
                    |cfg| *cfg = AppConfig::default(),
                    "All settings reset to defaults".into(),
                );
                self.pop_settings_view();
            }
        }
    }

    fn push_wifi_view(self: &Rc<Self>, view: WifiView) {
        self.wifi_stack.borrow_mut().push(view);
        self.hover_position.set(None);
        self.scroll.borrow_mut().reset();
        self.back_stack.borrow_mut().clear();
        self.menu.borrow_mut().query.clear();
        self.set_input("");
        self.filter();
    }

    fn pop_wifi_to_root(self: &Rc<Self>) {
        let mut stack = self.wifi_stack.borrow_mut();
        stack.truncate(1);
        if stack.is_empty() {
            stack.push(WifiView::Root);
        }
        drop(stack);
        self.hover_position.set(None);
        self.scroll.borrow_mut().reset();
        self.back_stack.borrow_mut().clear();
        self.menu.borrow_mut().query.clear();
        self.set_input("");
    }

    fn run_wifi_task<F>(
        self: &Rc<Self>,
        busy: String,
        success_notice: String,
        opts: WifiTaskOptions,
        task: F,
    ) where
        F: FnOnce() -> Result<(), String> + Send + 'static,
    {
        let op_id = self.op_gen.get().wrapping_add(1);
        self.op_gen.set(op_id);
        self.network_scanning.set(opts.spin);
        self.set_busy(Some(busy));
        if opts.spin {
            self.filter();
        }
        let p = self.clone();
        glib::MainContext::default().spawn_local(async move {
            let result = gio::spawn_blocking(move || {
                let op_result = task();
                let refreshed = wifi::load_state(opts.rescan_after);
                (op_result, refreshed)
            })
            .await
            .expect("wifi action worker");
            if p.op_gen.get() != op_id || p.menu.borrow().mode != Mode::Wifi {
                return;
            }
            p.network_scanning.set(false);
            p.set_busy(None);
            let (op_result, refreshed) = result;
            if let Ok(state) = refreshed {
                p.wifi_state.replace(Some(state));
            }
            match op_result {
                Ok(()) => {
                    if opts.pop_to_root_on_success {
                        p.pop_wifi_to_root();
                    } else if opts.pop_view_on_success {
                        let mut stack = p.wifi_stack.borrow_mut();
                        if stack.len() > 1 {
                            stack.pop();
                        }
                    }
                    p.filter();
                    if !success_notice.is_empty() {
                        p.show_notice(success_notice);
                    }
                }
                Err(err) => {
                    p.filter();
                    p.error(err);
                }
            }
        });
    }

    fn handle_wifi_action(self: &Rc<Self>, action: WifiAction) {
        match action {
            WifiAction::OpenView(view) => self.push_wifi_view(view),
            WifiAction::ToggleRadio(enable) => {
                let busy = if enable {
                    "Turning Wi-Fi on…"
                } else {
                    "Turning Wi-Fi off…"
                };
                let notice = if enable {
                    "Wi-Fi turned on"
                } else {
                    "Wi-Fi turned off"
                };
                self.run_wifi_task(
                    busy.into(),
                    notice.into(),
                    WifiTaskOptions {
                        rescan_after: enable,
                        ..Default::default()
                    },
                    move || wifi::set_radio(enable),
                );
            }
            WifiAction::Rescan => {
                self.run_wifi_task(
                    "Scanning for Wi-Fi networks…".into(),
                    "Wi-Fi scan complete".into(),
                    WifiTaskOptions {
                        rescan_after: true,
                        spin: true,
                        pop_to_root_on_success: true,
                        ..Default::default()
                    },
                    || Ok(()),
                );
            }
            WifiAction::Connect(net) => {
                if !net.is_saved() && net.is_secured() {
                    let ssid = net.ssid.clone();
                    self.open_input_prompt(
                        config::InputPromptConfig {
                            title: Some(ssid.clone()),
                            prompt: Some(format!("Enter password for \"{ssid}\"")),
                            placeholder: Some("Wi-Fi password…".into()),
                            default: None,
                            password: true,
                            allow_empty: false,
                        },
                        InputCallback::WifiPassword {
                            ssid,
                            saved_uuid: None,
                        },
                    );
                } else {
                    let ssid = net.ssid.clone();
                    let notice = format!("Connected to {ssid}");
                    self.run_wifi_task(
                        format!("Connecting to {ssid}…"),
                        notice,
                        WifiTaskOptions::default(),
                        move || wifi::connect_saved_or_open(&net),
                    );
                }
            }
            WifiAction::ConnectSaved { uuid, name } => {
                let notice = format!("Connected to {name}");
                self.run_wifi_task(
                    format!("Connecting to {name}…"),
                    notice,
                    WifiTaskOptions::default(),
                    move || wifi::connect_saved(&uuid),
                );
            }
            WifiAction::ConnectWithPassword { ssid, saved_uuid } => {
                self.open_input_prompt(
                    config::InputPromptConfig {
                        title: Some(ssid.clone()),
                        prompt: Some(format!("Enter password for \"{ssid}\"")),
                        placeholder: Some("Wi-Fi password…".into()),
                        default: None,
                        password: true,
                        allow_empty: false,
                    },
                    InputCallback::WifiPassword { ssid, saved_uuid },
                );
            }
            WifiAction::ConnectHidden => {
                self.open_input_prompt(
                    config::InputPromptConfig {
                        title: Some("Hidden Wi-Fi".into()),
                        prompt: Some("Network Name (SSID)".into()),
                        placeholder: Some("Enter hidden network SSID…".into()),
                        default: None,
                        password: false,
                        allow_empty: false,
                    },
                    InputCallback::WifiHiddenSsid,
                );
            }
            WifiAction::Disconnect {
                uuid,
                device,
                label,
            } => {
                let notice = format!("Disconnected from {label}");
                self.run_wifi_task(
                    format!("Disconnecting from {label}…"),
                    notice,
                    WifiTaskOptions::default(),
                    move || wifi::disconnect(uuid.as_deref(), device.as_deref()),
                );
            }
            WifiAction::SetAutoconnect { uuid, name, enable } => {
                let notice = if enable {
                    format!("Auto-connect enabled for {name}")
                } else {
                    format!("Auto-connect disabled for {name}")
                };
                self.run_wifi_task(
                    format!("Updating {name}…"),
                    notice,
                    WifiTaskOptions::default(),
                    move || wifi::set_autoconnect(&uuid, enable),
                );
            }
            WifiAction::Forget { uuid, name } => {
                let notice = format!("Forgot {name}");
                self.run_wifi_task(
                    format!("Forgetting {name}…"),
                    notice,
                    WifiTaskOptions {
                        pop_view_on_success: true,
                        ..Default::default()
                    },
                    move || wifi::forget(&uuid),
                );
            }
        }
    }

    fn push_bt_view(self: &Rc<Self>, view: BluetoothView) {
        let refresh_mac = match &view {
            BluetoothView::Device { mac, .. } => Some(mac.clone()),
            _ => None,
        };
        self.bt_stack.borrow_mut().push(view);
        self.hover_position.set(None);
        self.scroll.borrow_mut().reset();
        self.back_stack.borrow_mut().clear();
        self.menu.borrow_mut().query.clear();
        self.set_input("");
        self.filter();

        if let Some(mac) = refresh_mac {
            let op_id = self.op_gen.get();
            let p = self.clone();
            glib::MainContext::default().spawn_local(async move {
                let updated = gio::spawn_blocking(move || bluetooth::refresh_device(&mac))
                    .await
                    .expect("bluetooth info worker");
                if p.op_gen.get() != op_id || p.menu.borrow().mode != Mode::Bluetooth {
                    return;
                }
                if let Ok(Some(dev)) = updated {
                    if let Some(state) = p.bt_state.borrow_mut().as_mut() {
                        if let Some(existing) = state
                            .devices
                            .iter_mut()
                            .find(|d| d.mac.eq_ignore_ascii_case(&dev.mac))
                        {
                            *existing = dev;
                        } else {
                            state.devices.push(dev);
                        }
                    }
                    p.filter();
                }
            });
        }
    }

    fn pop_bt_to_root(self: &Rc<Self>) {
        let mut stack = self.bt_stack.borrow_mut();
        stack.truncate(1);
        if stack.is_empty() {
            stack.push(BluetoothView::Root);
        }
        drop(stack);
        self.hover_position.set(None);
        self.scroll.borrow_mut().reset();
        self.back_stack.borrow_mut().clear();
        self.menu.borrow_mut().query.clear();
        self.set_input("");
    }

    fn run_bt_task<F>(
        self: &Rc<Self>,
        busy: String,
        success_notice: String,
        spin: bool,
        pop_view_on_success: bool,
        pop_to_root_on_success: bool,
        task: F,
    ) where
        F: FnOnce() -> Result<(), String> + Send + 'static,
    {
        let op_id = self.op_gen.get().wrapping_add(1);
        self.op_gen.set(op_id);
        self.network_scanning.set(spin);
        self.set_busy(Some(busy));
        if spin {
            self.filter();
        }
        let p = self.clone();
        glib::MainContext::default().spawn_local(async move {
            let result = gio::spawn_blocking(move || {
                let op_result = task();
                let refreshed = bluetooth::load_state();
                (op_result, refreshed)
            })
            .await
            .expect("bluetooth action worker");
            if p.op_gen.get() != op_id || p.menu.borrow().mode != Mode::Bluetooth {
                return;
            }
            p.network_scanning.set(false);
            p.set_busy(None);
            let (op_result, refreshed) = result;
            if let Ok(state) = refreshed {
                p.bt_state.replace(Some(state));
            }
            match op_result {
                Ok(()) => {
                    if pop_to_root_on_success {
                        p.pop_bt_to_root();
                    } else if pop_view_on_success {
                        let mut stack = p.bt_stack.borrow_mut();
                        if stack.len() > 1 {
                            stack.pop();
                        }
                    }
                    p.filter();
                    if !success_notice.is_empty() {
                        p.show_notice(success_notice);
                    }
                }
                Err(err) => {
                    p.filter();
                    p.error(err);
                }
            }
        });
    }

    fn run_bt_pair(self: &Rc<Self>, mac: String, name: String, connect_after: bool) {
        let op_id = self.op_gen.get().wrapping_add(1);
        self.op_gen.set(op_id);
        self.set_busy(Some(format!("Pairing with {name}…")));

        let (prompt_tx, prompt_rx) = std::sync::mpsc::channel::<PairPrompt>();
        let p_prompt = self.clone();
        glib::timeout_add_local(Duration::from_millis(40), move || {
            loop {
                match prompt_rx.try_recv() {
                    Ok(prompt) => {
                        if p_prompt.op_gen.get() != op_id
                            || p_prompt.menu.borrow().mode != Mode::Bluetooth
                        {
                            match prompt {
                                PairPrompt::ConfirmPasskey { reply, .. } => {
                                    let _ = reply.send(false);
                                }
                                PairPrompt::RequestPin { reply, .. } => {
                                    let _ = reply.send(None);
                                }
                                PairPrompt::Status(_) => {}
                            }
                            return glib::ControlFlow::Break;
                        }
                        match prompt {
                            PairPrompt::Status(msg) => {
                                p_prompt.set_busy(Some(msg));
                            }
                            PairPrompt::ConfirmPasskey {
                                device,
                                passkey,
                                reply,
                            } => {
                                let ok = p_prompt.run_modal_dialog(|| {
                                    bluetooth::prompt_confirm_passkey(&device, &passkey)
                                });
                                let _ = reply.send(ok);
                            }
                            PairPrompt::RequestPin {
                                device,
                                prompt,
                                reply,
                            } => {
                                p_prompt.open_input_prompt(
                                    config::InputPromptConfig {
                                        title: Some(format!("Pair with {device}")),
                                        prompt: Some(prompt),
                                        placeholder: Some("PIN or passkey…".into()),
                                        default: None,
                                        password: false,
                                        allow_empty: false,
                                    },
                                    InputCallback::BluetoothPin { reply },
                                );
                            }
                        }
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => {
                        return glib::ControlFlow::Continue;
                    }
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        return glib::ControlFlow::Break;
                    }
                }
            }
        });

        let p = self.clone();
        let display_name = name.clone();
        glib::MainContext::default().spawn_local(async move {
            let result = gio::spawn_blocking(move || {
                let op_result = bluetooth::pair_interactive(&mac, &name, &prompt_tx).and_then(|()| {
                    let _ = bluetooth::set_trusted(&mac, true);
                    if connect_after {
                        let _ = prompt_tx.send(PairPrompt::Status(format!(
                            "Connecting to {name}…"
                        )));
                        bluetooth::connect(&mac)
                    } else {
                        Ok(())
                    }
                });
                drop(prompt_tx);
                let refreshed = bluetooth::load_state();
                (op_result, refreshed)
            })
            .await
            .expect("bluetooth pair worker");
            if p.op_gen.get() != op_id || p.menu.borrow().mode != Mode::Bluetooth {
                return;
            }
            p.set_busy(None);
            let (op_result, refreshed) = result;
            if let Ok(state) = refreshed {
                p.bt_state.replace(Some(state));
            }
            p.filter();
            match op_result {
                Ok(()) => {
                    let notice = if connect_after {
                        format!("Paired & connected to {display_name}")
                    } else {
                        format!("Paired with {display_name}")
                    };
                    p.show_notice(notice);
                }
                Err(err) => p.error(err),
            }
        });
    }

    fn handle_bluetooth_action(self: &Rc<Self>, action: BluetoothAction) {
        match action {
            BluetoothAction::OpenView(view) => self.push_bt_view(view),
            BluetoothAction::SetPower(enable) => {
                let busy = if enable {
                    "Turning Bluetooth on…"
                } else {
                    "Turning Bluetooth off…"
                };
                let notice = if enable {
                    "Bluetooth turned on"
                } else {
                    "Bluetooth turned off"
                };
                self.run_bt_task(busy.into(), notice.into(), false, false, false, move || {
                    bluetooth::set_power(enable)
                });
            }
            BluetoothAction::Scan => {
                self.run_bt_task(
                    "Scanning for Bluetooth devices…".into(),
                    "Bluetooth scan complete".into(),
                    true,
                    false,
                    true,
                    || bluetooth::scan(6),
                );
            }
            BluetoothAction::SetDiscoverable(enable) => {
                let notice = if enable {
                    "Adapter is now discoverable"
                } else {
                    "Adapter is now hidden"
                };
                self.run_bt_task(
                    "Updating visibility…".into(),
                    notice.into(),
                    false,
                    false,
                    false,
                    move || bluetooth::set_discoverable(enable),
                );
            }
            BluetoothAction::SetPairable(enable) => {
                let notice = if enable {
                    "Pairing enabled"
                } else {
                    "Pairing disabled"
                };
                self.run_bt_task(
                    "Updating pairable mode…".into(),
                    notice.into(),
                    false,
                    false,
                    false,
                    move || bluetooth::set_pairable(enable),
                );
            }
            BluetoothAction::SetAlias => {
                let current = self
                    .bt_state
                    .borrow()
                    .as_ref()
                    .map(|s| s.adapter.alias.clone())
                    .unwrap_or_default();
                self.open_input_prompt(
                    config::InputPromptConfig {
                        title: Some("Adapter Name".into()),
                        prompt: Some("Visible name for this computer".into()),
                        placeholder: Some("Enter adapter name…".into()),
                        default: (!current.is_empty()).then_some(current),
                        password: false,
                        allow_empty: false,
                    },
                    InputCallback::BluetoothAlias,
                );
            }
            BluetoothAction::Connect { mac, name } => {
                let notice = format!("Connected to {name}");
                self.run_bt_task(
                    format!("Connecting to {name}…"),
                    notice,
                    false,
                    false,
                    false,
                    move || bluetooth::connect(&mac),
                );
            }
            BluetoothAction::Disconnect { mac, name } => {
                let notice = format!("Disconnected {name}");
                self.run_bt_task(
                    format!("Disconnecting {name}…"),
                    notice,
                    false,
                    false,
                    false,
                    move || bluetooth::disconnect(&mac),
                );
            }
            BluetoothAction::DisconnectAll => {
                let devices = self
                    .bt_state
                    .borrow()
                    .as_ref()
                    .map(|s| s.devices.clone())
                    .unwrap_or_default();
                self.run_bt_task(
                    "Disconnecting all devices…".into(),
                    "Disconnected all Bluetooth devices".into(),
                    false,
                    false,
                    false,
                    move || bluetooth::disconnect_all(&devices),
                );
            }
            BluetoothAction::PairAndConnect { mac, name } => {
                self.run_bt_pair(mac, name, true);
            }
            BluetoothAction::PairOnly { mac, name } => {
                self.run_bt_pair(mac, name, false);
            }
            BluetoothAction::SetTrusted { mac, name, trusted } => {
                let notice = if trusted {
                    format!("Trusted {name}")
                } else {
                    format!("Untrusted {name}")
                };
                self.run_bt_task(
                    format!("Updating {name}…"),
                    notice,
                    false,
                    false,
                    false,
                    move || bluetooth::set_trusted(&mac, trusted),
                );
            }
            BluetoothAction::SetBlocked { mac, name, blocked } => {
                let notice = if blocked {
                    format!("Blocked {name}")
                } else {
                    format!("Unblocked {name}")
                };
                self.run_bt_task(
                    format!("Updating {name}…"),
                    notice,
                    false,
                    false,
                    false,
                    move || bluetooth::set_blocked(&mac, blocked),
                );
            }
            BluetoothAction::Remove { mac, name } => {
                let notice = format!("Removed {name}");
                self.run_bt_task(
                    format!("Removing {name}…"),
                    notice,
                    false,
                    true,
                    false,
                    move || bluetooth::remove(&mac),
                );
            }
        }
    }
}

fn number_from_keyval(keyval: gdk::keys::Key) -> Option<usize> {
    match keyval {
        key::_1 | key::KP_1 => Some(0),
        key::_2 | key::KP_2 => Some(1),
        key::_3 | key::KP_3 => Some(2),
        key::_4 | key::KP_4 => Some(3),
        key::_5 | key::KP_5 => Some(4),
        key::_6 | key::KP_6 => Some(5),
        key::_7 | key::KP_7 => Some(6),
        key::_8 | key::KP_8 => Some(7),
        _ => None,
    }
}

fn number_key(event: &gdk::EventKey) -> Option<usize> {
    if let Some(index) = number_from_keyval(event.keyval()) {
        return Some(index);
    }
    let state = event.state();
    if state.contains(gdk::ModifierType::SHIFT_MASK)
        && let Some(display) = gdk::Display::default()
        && let Some(keymap) = gdk::Keymap::for_display(&display)
        && let Some((unshifted, _, _, _)) = keymap.translate_keyboard_state(
            u32::from(event.hardware_keycode()),
            state - gdk::ModifierType::SHIFT_MASK,
            event.group() as i32,
        )
        && let Some(index) = number_from_keyval(unshifted.into())
    {
        return Some(index);
    }
    match event.hardware_keycode() {
        10..=17 => Some(usize::from(event.hardware_keycode() - 10)),
        _ => None,
    }
}

const MAX_TICK_DT: f64 = 1.0 / 30.0;

fn tick_dt(
    previous: &Cell<Option<i64>>,
    now: i64,
    started: i64,
    mapped_at_start: bool,
    refresh_interval_us: i64,
) -> f64 {
    match previous.get() {
        Some(prev) => {
            if now <= prev {
                return 0.0;
            }
            previous.set(Some(now));
            ((now - prev) as f64 / 1_000_000.0).min(MAX_TICK_DT)
        }
        None => {
            previous.set(Some(now));
            if !mapped_at_start {
                return 0.0;
            }
            let frame_dt = if refresh_interval_us > 0 {
                (refresh_interval_us as f64 / 1_000_000.0).min(MAX_TICK_DT)
            } else {
                1.0 / 60.0
            };
            ((now - started).max(0) as f64 / 1_000_000.0).min(frame_dt)
        }
    }
}

fn union_rect(a: cairo::RectangleInt, b: cairo::RectangleInt) -> cairo::RectangleInt {
    if a.width() <= 0 || a.height() <= 0 {
        return b;
    }
    if b.width() <= 0 || b.height() <= 0 {
        return a;
    }
    let x0 = a.x().min(b.x());
    let y0 = a.y().min(b.y());
    let x1 = (a.x() + a.width()).max(b.x() + b.width());
    let y1 = (a.y() + a.height()).max(b.y() + b.height());
    cairo::RectangleInt::new(x0, y0, x1 - x0, y1 - y0)
}

fn clamp_rect(rect: cairo::RectangleInt, max_w: i32, max_h: i32) -> Option<cairo::RectangleInt> {
    if max_w <= 0 || max_h <= 0 || rect.width() <= 0 || rect.height() <= 0 {
        return None;
    }
    let x0 = rect.x().clamp(0, max_w);
    let y0 = rect.y().clamp(0, max_h);
    let x1 = (rect.x() + rect.width()).clamp(0, max_w);
    let y1 = (rect.y() + rect.height()).clamp(0, max_h);
    (x1 > x0 && y1 > y0).then(|| cairo::RectangleInt::new(x0, y0, x1 - x0, y1 - y0))
}

fn clip_rects_from_context(cr: &cairo::Context) -> Option<Vec<cairo::Rectangle>> {
    let list = cr.copy_clip_rectangle_list().ok()?;
    Some(list.iter().copied().collect())
}

fn clip_rects_intersect(rects: &[cairo::Rectangle], rect: cairo::RectangleInt) -> bool {
    if rect.width() <= 0 || rect.height() <= 0 {
        return false;
    }
    let rx0 = f64::from(rect.x());
    let ry0 = f64::from(rect.y());
    let rx1 = f64::from(rect.x() + rect.width());
    let ry1 = f64::from(rect.y() + rect.height());
    rects.iter().any(|r| {
        r.width() > 0.0
            && r.height() > 0.0
            && r.x() + r.width() > rx0
            && r.x() < rx1
            && r.y() + r.height() > ry0
            && r.y() < ry1
    })
}

fn clip_rects_cover(rects: &[cairo::Rectangle], rect: cairo::RectangleInt) -> bool {
    if rect.width() <= 0 || rect.height() <= 0 {
        return true;
    }
    let rx0 = f64::from(rect.x());
    let ry0 = f64::from(rect.y());
    let rx1 = f64::from(rect.x() + rect.width());
    let ry1 = f64::from(rect.y() + rect.height());
    // Fast path for the common single-rectangle clip case.
    if rects.iter().any(|r| {
        r.x() <= rx0 && r.y() <= ry0 && r.x() + r.width() >= rx1 && r.y() + r.height() >= ry1
    }) {
        return true;
    }
    if rects.len() <= 1 {
        return false;
    }
    // Multi-rectangle clips (e.g. two adjacent exposes): shrink fractional edges
    // inward so we never over-claim coverage of partially clipped border pixels.
    let int_rects: Vec<cairo::RectangleInt> = rects
        .iter()
        .filter_map(|r| {
            let x0 = r.x().ceil() as i32;
            let y0 = r.y().ceil() as i32;
            let x1 = (r.x() + r.width()).floor() as i32;
            let y1 = (r.y() + r.height()).floor() as i32;
            (x1 > x0 && y1 > y0).then(|| cairo::RectangleInt::new(x0, y0, x1 - x0, y1 - y0))
        })
        .collect();
    if int_rects.is_empty() {
        return false;
    }
    cairo::Region::create_rectangles(&int_rects).contains_rectangle(&rect)
        == cairo::RegionOverlap::In
}

/// Maintains the invariant that whenever `Some(bounds)` is stored, every
/// non-transparent pixel on the `DrawingArea` lies within `bounds`.
fn next_painted_bounds(
    prev: Option<cairo::RectangleInt>,
    curr: Option<cairo::RectangleInt>,
    clip_rects: Option<&[cairo::Rectangle]>,
    full_surface: cairo::RectangleInt,
) -> Option<cairo::RectangleInt> {
    let rects = clip_rects?;
    match (prev, curr) {
        (Some(prev), Some(curr)) => {
            if clip_rects_cover(rects, prev) {
                Some(curr)
            } else {
                Some(union_rect(prev, curr))
            }
        }
        (Some(prev), None) => {
            if clip_rects_cover(rects, prev) {
                Some(cairo::RectangleInt::new(0, 0, 0, 0))
            } else {
                Some(prev)
            }
        }
        (None, Some(curr)) => clip_rects_cover(rects, full_surface).then_some(curr),
        (None, None) => {
            clip_rects_cover(rects, full_surface).then(|| cairo::RectangleInt::new(0, 0, 0, 0))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smooth_scroll_accumulates_touchpad_gestures_and_steps_wheel_notches() {
        let mut pager = ScrollPager::default();
        assert_eq!(pager.smooth(0.0, 1.0, false, false), 1);
        assert_eq!(pager.smooth(0.0, 1.5, false, false), 1);
        assert_eq!(pager.smooth(0.0, -1.0, false, false), -1);

        // Touchpad needs sustained travel and resets on stop or direction change.
        assert_eq!(pager.smooth(0.0, 1.0, false, true), 0);
        assert_eq!(pager.smooth(0.0, 1.0, false, true), 0);
        assert_eq!(pager.smooth(0.0, 1.0, false, true), 1);
        assert_eq!(pager.smooth(0.0, 2.0, false, true), 0);
        assert_eq!(pager.smooth(0.0, 0.0, true, true), 0);
        assert_eq!(pager.smooth(0.0, 1.5, false, true), 0);
        assert_eq!(pager.smooth(0.0, -2.0, false, true), 0);
        assert_eq!(pager.smooth(0.0, -1.0, false, true), -1);
        // Horizontal two-finger swipe also works.
        assert_eq!(pager.smooth(3.0, 0.1, false, true), 1);
    }

    #[test]
    fn custom_menu_stack_navigates_nested_submenus_and_returns_to_root() {
        let raw = r#"{
            "title": "Power",
            "items": [
                { "label": "Shutdown", "left_click": { "submenu": "confirm" } }
            ],
            "submenus": {
                "confirm": {
                    "title": "Confirm",
                    "items": [
                        { "label": "Deep", "left_click": { "submenu": "deep" } }
                    ]
                },
                "deep": {
                    "items": [
                        { "label": "Now", "left_click": "systemctl poweroff" }
                    ]
                }
            }
        }"#;
        let cfg: config::MenuConfig = serde_json::from_str(raw).unwrap();
        let mut active = ActiveCustomMenu::new("power".into(), cfg);
        assert_eq!(active.current_title(), "Power");
        assert_eq!(active.current_items().len(), 1);
        assert!(!active.pop_submenu());

        active.push_submenu("confirm".into()).unwrap();
        assert_eq!(active.current_title(), "Confirm");
        assert_eq!(active.current_items()[0].label, "Deep");

        active.push_submenu("deep".into()).unwrap();
        assert_eq!(active.current_title(), "deep");
        assert_eq!(active.current_items()[0].label, "Now");

        assert!(active.push_submenu("missing".into()).is_err());

        assert!(active.pop_submenu());
        assert_eq!(active.current_title(), "Confirm");
        assert!(active.pop_submenu());
        assert_eq!(active.current_title(), "Power");
        assert!(!active.pop_submenu());
    }

    #[test]
    fn custom_menu_loading_animation_inherits_from_root_and_respects_submenu_overrides() {
        let raw = r#"{
            "title": "Devices",
            "loading_animation": true,
            "items": [],
            "submenus": {
                "inherit": { "items": [] },
                "disable": { "loading_animation": false, "items": [] }
            }
        }"#;
        let cfg: config::MenuConfig = serde_json::from_str(raw).unwrap();
        let mut active = ActiveCustomMenu::new("devices".into(), cfg);
        assert!(active.current_loading_animation());

        active.push_submenu("inherit".into()).unwrap();
        assert!(active.current_loading_animation());
        active.pop_submenu();

        active.push_submenu("disable".into()).unwrap();
        assert!(!active.current_loading_animation());
        active.pop_submenu();

        assert_eq!(active.submenu_refresh_info("inherit").unwrap(), (None, true));
        assert_eq!(
            active.submenu_refresh_info("disable").unwrap(),
            (None, false)
        );
        assert!(active.submenu_refresh_info("missing").is_err());
    }

    #[test]
    fn custom_menu_stack_handles_list_and_input_submenus_and_inline_dropdowns() {
        let raw = r#"{
            "title": "Settings",
            "items": [
                {
                    "label": "Quick Dropdown",
                    "left_click": {
                        "title": "Select Mode",
                        "options": ["Fast", "Quiet"],
                        "command": "set-mode {}"
                    }
                }
            ],
            "submenus": {
                "list_sub": {
                    "title": "Choose Output",
                    "layout": "list",
                    "prompt": "Pick an output",
                    "options": ["HDMI", "DP"],
                    "command": "set-output {}"
                },
                "input_sub": {
                    "title": "New Name",
                    "layout": "input",
                    "prompt": "Enter device name",
                    "default": "testhost",
                    "command": "set-name {}"
                }
            }
        }"#;
        let cfg: config::MenuConfig = serde_json::from_str(raw).unwrap();
        let mut active = ActiveCustomMenu::new("settings".into(), cfg);
        assert_eq!(active.current_layout(), config::MenuLayout::Ring);
        assert!(active.current_input_action().is_none());

        active.push_submenu("list_sub".into()).unwrap();
        assert_eq!(active.current_layout(), config::MenuLayout::List);
        assert_eq!(active.current_title(), "Choose Output");
        assert_eq!(active.current_prompt(), Some("Pick an output"));
        assert_eq!(active.current_items().len(), 2);
        assert!(active.pop_submenu());

        active.push_submenu("input_sub".into()).unwrap();
        assert_eq!(active.current_layout(), config::MenuLayout::Input);
        let (prompt_cfg, cmd, _, _, _) = active.current_input_action().unwrap();
        assert_eq!(prompt_cfg.title.as_deref(), Some("New Name"));
        assert_eq!(prompt_cfg.default.as_deref(), Some("testhost"));
        assert_eq!(cmd.as_deref(), Some("set-name {}"));
        assert!(active.pop_submenu());

        let inline = active.current_items()[0]
            .action(false)
            .unwrap()
            .inline_list_submenu("Quick Dropdown")
            .unwrap();
        active.push_inline_list(inline);
        assert_eq!(active.current_layout(), config::MenuLayout::List);
        assert_eq!(active.current_title(), "Select Mode");
        assert_eq!(active.current_items().len(), 2);
        assert!(active.pop_submenu());
        assert_eq!(active.current_layout(), config::MenuLayout::Ring);
    }

    #[test]
    fn tick_dt_avoids_startup_jump_and_clamps_frame_stalls() {
        // Before the window is mapped, frame 0 starts cleanly at dt=0 regardless of map delay.
        let prev = Cell::new(None);
        assert_eq!(tick_dt(&prev, 1_120_000, 1_000_000, false, 6_944), 0.0);
        // Frame 1 advances by the true 144Hz frame interval.
        let dt1 = tick_dt(&prev, 1_126_944, 1_000_000, false, 6_944);
        assert!((dt1 - 0.006_944).abs() < 1e-6);
        // A sudden 100ms stall is clamped so springs never teleport across their transition.
        assert_eq!(
            tick_dt(&prev, 1_226_944, 1_000_000, false, 6_944),
            MAX_TICK_DT
        );
        // Backward frame timestamps never produce negative dt AND do not rewind `previous`.
        assert_eq!(tick_dt(&prev, 1_200_000, 1_000_000, false, 6_944), 0.0);
        let dt_after_jitter = tick_dt(&prev, 1_233_888, 1_000_000, false, 6_944);
        assert!(
            (dt_after_jitter - 0.006_944).abs() < 1e-6,
            "jitter must not rewind previous: got {dt_after_jitter}"
        );

        // On an already-mapped window, initial tick is capped to one refresh interval.
        let mapped_prev = Cell::new(None);
        let first = tick_dt(&mapped_prev, 2_050_000, 2_000_000, true, 6_944);
        assert!((first - 0.006_944).abs() < 1e-6);
        // If clock.frame_time() snapped before `started`, dt is 0 (no negative step).
        let backward_prev = Cell::new(None);
        assert_eq!(tick_dt(&backward_prev, 1_999_000, 2_000_000, true, 0), 0.0);
    }

    #[test]
    fn damage_rect_union_clamp_and_clip_predicates_handle_edges() {
        let a = cairo::RectangleInt::new(100, 100, 200, 200);
        let b = cairo::RectangleInt::new(250, 50, 200, 150);
        let u = union_rect(a, b);
        assert_eq!((u.x(), u.y(), u.width(), u.height()), (100, 50, 350, 250));

        // Clamping clips negative origins and oversize extents to the surface bounds.
        let off = cairo::RectangleInt::new(-10, -20, 50, 60);
        let clamped = clamp_rect(off, 1920, 1080).unwrap();
        assert_eq!(
            (clamped.x(), clamped.y(), clamped.width(), clamped.height()),
            (0, 0, 40, 40)
        );
        assert!(clamp_rect(cairo::RectangleInt::new(2000, 0, 50, 50), 1920, 1080).is_none());
        assert!(clamp_rect(a, 0, 1080).is_none());

        let r = |x0: f64, y0: f64, x1: f64, y1: f64| cairo::Rectangle::new(x0, y0, x1 - x0, y1 - y0);

        // Entry expose at (0, 0, 20, 25) does not intersect a centered picker.
        let centered = cairo::RectangleInt::new(760, 300, 400, 480);
        assert!(!clip_rects_intersect(&[r(0.0, 0.0, 20.0, 25.0)], centered));
        assert!(clip_rects_intersect(&[r(700.0, 250.0, 800.0, 350.0)], centered));
        assert!(!clip_rects_cover(&[r(700.0, 250.0, 800.0, 350.0)], centered));
        assert!(clip_rects_cover(&[r(760.0, 300.0, 1160.0, 780.0)], centered));
        assert!(!clip_rects_cover(&[r(761.0, 300.0, 1160.0, 780.0)], centered));

        let full = cairo::RectangleInt::new(0, 0, 1920, 1080);
        let big = cairo::RectangleInt::new(650, 290, 620, 500);
        let small = cairo::RectangleInt::new(760, 300, 400, 480);
        // Two disjoint clips whose bounding box covers `big` must NOT claim `big` was cleared!
        let disjoint = [r(0.0, 0.0, 25.0, 30.0), r(1000.0, 600.0, 1300.0, 900.0)];
        assert!(!clip_rects_cover(&disjoint, big));
        assert_eq!(
            next_painted_bounds(Some(big), Some(small), Some(&disjoint), full),
            Some(big)
        );
        // Two adjacent tiles that genuinely cover `big` DO count as covering `big`.
        let adjacent = [r(650.0, 290.0, 960.0, 790.0), r(960.0, 290.0, 1270.0, 790.0)];
        assert!(clip_rects_cover(&adjacent, big));
        assert_eq!(
            next_painted_bounds(Some(big), Some(small), Some(&adjacent), full),
            Some(small)
        );

        // Before any full clear, partial exposes keep painted_bounds = None.
        assert_eq!(
            next_painted_bounds(None, Some(small), Some(&[r(760.0, 300.0, 1160.0, 780.0)]), full),
            None
        );
        // First full expose establishes exact painted_bounds.
        assert_eq!(
            next_painted_bounds(None, Some(small), Some(&[r(0.0, 0.0, 1920.0, 1080.0)]), full),
            Some(small)
        );
        // Shrinking from `big` to `small` only shrinks painted_bounds when `big` was cleared.
        assert_eq!(
            next_painted_bounds(
                Some(big),
                Some(small),
                Some(&[r(760.0, 300.0, 1160.0, 780.0)]),
                full
            ),
            Some(big)
        );
        assert_eq!(
            next_painted_bounds(
                Some(big),
                Some(small),
                Some(&[r(650.0, 290.0, 1270.0, 790.0)]),
                full
            ),
            Some(small)
        );
        // Growing during a partial expose unions prev and curr so new pixels are never forgotten.
        assert_eq!(
            next_painted_bounds(
                Some(small),
                Some(big),
                Some(&[r(700.0, 290.0, 900.0, 790.0)]),
                full
            ),
            Some(big)
        );
        // Clearing to empty (`None`) resets painted_bounds to empty once `prev` is covered.
        let cleared = next_painted_bounds(
            Some(small),
            None,
            Some(&[r(760.0, 300.0, 1160.0, 780.0)]),
            full,
        )
        .unwrap();
        assert_eq!((cleared.width(), cleared.height()), (0, 0));

        // Verify `clip_rects_from_context` captures real multi-rect Cairo clips accurately.
        let surf = cairo::ImageSurface::create(cairo::Format::ARgb32, 1920, 1080).unwrap();
        let cr = cairo::Context::new(&surf).unwrap();
        cr.rectangle(0.0, 0.0, 25.0, 30.0);
        cr.rectangle(1000.0, 600.0, 300.0, 300.0);
        cr.clip();
        let ctx_rects = clip_rects_from_context(&cr).unwrap();
        assert!(!clip_rects_cover(&ctx_rects, big));
        assert!(clip_rects_intersect(&ctx_rects, big));
    }

    #[test]
    fn instance_guard_closes_older_instances_and_keeps_newest() {
        use std::sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        };

        let dir = env::temp_dir().join(format!("recircles-instance-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);

        let ctx = glib::MainContext::default();
        let pump_until = |flag: &AtomicBool| {
            let start = std::time::Instant::now();
            while !flag.load(Ordering::SeqCst)
                && start.elapsed() < Duration::from_millis(500)
            {
                ctx.iteration(false);
                std::thread::sleep(Duration::from_millis(5));
            }
        };

        let first_closed = Arc::new(AtomicBool::new(false));
        let first_flag = first_closed.clone();
        let first_guard = InstanceGuard::acquire_in(&dir, move || {
            first_flag.store(true, Ordering::SeqCst);
        })
        .unwrap();
        assert!(!first_closed.load(Ordering::SeqCst));

        // When a newer instance arrives, it immediately takes over without blocking on the old loop.
        let second_closed = Arc::new(AtomicBool::new(false));
        let second_flag = second_closed.clone();
        let second_guard = InstanceGuard::acquire_in(&dir, move || {
            second_flag.store(true, Ordering::SeqCst);
        })
        .unwrap();
        pump_until(&first_closed);
        assert!(first_closed.load(Ordering::SeqCst));
        // Dropping the old guard after replacement must not remove the new instance's socket.
        drop(first_guard);
        assert!(dir.join("instance.sock").exists());
        assert!(!second_closed.load(Ordering::SeqCst));

        // A third instance immediately takes over and closes the second one.
        let third_closed = Arc::new(AtomicBool::new(false));
        let third_flag = third_closed.clone();
        let third_guard = InstanceGuard::acquire_in(&dir, move || {
            third_flag.store(true, Ordering::SeqCst);
        })
        .unwrap();
        pump_until(&second_closed);
        assert!(second_closed.load(Ordering::SeqCst));
        drop(second_guard);
        assert!(dir.join("instance.sock").exists());
        assert!(!third_closed.load(Ordering::SeqCst));

        // Normal exit of the newest instance cleans up the socket file.
        drop(third_guard);
        assert!(!dir.join("instance.sock").exists());
        let _ = fs::remove_dir_all(&dir);
    }
}
