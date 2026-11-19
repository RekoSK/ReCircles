use crate::config::{
    self, AccentColors, AppConfig, AppearanceConfig, CapsuleColors, ColorsConfig, ConfigStore,
    KeyList, KeybindsConfig, MainMenuEntry, OverflowLayout, RingColors, TextColors,
};
use crate::menu::Mode;
use crate::ring::Icon;
use gtk::gdk;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorGroup {
    Ring,
    Capsule,
    Text,
    Accents,
}

impl ColorGroup {
    pub fn title(self) -> &'static str {
        match self {
            Self::Ring => "Ring Colors",
            Self::Capsule => "Capsule Colors",
            Self::Text => "Text Colors",
            Self::Accents => "Accent Colors",
        }
    }

    pub fn fields(self) -> &'static [ColorField] {
        match self {
            Self::Ring => &[
                ColorField::RingTop,
                ColorField::RingMid,
                ColorField::RingBottom,
                ColorField::RimTop,
                ColorField::RimBottom,
                ColorField::Divider,
                ColorField::Shadow,
            ],
            Self::Capsule => &[
                ColorField::CapsuleTop,
                ColorField::CapsuleBottom,
                ColorField::CapsuleBorder,
            ],
            Self::Text => &[
                ColorField::TextPrimary,
                ColorField::TextCapsule,
                ColorField::TextSuggestion,
                ColorField::TextShadow,
            ],
            Self::Accents => &[
                ColorField::AccentFiles,
                ColorField::AccentApps,
                ColorField::AccentWindows,
                ColorField::AccentWifi,
                ColorField::AccentBluetooth,
                ColorField::AccentCustom,
                ColorField::AccentAction,
            ],
        }
    }

    pub fn reset(self, colors: &mut ColorsConfig) {
        match self {
            Self::Ring => colors.ring = RingColors::default(),
            Self::Capsule => colors.capsule = CapsuleColors::default(),
            Self::Text => colors.text = TextColors::default(),
            Self::Accents => colors.accents = AccentColors::default(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorField {
    RingTop,
    RingMid,
    RingBottom,
    RimTop,
    RimBottom,
    Divider,
    Shadow,
    CapsuleTop,
    CapsuleBottom,
    CapsuleBorder,
    TextPrimary,
    TextCapsule,
    TextSuggestion,
    TextShadow,
    AccentFiles,
    AccentApps,
    AccentWindows,
    AccentWifi,
    AccentBluetooth,
    AccentCustom,
    AccentAction,
}

impl ColorField {
    pub fn label(self) -> &'static str {
        match self {
            Self::RingTop => "Ring Top",
            Self::RingMid => "Ring Mid",
            Self::RingBottom => "Ring Bottom",
            Self::RimTop => "Rim Top",
            Self::RimBottom => "Rim Bottom",
            Self::Divider => "Divider",
            Self::Shadow => "Ring Shadow",
            Self::CapsuleTop => "Capsule Top",
            Self::CapsuleBottom => "Capsule Bottom",
            Self::CapsuleBorder => "Capsule Border",
            Self::TextPrimary => "Primary Text",
            Self::TextCapsule => "Capsule Text",
            Self::TextSuggestion => "Suggestion Text",
            Self::TextShadow => "Text Shadow",
            Self::AccentFiles => "Files Accent",
            Self::AccentApps => "Apps Accent",
            Self::AccentWindows => "Windows Accent",
            Self::AccentWifi => "Wi-Fi Accent",
            Self::AccentBluetooth => "Bluetooth Accent",
            Self::AccentCustom => "Custom Accent",
            Self::AccentAction => "Action Accent",
        }
    }

    pub fn short_label(self) -> &'static str {
        match self {
            Self::RingTop => "Top",
            Self::RingMid => "Mid",
            Self::RingBottom => "Bottom",
            Self::RimTop => "Rim Top",
            Self::RimBottom => "Rim Bottom",
            Self::Divider => "Divider",
            Self::Shadow => "Shadow",
            Self::CapsuleTop => "Top",
            Self::CapsuleBottom => "Bottom",
            Self::CapsuleBorder => "Border",
            Self::TextPrimary => "Primary",
            Self::TextCapsule => "Capsule",
            Self::TextSuggestion => "Suggestion",
            Self::TextShadow => "Shadow",
            Self::AccentFiles => "Files",
            Self::AccentApps => "Apps",
            Self::AccentWindows => "Windows",
            Self::AccentWifi => "Wi-Fi",
            Self::AccentBluetooth => "Bluetooth",
            Self::AccentCustom => "Custom",
            Self::AccentAction => "Action",
        }
    }

    pub fn get(self, colors: &ColorsConfig) -> &str {
        match self {
            Self::RingTop => &colors.ring.top,
            Self::RingMid => &colors.ring.mid,
            Self::RingBottom => &colors.ring.bottom,
            Self::RimTop => &colors.ring.rim_top,
            Self::RimBottom => &colors.ring.rim_bottom,
            Self::Divider => &colors.ring.divider,
            Self::Shadow => &colors.ring.shadow,
            Self::CapsuleTop => &colors.capsule.top,
            Self::CapsuleBottom => &colors.capsule.bottom,
            Self::CapsuleBorder => &colors.capsule.border,
            Self::TextPrimary => &colors.text.primary,
            Self::TextCapsule => &colors.text.capsule,
            Self::TextSuggestion => &colors.text.suggestion,
            Self::TextShadow => &colors.text.shadow,
            Self::AccentFiles => &colors.accents.files,
            Self::AccentApps => &colors.accents.apps,
            Self::AccentWindows => &colors.accents.windows,
            Self::AccentWifi => &colors.accents.wifi,
            Self::AccentBluetooth => &colors.accents.bluetooth,
            Self::AccentCustom => &colors.accents.custom,
            Self::AccentAction => &colors.accents.action,
        }
    }

    pub fn set(self, colors: &mut ColorsConfig, value: String) {
        let slot = match self {
            Self::RingTop => &mut colors.ring.top,
            Self::RingMid => &mut colors.ring.mid,
            Self::RingBottom => &mut colors.ring.bottom,
            Self::RimTop => &mut colors.ring.rim_top,
            Self::RimBottom => &mut colors.ring.rim_bottom,
            Self::Divider => &mut colors.ring.divider,
            Self::Shadow => &mut colors.ring.shadow,
            Self::CapsuleTop => &mut colors.capsule.top,
            Self::CapsuleBottom => &mut colors.capsule.bottom,
            Self::CapsuleBorder => &mut colors.capsule.border,
            Self::TextPrimary => &mut colors.text.primary,
            Self::TextCapsule => &mut colors.text.capsule,
            Self::TextSuggestion => &mut colors.text.suggestion,
            Self::TextShadow => &mut colors.text.shadow,
            Self::AccentFiles => &mut colors.accents.files,
            Self::AccentApps => &mut colors.accents.apps,
            Self::AccentWindows => &mut colors.accents.windows,
            Self::AccentWifi => &mut colors.accents.wifi,
            Self::AccentBluetooth => &mut colors.accents.bluetooth,
            Self::AccentCustom => &mut colors.accents.custom,
            Self::AccentAction => &mut colors.accents.action,
        };
        *slot = value;
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeybindField {
    Back,
    Home,
    Activate,
    SecondaryActivate,
    OpenCurrentFolder,
    Next,
    Previous,
    PageNext,
    PagePrevious,
    AcceptCompletion,
    UndoCompletion,
    ClearQuery,
}

impl KeybindField {
    pub const ALL: &'static [Self] = &[
        Self::Back,
        Self::Home,
        Self::Activate,
        Self::SecondaryActivate,
        Self::OpenCurrentFolder,
        Self::Next,
        Self::Previous,
        Self::PageNext,
        Self::PagePrevious,
        Self::AcceptCompletion,
        Self::UndoCompletion,
        Self::ClearQuery,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Back => "Back",
            Self::Home => "Home",
            Self::Activate => "Activate",
            Self::SecondaryActivate => "Secondary Activate",
            Self::OpenCurrentFolder => "Open Folder",
            Self::Next => "Next Item",
            Self::Previous => "Previous Item",
            Self::PageNext => "Next Page",
            Self::PagePrevious => "Previous Page",
            Self::AcceptCompletion => "Accept Completion",
            Self::UndoCompletion => "Undo Completion",
            Self::ClearQuery => "Clear Query",
        }
    }

    pub fn get(self, kb: &KeybindsConfig) -> &KeyList {
        match self {
            Self::Back => &kb.back,
            Self::Home => &kb.home,
            Self::Activate => &kb.activate,
            Self::SecondaryActivate => &kb.secondary_activate,
            Self::OpenCurrentFolder => &kb.open_current_folder,
            Self::Next => &kb.next,
            Self::Previous => &kb.previous,
            Self::PageNext => &kb.page_next,
            Self::PagePrevious => &kb.page_previous,
            Self::AcceptCompletion => &kb.accept_completion,
            Self::UndoCompletion => &kb.undo_completion,
            Self::ClearQuery => &kb.clear_query,
        }
    }

    pub fn get_mut(self, kb: &mut KeybindsConfig) -> &mut KeyList {
        match self {
            Self::Back => &mut kb.back,
            Self::Home => &mut kb.home,
            Self::Activate => &mut kb.activate,
            Self::SecondaryActivate => &mut kb.secondary_activate,
            Self::OpenCurrentFolder => &mut kb.open_current_folder,
            Self::Next => &mut kb.next,
            Self::Previous => &mut kb.previous,
            Self::PageNext => &mut kb.page_next,
            Self::PagePrevious => &mut kb.page_previous,
            Self::AcceptCompletion => &mut kb.accept_completion,
            Self::UndoCompletion => &mut kb.undo_completion,
            Self::ClearQuery => &mut kb.clear_query,
        }
    }

    pub fn reset(self, kb: &mut KeybindsConfig) {
        let defaults = KeybindsConfig::default();
        *self.get_mut(kb) = self.get(&defaults).clone();
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppearanceToggle {
    OverflowLayout,
    Animations,
    Animation3d,
    Translucent,
    Blur,
    HighContrast,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SettingsView {
    Root,
    Appearance,
    Colors,
    ColorTheme,
    ThemeImport(PathBuf),
    ColorGroup(ColorGroup),
    Keybinds,
    Keybind(KeybindField),
    MainMenu,
    MainMenuEntry(usize),
    MainMenuAdd,
    MainMenuImport(PathBuf),
    ConfirmResetAll,
}

impl SettingsView {
    pub fn layout(&self) -> config::MenuLayout {
        match self {
            Self::ColorTheme => config::MenuLayout::List,
            _ => config::MenuLayout::Ring,
        }
    }

    pub fn title(&self, config: &AppConfig) -> String {
        match self {
            Self::Root => "Settings".into(),
            Self::Appearance => "Appearance".into(),
            Self::Colors => "Colors".into(),
            Self::ColorTheme => "Theme".into(),
            Self::ThemeImport(dir) => import_dir_title("Add Theme", dir),
            Self::ColorGroup(group) => group.title().into(),
            Self::Keybinds => "Keybinds".into(),
            Self::Keybind(field) => field.label().into(),
            Self::MainMenu => "Main Menu".into(),
            Self::MainMenuEntry(index) => config
                .main_menu
                .get(*index)
                .map(entry_display_label)
                .unwrap_or_else(|| "Main Menu Entry".into()),
            Self::MainMenuAdd => "Add to Main Menu".into(),
            Self::MainMenuImport(dir) => import_dir_title("Add Menu", dir),
            Self::ConfirmResetAll => "Reset All Settings?".into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SettingsAction {
    OpenView(SettingsView),
    ToggleAppearance(AppearanceToggle),
    ResetAppearance,
    ApplyTheme(String),
    ImportThemeFile(PathBuf),
    PickColor(ColorField),
    ResetColorGroup(ColorGroup),
    ResetAllColors,
    ToggleNumberShortcuts,
    CaptureKeybind(KeybindField),
    TypeKeybind(KeybindField),
    RemoveKeybind { field: KeybindField, index: usize },
    ResetKeybind(KeybindField),
    ResetAllKeybinds,
    MoveMainMenu { index: usize, delta: isize },
    EditMainMenuLabel(usize),
    EditMainMenuIcon(usize),
    BrowseMainMenuIcon(usize),
    RemoveMainMenuEntry(usize),
    AddMainMenuEntry(String),
    ImportMenuFile(PathBuf),
    ResetMainMenu,
    ResetAll,
    GoBack,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SettingsItem {
    pub label: String,
    pub detail: String,
    pub icon: Icon,
    pub theme_icon: String,
    pub active: bool,
    pub action: SettingsAction,
    pub secondary_action: Option<SettingsAction>,
}

fn swatch_icon(hex: &str) -> Icon {
    let (r, g, b) = config::parse_hex_rgb_u8(hex).unwrap_or((180, 140, 100));
    Icon::ColorSwatch(r, g, b)
}

fn entry_display_label(entry: &MainMenuEntry) -> String {
    if let Some(label) = entry.label.as_deref().filter(|l| !l.trim().is_empty()) {
        return label.to_owned();
    }
    if let Some(mode) = config::parse_builtin(&entry.name) {
        return mode.title().to_owned();
    }
    entry.name.clone()
}

fn builtin_icon(mode: Mode) -> Icon {
    match mode {
        Mode::Files => Icon::Files,
        Mode::Apps => Icon::Applications,
        Mode::Windows => Icon::Windows,
        Mode::Wifi => Icon::Wifi,
        Mode::Bluetooth => Icon::Bluetooth,
        Mode::Settings => Icon::Settings,
        Mode::Home | Mode::Custom => Icon::CustomMenu,
    }
}

fn format_key_list(list: &KeyList) -> String {
    if list.0.is_empty() {
        "Unbound".into()
    } else {
        list.0.join(", ")
    }
}

pub fn build_items(
    config: &AppConfig,
    store: &ConfigStore,
    view: &SettingsView,
) -> Vec<SettingsItem> {
    match view {
        SettingsView::Root => build_root_items(),
        SettingsView::Appearance => build_appearance_items(&config.appearance),
        SettingsView::Colors => build_colors_items(&config.colors, store),
        SettingsView::ColorTheme => build_color_theme_items(&config.colors, store),
        SettingsView::ThemeImport(dir) => build_theme_import_items(dir),
        SettingsView::ColorGroup(group) => build_color_group_items(&config.colors, *group),
        SettingsView::Keybinds => build_keybinds_items(&config.keybinds),
        SettingsView::Keybind(field) => build_keybind_detail_items(&config.keybinds, *field),
        SettingsView::MainMenu => build_main_menu_items(config),
        SettingsView::MainMenuEntry(index) => build_main_menu_entry_items(config, *index),
        SettingsView::MainMenuAdd => build_main_menu_add_items(config, store),
        SettingsView::MainMenuImport(dir) => build_main_menu_import_items(dir),
        SettingsView::ConfirmResetAll => build_confirm_reset_items(),
    }
}

pub fn idle_status(config: &AppConfig, view: &SettingsView) -> String {
    match view {
        SettingsView::Root => "Changes save to recircles.json and apply immediately".into(),
        SettingsView::Appearance => "Click any option to toggle".into(),
        SettingsView::Colors => "Choose a theme or color category".into(),
        SettingsView::ColorTheme => "Click a theme to apply · Esc to go back".into(),
        SettingsView::ThemeImport(dir) => {
            format!("{} · Select a .json or .zip theme", dir.display())
        }
        SettingsView::ColorGroup(_) => "Click any color to open the RGB picker".into(),
        SettingsView::Keybinds => "Click a binding to edit · Esc to go back".into(),
        SettingsView::Keybind(field) => {
            format!("Current: {}", format_key_list(field.get(&config.keybinds)))
        }
        SettingsView::MainMenu => {
            format!("{} items in main ring · Click to reorder or edit", config.main_menu.len())
        }
        SettingsView::MainMenuEntry(index) => config
            .main_menu
            .get(*index)
            .map(|e| format!("Position #{} · {}", index + 1, e.name))
            .unwrap_or_else(|| "Entry not found".into()),
        SettingsView::MainMenuAdd => "Pin a menu or choose Add Menu to import .json / .zip".into(),
        SettingsView::MainMenuImport(dir) => {
            format!("{} · Select a .json or .zip menu", dir.display())
        }
        SettingsView::ConfirmResetAll => "This will overwrite recircles.json with defaults".into(),
    }
}

fn build_root_items() -> Vec<SettingsItem> {
    vec![
        SettingsItem {
            label: "Appearance".into(),
            detail: "Layout, animations, translucency, blur and contrast".into(),
            icon: Icon::Appearance,
            theme_icon: String::new(),
            active: false,
            action: SettingsAction::OpenView(SettingsView::Appearance),
            secondary_action: None,
        },
        SettingsItem {
            label: "Colors".into(),
            detail: "Ring, capsule, text and accent hex colors".into(),
            icon: Icon::Palette,
            theme_icon: String::new(),
            active: false,
            action: SettingsAction::OpenView(SettingsView::Colors),
            secondary_action: None,
        },
        SettingsItem {
            label: "Keybinds".into(),
            detail: "Navigation, activation, completion and number shortcuts".into(),
            icon: Icon::Keyboard,
            theme_icon: String::new(),
            active: false,
            action: SettingsAction::OpenView(SettingsView::Keybinds),
            secondary_action: None,
        },
        SettingsItem {
            label: "Main Menu".into(),
            detail: "Reorder, customize labels/icons, add or remove entries".into(),
            icon: Icon::CustomMenu,
            theme_icon: String::new(),
            active: false,
            action: SettingsAction::OpenView(SettingsView::MainMenu),
            secondary_action: None,
        },
        SettingsItem {
            label: "Reset All".into(),
            detail: "Restore all settings in recircles.json to defaults".into(),
            icon: Icon::Reset,
            theme_icon: String::new(),
            active: false,
            action: SettingsAction::OpenView(SettingsView::ConfirmResetAll),
            secondary_action: None,
        },
    ]
}

fn toggle_item(
    label: &str,
    on: bool,
    on_detail: &str,
    off_detail: &str,
    action: SettingsAction,
) -> SettingsItem {
    SettingsItem {
        label: label.into(),
        detail: if on {
            format!("On · {on_detail}")
        } else {
            format!("Off · {off_detail}")
        },
        icon: if on { Icon::ToggleOn } else { Icon::ToggleOff },
        theme_icon: String::new(),
        active: on,
        action,
        secondary_action: None,
    }
}

fn build_appearance_items(a: &AppearanceConfig) -> Vec<SettingsItem> {
    let is_box = a.overflow_layout == OverflowLayout::Box;
    vec![
        SettingsItem {
            label: if is_box {
                "Overflow: Box".into()
            } else {
                "Overflow: Rings".into()
            },
            detail: if is_box {
                "Grid box for 9+ items · Click to switch to paged rings".into()
            } else {
                "Paged rings for 9+ items · Click to switch to grid box".into()
            },
            icon: Icon::Windows,
            theme_icon: String::new(),
            active: is_box,
            action: SettingsAction::ToggleAppearance(AppearanceToggle::OverflowLayout),
            secondary_action: None,
        },
        toggle_item(
            "Animations",
            a.animations,
            "Smooth springs and transitions",
            "Instant transitions (reduced motion)",
            SettingsAction::ToggleAppearance(AppearanceToggle::Animations),
        ),
        toggle_item(
            "3D Loading",
            a.animation_3d,
            "3D tumbling loading ring",
            "2D pulsing loading ring",
            SettingsAction::ToggleAppearance(AppearanceToggle::Animation3d),
        ),
        toggle_item(
            "Translucent",
            a.translucent,
            "See-through ring material",
            "Opaque solid surfaces",
            SettingsAction::ToggleAppearance(AppearanceToggle::Translucent),
        ),
        toggle_item(
            "Blur",
            a.blur,
            "Compositor frosted glass backdrop",
            "No background blur",
            SettingsAction::ToggleAppearance(AppearanceToggle::Blur),
        ),
        toggle_item(
            "High Contrast",
            a.high_contrast,
            "Opaque high-contrast rim and surfaces",
            "Standard contrast",
            SettingsAction::ToggleAppearance(AppearanceToggle::HighContrast),
        ),
        SettingsItem {
            label: "Reset Appearance".into(),
            detail: "Restore default appearance settings".into(),
            icon: Icon::Reset,
            theme_icon: String::new(),
            active: false,
            action: SettingsAction::ResetAppearance,
            secondary_action: None,
        },
    ]
}

fn build_colors_items(c: &ColorsConfig, store: &ConfigStore) -> Vec<SettingsItem> {
    let active_name = store
        .active_theme(c)
        .map(|t| t.name)
        .unwrap_or_else(|| "Custom".into());
    vec![
        SettingsItem {
            label: "Theme".into(),
            detail: format!("Current: {active_name} · Choose a built-in or local theme"),
            icon: Icon::Palette,
            theme_icon: String::new(),
            active: false,
            action: SettingsAction::OpenView(SettingsView::ColorTheme),
            secondary_action: None,
        },
        SettingsItem {
            label: "Ring".into(),
            detail: format!("7 colors · Top {} · Rim {}", c.ring.top, c.ring.rim_top),
            icon: swatch_icon(&c.ring.top),
            theme_icon: String::new(),
            active: false,
            action: SettingsAction::OpenView(SettingsView::ColorGroup(ColorGroup::Ring)),
            secondary_action: None,
        },
        SettingsItem {
            label: "Capsule".into(),
            detail: format!(
                "3 colors · Top {} · Border {}",
                c.capsule.top, c.capsule.border
            ),
            icon: swatch_icon(&c.capsule.top),
            theme_icon: String::new(),
            active: false,
            action: SettingsAction::OpenView(SettingsView::ColorGroup(ColorGroup::Capsule)),
            secondary_action: None,
        },
        SettingsItem {
            label: "Text".into(),
            detail: format!(
                "4 colors · Primary {} · Suggestion {}",
                c.text.primary, c.text.suggestion
            ),
            icon: swatch_icon(&c.text.primary),
            theme_icon: String::new(),
            active: false,
            action: SettingsAction::OpenView(SettingsView::ColorGroup(ColorGroup::Text)),
            secondary_action: None,
        },
        SettingsItem {
            label: "Accents".into(),
            detail: format!(
                "7 colors · Files {} · Apps {}",
                c.accents.files, c.accents.apps
            ),
            icon: swatch_icon(&c.accents.files),
            theme_icon: String::new(),
            active: false,
            action: SettingsAction::OpenView(SettingsView::ColorGroup(ColorGroup::Accents)),
            secondary_action: None,
        },
        SettingsItem {
            label: "Reset All Colors".into(),
            detail: "Restore default color palette".into(),
            icon: Icon::Reset,
            theme_icon: String::new(),
            active: false,
            action: SettingsAction::ResetAllColors,
            secondary_action: None,
        },
    ]
}

fn build_color_theme_items(c: &ColorsConfig, store: &ConfigStore) -> Vec<SettingsItem> {
    let active_id = store.active_theme(c).map(|t| t.id);
    let themes = store.list_themes();
    let mut items = Vec::with_capacity(themes.len() + 1);
    for theme in themes {
        let is_active = active_id.as_deref() == Some(theme.id.as_str());
        let kind = if theme.builtin { "Built-in" } else { "Local" };
        items.push(SettingsItem {
            label: theme.name,
            detail: if is_active {
                format!("Active · {kind} theme")
            } else {
                format!("{kind} theme · Click to apply")
            },
            icon: swatch_icon(&theme.colors.ring.rim_bottom),
            theme_icon: String::new(),
            active: is_active,
            action: SettingsAction::ApplyTheme(theme.id),
            secondary_action: None,
        });
    }
    items.push(SettingsItem {
        label: "Add local theme...".into(),
        detail: "Browse folders for a theme .json or .zip to install".into(),
        icon: Icon::Add,
        theme_icon: String::new(),
        active: false,
        action: SettingsAction::OpenView(SettingsView::ThemeImport(default_import_dir())),
        secondary_action: None,
    });
    items
}

fn build_color_group_items(colors: &ColorsConfig, group: ColorGroup) -> Vec<SettingsItem> {
    let mut items: Vec<SettingsItem> = group
        .fields()
        .iter()
        .map(|&field| {
            let hex = field.get(colors);
            SettingsItem {
                label: field.short_label().into(),
                detail: format!("{hex} · Click to edit RGB color"),
                icon: swatch_icon(hex),
                theme_icon: String::new(),
                active: false,
                action: SettingsAction::PickColor(field),
                secondary_action: Some(SettingsAction::PickColor(field)),
            }
        })
        .collect();
    items.push(SettingsItem {
        label: "Reset Group".into(),
        detail: format!("Restore default {}", group.title().to_lowercase()),
        icon: Icon::Reset,
        theme_icon: String::new(),
        active: false,
        action: SettingsAction::ResetColorGroup(group),
        secondary_action: None,
    });
    items
}

fn build_keybinds_items(kb: &KeybindsConfig) -> Vec<SettingsItem> {
    let mut items = Vec::with_capacity(KeybindField::ALL.len() + 2);
    items.push(toggle_item(
        "Number Shortcuts",
        kb.number_shortcuts,
        "1–8 select visible items",
        "Number shortcuts disabled",
        SettingsAction::ToggleNumberShortcuts,
    ));
    for &field in KeybindField::ALL {
        items.push(SettingsItem {
            label: field.label().into(),
            detail: format!("{} · Click to edit", format_key_list(field.get(kb))),
            icon: Icon::Keyboard,
            theme_icon: String::new(),
            active: false,
            action: SettingsAction::OpenView(SettingsView::Keybind(field)),
            secondary_action: None,
        });
    }
    items.push(SettingsItem {
        label: "Reset Keybinds".into(),
        detail: "Restore all keybinds to defaults".into(),
        icon: Icon::Reset,
        theme_icon: String::new(),
        active: false,
        action: SettingsAction::ResetAllKeybinds,
        secondary_action: None,
    });
    items
}

fn build_keybind_detail_items(kb: &KeybindsConfig, field: KeybindField) -> Vec<SettingsItem> {
    let mut items = vec![
        SettingsItem {
            label: "Press Key".into(),
            detail: format!("Capture a new keybind for {}", field.label()),
            icon: Icon::Add,
            theme_icon: String::new(),
            active: false,
            action: SettingsAction::CaptureKeybind(field),
            secondary_action: None,
        },
        SettingsItem {
            label: "Type Key".into(),
            detail: "Enter a key name like Escape, Ctrl+u or Shift+Tab".into(),
            icon: Icon::Keyboard,
            theme_icon: String::new(),
            active: false,
            action: SettingsAction::TypeKeybind(field),
            secondary_action: None,
        },
    ];
    for (index, spec) in field.get(kb).0.iter().enumerate() {
        items.push(SettingsItem {
            label: spec.clone(),
            detail: "Click to remove this binding".into(),
            icon: Icon::Forget,
            theme_icon: String::new(),
            active: true,
            action: SettingsAction::RemoveKeybind { field, index },
            secondary_action: None,
        });
    }
    let defaults = KeybindsConfig::default();
    items.push(SettingsItem {
        label: "Reset Binding".into(),
        detail: format!("Default: {}", format_key_list(field.get(&defaults))),
        icon: Icon::Reset,
        theme_icon: String::new(),
        active: false,
        action: SettingsAction::ResetKeybind(field),
        secondary_action: None,
    });
    items
}

fn build_main_menu_items(config: &AppConfig) -> Vec<SettingsItem> {
    let mut items = Vec::with_capacity(config.main_menu.len() + 2);
    for (index, entry) in config.main_menu.iter().enumerate() {
        let icon = config::parse_builtin(&entry.name)
            .map(builtin_icon)
            .unwrap_or(Icon::CustomMenu);
        let mut detail_parts = vec![format!("#{} · {}", index + 1, entry.name)];
        if let Some(label) = entry.label.as_deref().filter(|l| !l.trim().is_empty()) {
            detail_parts.push(format!("Label: {label}"));
        }
        if let Some(ic) = entry.icon.as_deref().filter(|i| !i.trim().is_empty()) {
            detail_parts.push(format!("Icon: {ic}"));
        }
        items.push(SettingsItem {
            label: entry_display_label(entry),
            detail: detail_parts.join(" · "),
            icon,
            theme_icon: entry.icon.clone().unwrap_or_default(),
            active: false,
            action: SettingsAction::OpenView(SettingsView::MainMenuEntry(index)),
            secondary_action: None,
        });
    }
    items.push(SettingsItem {
        label: "Add Entry".into(),
        detail: "Pin a built-in or custom menu to the main ring".into(),
        icon: Icon::Add,
        theme_icon: String::new(),
        active: false,
        action: SettingsAction::OpenView(SettingsView::MainMenuAdd),
        secondary_action: None,
    });
    items.push(SettingsItem {
        label: "Reset Main Menu".into(),
        detail: "Restore default main ring entries".into(),
        icon: Icon::Reset,
        theme_icon: String::new(),
        active: false,
        action: SettingsAction::ResetMainMenu,
        secondary_action: None,
    });
    items
}

fn build_main_menu_entry_items(config: &AppConfig, index: usize) -> Vec<SettingsItem> {
    let Some(entry) = config.main_menu.get(index) else {
        return Vec::new();
    };
    let mut items = Vec::new();
    if index > 0 {
        items.push(SettingsItem {
            label: "Move Earlier".into(),
            detail: format!("Move from #{} to #{}", index + 1, index),
            icon: Icon::MoveUp,
            theme_icon: String::new(),
            active: false,
            action: SettingsAction::MoveMainMenu { index, delta: -1 },
            secondary_action: None,
        });
    }
    if index + 1 < config.main_menu.len() {
        items.push(SettingsItem {
            label: "Move Later".into(),
            detail: format!("Move from #{} to #{}", index + 1, index + 2),
            icon: Icon::MoveDown,
            theme_icon: String::new(),
            active: false,
            action: SettingsAction::MoveMainMenu { index, delta: 1 },
            secondary_action: None,
        });
    }
    let current_label = entry
        .label
        .as_deref()
        .filter(|l| !l.trim().is_empty())
        .unwrap_or("Default");
    items.push(SettingsItem {
        label: "Set Label".into(),
        detail: format!("Current: {current_label} · Empty resets to default"),
        icon: Icon::Action,
        theme_icon: String::new(),
        active: false,
        action: SettingsAction::EditMainMenuLabel(index),
        secondary_action: None,
    });
    let current_icon = entry
        .icon
        .as_deref()
        .filter(|i| !i.trim().is_empty())
        .unwrap_or("Default");
    items.push(SettingsItem {
        label: "Set Icon".into(),
        detail: format!("Current: {current_icon} · Click to type · Right-click to browse"),
        icon: Icon::Palette,
        theme_icon: entry.icon.clone().unwrap_or_default(),
        active: false,
        action: SettingsAction::EditMainMenuIcon(index),
        secondary_action: Some(SettingsAction::BrowseMainMenuIcon(index)),
    });
    items.push(SettingsItem {
        label: "Remove".into(),
        detail: format!("Remove {} from the main ring", entry.name),
        icon: Icon::Forget,
        theme_icon: String::new(),
        active: false,
        action: SettingsAction::RemoveMainMenuEntry(index),
        secondary_action: None,
    });
    items
}

fn default_import_dir() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_dir())
        .unwrap_or_else(|| PathBuf::from("/"))
}

fn import_dir_title(prefix: &str, dir: &Path) -> String {
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from)
        && dir == home
    {
        return format!("{prefix} · ~");
    }
    let leaf = dir
        .file_name()
        .and_then(|n| n.to_str())
        .filter(|n| !n.is_empty())
        .unwrap_or("/");
    format!("{prefix} · {leaf}")
}

fn build_main_menu_add_items(config: &AppConfig, store: &ConfigStore) -> Vec<SettingsItem> {
    let mut items = vec![SettingsItem {
        label: "Add Menu".into(),
        detail: "Browse folders for a menu .json or .zip to install".into(),
        icon: Icon::Add,
        theme_icon: String::new(),
        active: false,
        action: SettingsAction::OpenView(SettingsView::MainMenuImport(default_import_dir())),
        secondary_action: None,
    }];
    let builtins = [
        ("files", Mode::Files),
        ("apps", Mode::Apps),
        ("windows", Mode::Windows),
        ("wifi", Mode::Wifi),
        ("bluetooth", Mode::Bluetooth),
        ("settings", Mode::Settings),
    ];
    for (name, mode) in builtins {
        let already = config
            .main_menu
            .iter()
            .any(|e| config::parse_builtin(&e.name) == Some(mode));
        if !already {
            items.push(SettingsItem {
                label: mode.title().into(),
                detail: format!("Built-in menu ({name}) · Click to add"),
                icon: builtin_icon(mode),
                theme_icon: String::new(),
                active: false,
                action: SettingsAction::AddMainMenuEntry(name.into()),
                secondary_action: None,
            });
        }
    }
    for name in store.list_custom_menus() {
        let already = config.main_menu.iter().any(|e| e.name == name);
        if !already {
            let label = store
                .load_menu(&name)
                .ok()
                .and_then(|m| m.title)
                .filter(|t| !t.trim().is_empty())
                .unwrap_or_else(|| name.clone());
            items.push(SettingsItem {
                label,
                detail: format!("Custom menu ({name}.json) · Click to add"),
                icon: Icon::CustomMenu,
                theme_icon: String::new(),
                active: false,
                action: SettingsAction::AddMainMenuEntry(name),
                secondary_action: None,
            });
        }
    }
    items
}

#[derive(Clone, Copy)]
enum ImportKind {
    Menu,
    Theme,
}

impl ImportKind {
    fn open_dir(self, path: PathBuf) -> SettingsAction {
        match self {
            Self::Menu => SettingsAction::OpenView(SettingsView::MainMenuImport(path)),
            Self::Theme => SettingsAction::OpenView(SettingsView::ThemeImport(path)),
        }
    }

    fn import_file(self, path: PathBuf) -> SettingsAction {
        match self {
            Self::Menu => SettingsAction::ImportMenuFile(path),
            Self::Theme => SettingsAction::ImportThemeFile(path),
        }
    }

    fn file_detail(self, path: &Path, is_zip: bool) -> String {
        match (self, is_zip) {
            (Self::Menu, true) => {
                format!("Menu archive (.zip) · Click to install · {}", path.display())
            }
            (Self::Menu, false) => {
                format!("Menu JSON (.json) · Click to install · {}", path.display())
            }
            (Self::Theme, true) => {
                format!("Theme archive (.zip) · Click to install · {}", path.display())
            }
            (Self::Theme, false) => {
                format!("Theme JSON (.json) · Click to install · {}", path.display())
            }
        }
    }

    fn file_icon(self, is_zip: bool) -> Icon {
        match (self, is_zip) {
            (Self::Menu, true) => Icon::CustomMenu,
            (Self::Theme, _) => Icon::Palette,
            (Self::Menu, false) => Icon::File,
        }
    }
}

fn build_main_menu_import_items(dir: &Path) -> Vec<SettingsItem> {
    build_import_items(dir, ImportKind::Menu)
}

fn build_theme_import_items(dir: &Path) -> Vec<SettingsItem> {
    build_import_items(dir, ImportKind::Theme)
}

fn build_import_items(dir: &Path, kind: ImportKind) -> Vec<SettingsItem> {
    let mut items = Vec::new();
    if let Some(parent) = dir.parent() {
        items.push(SettingsItem {
            label: "..".into(),
            detail: format!("Parent folder · {}", parent.display()),
            icon: Icon::MoveUp,
            theme_icon: String::new(),
            active: false,
            action: kind.open_dir(parent.to_path_buf()),
            secondary_action: None,
        });
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return items;
    };
    let mut dirs: Vec<(String, PathBuf)> = Vec::new();
    let mut files: Vec<(String, PathBuf, bool)> = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if name.starts_with('.') {
            continue;
        }
        let Ok(meta) = std::fs::metadata(&path) else {
            continue;
        };
        if meta.is_dir() {
            dirs.push((name.to_owned(), path));
        } else if meta.is_file() {
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| e.to_ascii_lowercase());
            match ext.as_deref() {
                Some("json") => files.push((name.to_owned(), path, false)),
                Some("zip") => files.push((name.to_owned(), path, true)),
                _ => {}
            }
        }
    }
    dirs.sort_by_key(|(name, _)| name.to_ascii_lowercase());
    files.sort_by_key(|(name, _, _)| name.to_ascii_lowercase());

    for (name, path) in dirs {
        items.push(SettingsItem {
            detail: format!("Folder · {}", path.display()),
            label: name,
            icon: Icon::Folder,
            theme_icon: String::new(),
            active: false,
            action: kind.open_dir(path),
            secondary_action: None,
        });
    }
    for (name, path, is_zip) in files {
        items.push(SettingsItem {
            detail: kind.file_detail(&path, is_zip),
            label: name,
            icon: kind.file_icon(is_zip),
            theme_icon: String::new(),
            active: true,
            action: kind.import_file(path),
            secondary_action: None,
        });
    }
    items
}

fn build_confirm_reset_items() -> Vec<SettingsItem> {
    vec![
        SettingsItem {
            label: "Reset Everything".into(),
            detail: "Overwrite recircles.json with default settings".into(),
            icon: Icon::Forget,
            theme_icon: String::new(),
            active: false,
            action: SettingsAction::ResetAll,
            secondary_action: None,
        },
        SettingsItem {
            label: "Cancel".into(),
            detail: "Keep current settings".into(),
            icon: Icon::Disconnect,
            theme_icon: String::new(),
            active: false,
            action: SettingsAction::GoBack,
            secondary_action: None,
        },
    ]
}

/// Format a GDK key event into a ReCircles keybind string (e.g. `Ctrl+Shift+j`).
pub fn format_key_event(event: &gdk::EventKey) -> Option<String> {
    if event.is_modifier() {
        return None;
    }
    format_key_parts(event.keyval(), event.state())
}

pub fn format_key_parts(keyval: gdk::keys::Key, state: gdk::ModifierType) -> Option<String> {
    let mut mods = state
        & (gdk::ModifierType::CONTROL_MASK
            | gdk::ModifierType::SHIFT_MASK
            | gdk::ModifierType::MOD1_MASK
            | gdk::ModifierType::SUPER_MASK
            | gdk::ModifierType::META_MASK);
    let keyval = if keyval == gdk::keys::constants::ISO_Left_Tab {
        mods.insert(gdk::ModifierType::SHIFT_MASK);
        gdk::keys::constants::Tab
    } else {
        keyval.to_lower()
    };
    let raw_name = keyval.name()?;
    let key_name = raw_name.as_str();
    if key_name.is_empty() || keyval == gdk::keys::constants::VoidSymbol {
        return None;
    }
    let mut parts = Vec::new();
    if mods.contains(gdk::ModifierType::CONTROL_MASK) {
        parts.push("Ctrl");
    }
    if mods.contains(gdk::ModifierType::MOD1_MASK) {
        parts.push("Alt");
    }
    if mods.contains(gdk::ModifierType::SHIFT_MASK) {
        parts.push("Shift");
    }
    if mods.contains(gdk::ModifierType::SUPER_MASK) {
        parts.push("Super");
    }
    if mods.contains(gdk::ModifierType::META_MASK) {
        parts.push("Meta");
    }
    parts.push(key_name);
    Some(parts.join("+"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn temp_store(label: &str) -> (PathBuf, ConfigStore) {
        let dir = std::env::temp_dir().join(format!(
            "recircles-settings-{label}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        (dir.clone(), ConfigStore::from_dir(dir))
    }

    #[test]
    fn builds_all_settings_views_and_covers_all_config_fields() {
        let (dir, store) = temp_store("views");
        let cfg = AppConfig::default();

        let root = build_items(&cfg, &store, &SettingsView::Root);
        assert_eq!(root.len(), 5);

        let appearance = build_items(&cfg, &store, &SettingsView::Appearance);
        assert_eq!(appearance.len(), 7);

        let colors = build_items(&cfg, &store, &SettingsView::Colors);
        assert_eq!(colors.len(), 6);
        assert_eq!(colors[0].label, "Theme");
        assert_eq!(
            colors[0].action,
            SettingsAction::OpenView(SettingsView::ColorTheme)
        );

        let theme_view = SettingsView::ColorTheme;
        assert_eq!(theme_view.layout(), config::MenuLayout::List);
        let theme_items = build_items(&cfg, &store, &theme_view);
        assert!(theme_items.iter().any(|i| i.label == "Light Green"));
        assert_eq!(
            theme_items.last().map(|i| i.label.as_str()),
            Some("Add local theme...")
        );
        assert!(theme_items[0].active);

        // 7 ring + 3 capsule + 4 text + 7 accents = 21 color fields (+1 reset per group)
        assert_eq!(
            build_items(&cfg, &store, &SettingsView::ColorGroup(ColorGroup::Ring)).len(),
            8
        );
        assert_eq!(
            build_items(&cfg, &store, &SettingsView::ColorGroup(ColorGroup::Capsule)).len(),
            4
        );
        assert_eq!(
            build_items(&cfg, &store, &SettingsView::ColorGroup(ColorGroup::Text)).len(),
            5
        );
        assert_eq!(
            build_items(&cfg, &store, &SettingsView::ColorGroup(ColorGroup::Accents)).len(),
            8
        );

        // 1 number_shortcuts + 12 keybind fields + 1 reset = 14
        let keybinds = build_items(&cfg, &store, &SettingsView::Keybinds);
        assert_eq!(keybinds.len(), 14);

        // Activate has 2 defaults (Return, KP_Enter) + Press Key + Type Key + Reset = 5
        let activate = build_items(&cfg, &store, &SettingsView::Keybind(KeybindField::Activate));
        assert_eq!(activate.len(), 5);

        // Main menu has 6 default entries + Add Entry + Reset = 8
        let main_menu = build_items(&cfg, &store, &SettingsView::MainMenu);
        assert_eq!(main_menu.len(), 8);

        // First entry has Move Later, Set Label, Set Icon, Remove (no Move Earlier)
        let first_entry = build_items(&cfg, &store, &SettingsView::MainMenuEntry(0));
        assert_eq!(first_entry.len(), 4);

        // Middle entry has Move Earlier, Move Later, Set Label, Set Icon, Remove
        let mid_entry = build_items(&cfg, &store, &SettingsView::MainMenuEntry(1));
        assert_eq!(mid_entry.len(), 5);

        // Confirm reset has 2 items
        let confirm = build_items(&cfg, &store, &SettingsView::ConfirmResetAll);
        assert_eq!(confirm.len(), 2);

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn color_field_getters_setters_and_group_resets_work() {
        let mut colors = ColorsConfig::default();
        for group in [
            ColorGroup::Ring,
            ColorGroup::Capsule,
            ColorGroup::Text,
            ColorGroup::Accents,
        ] {
            for &field in group.fields() {
                field.set(&mut colors, "#123456".into());
                assert_eq!(field.get(&colors), "#123456");
            }
            group.reset(&mut colors);
            let defaults = ColorsConfig::default();
            for &field in group.fields() {
                assert_eq!(field.get(&colors), field.get(&defaults));
            }
        }
    }

    #[test]
    fn format_key_parts_round_trips_through_keybind_compiler() {
        let spec = format_key_parts(
            gdk::keys::constants::ISO_Left_Tab,
            gdk::ModifierType::SHIFT_MASK,
        )
        .unwrap();
        assert_eq!(spec, "Shift+Tab");

        let spec2 = format_key_parts(
            gdk::keys::constants::U,
            gdk::ModifierType::CONTROL_MASK,
        )
        .unwrap();
        assert_eq!(spec2, "Ctrl+u");

        let mut kb = KeybindsConfig::default();
        kb.previous.0.push(spec);
        kb.clear_query.0.push(spec2);
        assert!(kb.compile().is_ok());
    }

    #[test]
    fn main_menu_add_lists_unpinned_builtins_and_custom_menus() {
        let (dir, store) = temp_store("add-list");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            store.menu_path("power"),
            r#"{"title":"Power Menu","items":[]}"#,
        )
        .unwrap();
        let mut cfg = AppConfig::default();
        cfg.main_menu.retain(|e| e.name != "wifi");

        let add_items = build_items(&cfg, &store, &SettingsView::MainMenuAdd);
        let labels: Vec<&str> = add_items.iter().map(|i| i.label.as_str()).collect();
        assert_eq!(labels, vec!["Add Menu", "Wi-Fi", "Power Menu"]);

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn color_theme_dropdown_lists_builtins_local_themes_and_add_option_last() {
        let (dir, store) = temp_store("themes-dropdown");
        std::fs::create_dir_all(store.themes_dir()).unwrap();
        std::fs::write(
            store.theme_path("matcha"),
            r##"{"name":"Matcha","ring":{"top":"#dff5e4"}}"##,
        )
        .unwrap();

        let cfg = AppConfig {
            colors: store.find_theme("matcha").unwrap().colors,
            ..Default::default()
        };
        let items = build_items(&cfg, &store, &SettingsView::ColorTheme);
        let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(labels.contains(&"Light Green"));
        assert!(labels.contains(&"Matcha"));
        assert_eq!(labels.last().copied(), Some("Add local theme..."));

        let matcha = items.iter().find(|i| i.label == "Matcha").unwrap();
        assert!(matcha.active);
        assert_eq!(matcha.action, SettingsAction::ApplyTheme("matcha".into()));

        let colors_root = build_items(&cfg, &store, &SettingsView::Colors);
        assert!(colors_root[0].detail.contains("Matcha"));

        let import_items = build_items(&cfg, &store, &SettingsView::ThemeImport(store.themes_dir()));
        assert!(import_items.iter().any(|i| {
            i.label == "matcha.json"
                && i.action == SettingsAction::ImportThemeFile(store.theme_path("matcha"))
        }));

        let _ = std::fs::remove_dir_all(dir);
    }
}
