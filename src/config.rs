use crate::menu::{HomeEntry, Mode, Target};
use crate::ring::{Palette, Rgb};
use crate::themes::{self, Theme};
use gtk::gdk;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::env;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

pub const CONFIG_FILENAME: &str = "recircles.json";
pub const THEMES_DIRNAME: &str = "themes";

fn default_true() -> bool {
    true
}

fn default_none() -> Option<String> {
    None
}

fn is_false(value: &bool) -> bool {
    !*value
}

pub fn default_main_menu() -> Vec<MainMenuEntry> {
    ["files", "apps", "windows", "wifi", "bluetooth", "settings"]
        .into_iter()
        .map(|name| MainMenuEntry {
            name: name.into(),
            label: None,
            icon: None,
        })
        .collect()
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default = "default_main_menu")]
    pub main_menu: Vec<MainMenuEntry>,
    #[serde(default)]
    pub appearance: AppearanceConfig,
    #[serde(default)]
    pub colors: ColorsConfig,
    #[serde(default)]
    pub keybinds: KeybindsConfig,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            main_menu: default_main_menu(),
            appearance: AppearanceConfig::default(),
            colors: ColorsConfig::default(),
            keybinds: KeybindsConfig::default(),
        }
    }
}

impl AppConfig {
    pub fn validate(&self) -> Result<(), String> {
        self.colors.to_palette()?;
        self.keybinds.compile()?;
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppearanceConfig {
    pub overflow_layout: OverflowLayout,
    pub animations: bool,
    #[serde(
        default = "default_true",
        alias = "3d_animation",
        alias = "three_d_animation",
        alias = "loading_3d"
    )]
    pub animation_3d: bool,
    pub translucent: bool,
    pub blur: bool,
    pub high_contrast: bool,
}

impl Default for AppearanceConfig {
    fn default() -> Self {
        Self {
            overflow_layout: OverflowLayout::default(),
            animations: true,
            animation_3d: true,
            translucent: true,
            blur: true,
            high_contrast: false,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OverflowLayout {
    #[default]
    Box,
    Rings,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ColorsConfig {
    #[serde(default = "default_none", skip_serializing_if = "Option::is_none")]
    pub theme: Option<String>,
    pub ring: RingColors,
    pub capsule: CapsuleColors,
    pub text: TextColors,
    pub accents: AccentColors,
}

impl Default for ColorsConfig {
    fn default() -> Self {
        Self {
            theme: Some("default".into()),
            ring: RingColors::default(),
            capsule: CapsuleColors::default(),
            text: TextColors::default(),
            accents: AccentColors::default(),
        }
    }
}

impl ColorsConfig {
    pub fn same_palette(&self, other: &Self) -> bool {
        self.ring == other.ring
            && self.capsule == other.capsule
            && self.text == other.text
            && self.accents == other.accents
    }

    fn overlay_partial_json(&mut self, obj: &serde_json::Map<String, serde_json::Value>) {
        let apply_str = |section: &serde_json::Map<String, serde_json::Value>,
                         key: &str,
                         target: &mut String| {
            if let Some(val) = section.get(key).and_then(serde_json::Value::as_str) {
                *target = val.to_owned();
            }
        };
        if let Some(ring) = obj.get("ring").and_then(serde_json::Value::as_object) {
            apply_str(ring, "top", &mut self.ring.top);
            apply_str(ring, "mid", &mut self.ring.mid);
            apply_str(ring, "bottom", &mut self.ring.bottom);
            apply_str(ring, "rim_top", &mut self.ring.rim_top);
            apply_str(ring, "rim_bottom", &mut self.ring.rim_bottom);
            apply_str(ring, "divider", &mut self.ring.divider);
            apply_str(ring, "shadow", &mut self.ring.shadow);
        }
        if let Some(capsule) = obj.get("capsule").and_then(serde_json::Value::as_object) {
            apply_str(capsule, "top", &mut self.capsule.top);
            apply_str(capsule, "bottom", &mut self.capsule.bottom);
            apply_str(capsule, "border", &mut self.capsule.border);
        }
        if let Some(text) = obj.get("text").and_then(serde_json::Value::as_object) {
            apply_str(text, "primary", &mut self.text.primary);
            apply_str(text, "capsule", &mut self.text.capsule);
            apply_str(text, "suggestion", &mut self.text.suggestion);
            apply_str(text, "shadow", &mut self.text.shadow);
        }
        if let Some(accents) = obj.get("accents").and_then(serde_json::Value::as_object) {
            apply_str(accents, "files", &mut self.accents.files);
            apply_str(accents, "apps", &mut self.accents.apps);
            apply_str(accents, "windows", &mut self.accents.windows);
            apply_str(accents, "wifi", &mut self.accents.wifi);
            apply_str(accents, "bluetooth", &mut self.accents.bluetooth);
            apply_str(accents, "custom", &mut self.accents.custom);
            apply_str(accents, "action", &mut self.accents.action);
        }
    }

    pub fn to_palette(&self) -> Result<Palette, String> {
        Ok(Palette {
            ring_top: parse_hex_color(&self.ring.top, "colors.ring.top")?,
            ring_mid: parse_hex_color(&self.ring.mid, "colors.ring.mid")?,
            ring_bottom: parse_hex_color(&self.ring.bottom, "colors.ring.bottom")?,
            rim_top: parse_hex_color(&self.ring.rim_top, "colors.ring.rim_top")?,
            rim_bottom: parse_hex_color(&self.ring.rim_bottom, "colors.ring.rim_bottom")?,
            divider: parse_hex_color(&self.ring.divider, "colors.ring.divider")?,
            shadow: parse_hex_color(&self.ring.shadow, "colors.ring.shadow")?,
            capsule_top: parse_hex_color(&self.capsule.top, "colors.capsule.top")?,
            capsule_bottom: parse_hex_color(&self.capsule.bottom, "colors.capsule.bottom")?,
            capsule_border: parse_hex_color(&self.capsule.border, "colors.capsule.border")?,
            text_primary: parse_hex_color(&self.text.primary, "colors.text.primary")?,
            text_capsule: parse_hex_color(&self.text.capsule, "colors.text.capsule")?,
            text_suggestion: parse_hex_color(&self.text.suggestion, "colors.text.suggestion")?,
            text_shadow: parse_hex_color(&self.text.shadow, "colors.text.shadow")?,
            accent_files: parse_hex_color(&self.accents.files, "colors.accents.files")?,
            accent_apps: parse_hex_color(&self.accents.apps, "colors.accents.apps")?,
            accent_windows: parse_hex_color(&self.accents.windows, "colors.accents.windows")?,
            accent_wifi: parse_hex_color(&self.accents.wifi, "colors.accents.wifi")?,
            accent_bluetooth: parse_hex_color(
                &self.accents.bluetooth,
                "colors.accents.bluetooth",
            )?,
            accent_custom: parse_hex_color(&self.accents.custom, "colors.accents.custom")?,
            accent_action: parse_hex_color(&self.accents.action, "colors.accents.action")?,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct RingColors {
    pub top: String,
    pub mid: String,
    pub bottom: String,
    pub rim_top: String,
    pub rim_bottom: String,
    pub divider: String,
    pub shadow: String,
}

impl Default for RingColors {
    fn default() -> Self {
        Self {
            top: "#422614".into(),
            mid: "#26160d".into(),
            bottom: "#2e1a0f".into(),
            rim_top: "#ffd19e".into(),
            rim_bottom: "#f59447".into(),
            divider: "#ffd6ad".into(),
            shadow: "#030100".into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct CapsuleColors {
    pub top: String,
    pub bottom: String,
    pub border: String,
}

impl Default for CapsuleColors {
    fn default() -> Self {
        Self {
            top: "#362114".into(),
            bottom: "#21130d".into(),
            border: "#ffd1a8".into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct TextColors {
    pub primary: String,
    pub capsule: String,
    pub suggestion: String,
    pub shadow: String,
}

impl Default for TextColors {
    fn default() -> Self {
        Self {
            primary: "#fff7f0".into(),
            capsule: "#fff5eb".into(),
            suggestion: "#d7b9a4".into(),
            shadow: "#090402".into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AccentColors {
    pub files: String,
    pub apps: String,
    pub windows: String,
    pub wifi: String,
    pub bluetooth: String,
    pub custom: String,
    pub action: String,
}

impl Default for AccentColors {
    fn default() -> Self {
        Self {
            files: "#ff9e47".into(),
            apps: "#ffc75c".into(),
            windows: "#ff7a57".into(),
            wifi: "#5cd6b3".into(),
            bluetooth: "#6ba8ff".into(),
            custom: "#ffb86b".into(),
            action: "#fa8f5c".into(),
        }
    }
}

pub fn parse_hex_rgb_u8(value: &str) -> Option<(u8, u8, u8)> {
    let hex = value.trim().strip_prefix('#').unwrap_or(value.trim());
    let (r, g, b) = match hex.len() {
        6 => (
            u8::from_str_radix(&hex[0..2], 16).ok()?,
            u8::from_str_radix(&hex[2..4], 16).ok()?,
            u8::from_str_radix(&hex[4..6], 16).ok()?,
        ),
        3 => {
            let expand = |s: &str| u8::from_str_radix(s, 16).ok().map(|n| n * 17);
            (expand(&hex[0..1])?, expand(&hex[1..2])?, expand(&hex[2..3])?)
        }
        _ => return None,
    };
    Some((r, g, b))
}

pub fn format_hex_rgb(r: u8, g: u8, b: u8) -> String {
    format!("#{r:02x}{g:02x}{b:02x}")
}

pub fn parse_hex_color(value: &str, field: &str) -> Result<Rgb, String> {
    let Some((r, g, b)) = parse_hex_rgb_u8(value) else {
        return Err(format!(
            "Invalid color {value:?} for {field}: expected #RRGGBB or #RGB."
        ));
    };
    Ok((f64::from(r) / 255.0, f64::from(g) / 255.0, f64::from(b) / 255.0))
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct KeyList(pub Vec<String>);

impl KeyList {
    fn of(keys: &[&str]) -> Self {
        Self(keys.iter().map(|&k| k.to_owned()).collect())
    }
}

impl<'de> Deserialize<'de> for KeyList {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum OneOrMany {
            One(String),
            Many(Vec<String>),
        }
        match OneOrMany::deserialize(deserializer)? {
            OneOrMany::One(s) => Ok(Self(vec![s])),
            OneOrMany::Many(v) => Ok(Self(v)),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct KeybindsConfig {
    pub back: KeyList,
    pub home: KeyList,
    pub activate: KeyList,
    pub secondary_activate: KeyList,
    pub open_current_folder: KeyList,
    pub next: KeyList,
    pub previous: KeyList,
    pub page_next: KeyList,
    pub page_previous: KeyList,
    pub accept_completion: KeyList,
    pub undo_completion: KeyList,
    pub clear_query: KeyList,
    #[serde(default = "default_true")]
    pub number_shortcuts: bool,
}

impl Default for KeybindsConfig {
    fn default() -> Self {
        Self {
            back: KeyList::of(&["Escape"]),
            home: KeyList::of(&["Shift+Escape"]),
            activate: KeyList::of(&["Return", "KP_Enter"]),
            secondary_activate: KeyList::of(&["Shift+Return", "Shift+KP_Enter"]),
            open_current_folder: KeyList::of(&["Alt+Return", "Alt+KP_Enter"]),
            next: KeyList::of(&["Down", "Tab"]),
            previous: KeyList::of(&["Up", "Shift+Tab"]),
            page_next: KeyList::of(&["Page_Down"]),
            page_previous: KeyList::of(&["Page_Up"]),
            accept_completion: KeyList::of(&["Right", "KP_Right"]),
            undo_completion: KeyList::of(&["Left", "KP_Left"]),
            clear_query: KeyList::of(&["Ctrl+u"]),
            number_shortcuts: true,
        }
    }
}

impl KeybindsConfig {
    pub fn compile(&self) -> Result<CompiledKeybinds, String> {
        Ok(CompiledKeybinds {
            back: compile_key_list(&self.back.0, "keybinds.back")?,
            home: compile_key_list(&self.home.0, "keybinds.home")?,
            activate: compile_key_list(&self.activate.0, "keybinds.activate")?,
            secondary_activate: compile_key_list(
                &self.secondary_activate.0,
                "keybinds.secondary_activate",
            )?,
            open_current_folder: compile_key_list(
                &self.open_current_folder.0,
                "keybinds.open_current_folder",
            )?,
            next: compile_key_list(&self.next.0, "keybinds.next")?,
            previous: compile_key_list(&self.previous.0, "keybinds.previous")?,
            page_next: compile_key_list(&self.page_next.0, "keybinds.page_next")?,
            page_previous: compile_key_list(&self.page_previous.0, "keybinds.page_previous")?,
            accept_completion: compile_key_list(
                &self.accept_completion.0,
                "keybinds.accept_completion",
            )?,
            undo_completion: compile_key_list(
                &self.undo_completion.0,
                "keybinds.undo_completion",
            )?,
            clear_query: compile_key_list(&self.clear_query.0, "keybinds.clear_query")?,
            number_shortcuts: self.number_shortcuts,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeyCombo {
    keyval: gdk::keys::Key,
    mods: gdk::ModifierType,
}

impl KeyCombo {
    pub fn matches(&self, event: &gdk::EventKey) -> bool {
        self.matches_parts(event.keyval(), event.state())
    }

    pub fn matches_parts(&self, keyval: gdk::keys::Key, state: gdk::ModifierType) -> bool {
        let relevant = gdk::ModifierType::CONTROL_MASK
            | gdk::ModifierType::SHIFT_MASK
            | gdk::ModifierType::MOD1_MASK
            | gdk::ModifierType::SUPER_MASK
            | gdk::ModifierType::META_MASK;
        let mut mods = state & relevant;
        let normalized = if keyval == gdk::keys::constants::ISO_Left_Tab {
            mods.insert(gdk::ModifierType::SHIFT_MASK);
            gdk::keys::constants::Tab
        } else {
            keyval.to_lower()
        };
        normalized == self.keyval && mods == self.mods
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyBinding(Vec<KeyCombo>);

impl KeyBinding {
    pub fn matches(&self, event: &gdk::EventKey) -> bool {
        self.0.iter().any(|combo| combo.matches(event))
    }

    #[cfg(test)]
    pub fn matches_parts(&self, keyval: gdk::keys::Key, state: gdk::ModifierType) -> bool {
        self.0
            .iter()
            .any(|combo| combo.matches_parts(keyval, state))
    }
}

#[derive(Clone, Debug)]
pub struct CompiledKeybinds {
    pub back: KeyBinding,
    pub home: KeyBinding,
    pub activate: KeyBinding,
    pub secondary_activate: KeyBinding,
    pub open_current_folder: KeyBinding,
    pub next: KeyBinding,
    pub previous: KeyBinding,
    pub page_next: KeyBinding,
    pub page_previous: KeyBinding,
    pub accept_completion: KeyBinding,
    pub undo_completion: KeyBinding,
    pub clear_query: KeyBinding,
    pub number_shortcuts: bool,
}

fn compile_key_list(list: &[String], field: &str) -> Result<KeyBinding, String> {
    list.iter()
        .map(|spec| parse_key_combo(spec, field))
        .collect::<Result<Vec<_>, _>>()
        .map(KeyBinding)
}

fn parse_key_combo(spec: &str, field: &str) -> Result<KeyCombo, String> {
    let trimmed = spec.trim();
    if trimmed.is_empty() {
        return Err(format!("Empty keybind entry in {field}."));
    }
    let parts: Vec<&str> = if trimmed == "+" {
        vec!["plus"]
    } else {
        trimmed.split('+').map(str::trim).collect()
    };
    if parts.iter().any(|p| p.is_empty()) {
        return Err(format!("Invalid keybind {spec:?} in {field}."));
    }
    let (key_part, mod_parts) = parts.split_last().unwrap();
    let mut mods = gdk::ModifierType::empty();
    for &modifier in mod_parts {
        match modifier.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => mods.insert(gdk::ModifierType::CONTROL_MASK),
            "shift" => mods.insert(gdk::ModifierType::SHIFT_MASK),
            "alt" | "mod1" => mods.insert(gdk::ModifierType::MOD1_MASK),
            "super" | "win" | "mod4" => mods.insert(gdk::ModifierType::SUPER_MASK),
            "meta" => mods.insert(gdk::ModifierType::META_MASK),
            _ => {
                return Err(format!(
                    "Invalid keybind {spec:?} in {field}: unknown modifier {modifier:?}."
                ));
            }
        }
    }
    let canonical = match key_part.to_ascii_lowercase().as_str() {
        "esc" | "escape" => "Escape",
        "enter" | "return" => "Return",
        "kp_enter" => "KP_Enter",
        "tab" => "Tab",
        "iso_left_tab" => "ISO_Left_Tab",
        "up" => "Up",
        "down" => "Down",
        "left" => "Left",
        "right" => "Right",
        "kp_up" => "KP_Up",
        "kp_down" => "KP_Down",
        "kp_left" => "KP_Left",
        "kp_right" => "KP_Right",
        "pgup" | "pageup" | "page_up" => "Page_Up",
        "pgdn" | "pgdown" | "pagedown" | "page_down" => "Page_Down",
        "space" => "space",
        "backspace" => "BackSpace",
        "del" | "delete" => "Delete",
        _ => key_part,
    };
    let keyval = gdk::keys::Key::from_name(canonical);
    if keyval == gdk::keys::constants::VoidSymbol {
        return Err(format!(
            "Invalid keybind {spec:?} in {field}: unknown key {key_part:?}."
        ));
    }
    let normalized = if keyval == gdk::keys::constants::ISO_Left_Tab {
        mods.insert(gdk::ModifierType::SHIFT_MASK);
        gdk::keys::constants::Tab
    } else {
        keyval.to_lower()
    };
    Ok(KeyCombo {
        keyval: normalized,
        mods,
    })
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MainMenuEntry {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MenuLayout {
    #[default]
    #[serde(alias = "default", alias = "auto")]
    Ring,
    #[serde(
        alias = "dropdown",
        alias = "pick",
        alias = "pick_and_choose",
        alias = "choose",
        alias = "select"
    )]
    List,
    #[serde(alias = "string", alias = "text", alias = "prompt")]
    Input,
}

impl MenuLayout {
    pub fn is_ring(&self) -> bool {
        matches!(self, Self::Ring)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct InputPromptConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placeholder: Option<String>,
    #[serde(default, alias = "initial", skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub password: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub allow_empty: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum InputSpec {
    Flag(bool),
    Prompt(String),
    Config(InputPromptConfig),
}

impl InputSpec {
    pub fn is_enabled(&self) -> bool {
        !matches!(self, Self::Flag(false))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MenuConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(
        default,
        alias = "type",
        alias = "kind",
        skip_serializing_if = "MenuLayout::is_ring"
    )]
    pub layout: MenuLayout,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_open: Option<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub loading_animation: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placeholder: Option<String>,
    #[serde(default, alias = "initial", skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub password: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub allow_empty: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(default, alias = "update", skip_serializing_if = "is_false")]
    pub reload: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub submenu: Option<String>,
    #[serde(default, alias = "choices", skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<String>,
    #[serde(default)]
    pub items: Vec<MenuItem>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub submenus: BTreeMap<String, SubmenuConfig>,
}

impl MenuConfig {
    pub fn display_title<'a>(&'a self, fallback: &'a str) -> &'a str {
        self.title
            .as_deref()
            .filter(|t| !t.trim().is_empty())
            .unwrap_or(fallback)
    }

    pub fn resolved_items(&self) -> Vec<MenuItem> {
        resolve_section_items(
            &self.items,
            &self.options,
            self.command.as_deref(),
            self.submenu.as_deref(),
            self.reload,
            None,
        )
    }

    pub fn input_prompt(&self, fallback_title: &str) -> InputPromptConfig {
        InputPromptConfig {
            title: Some(self.display_title(fallback_title).to_owned()),
            prompt: self.prompt.clone(),
            placeholder: self.placeholder.clone(),
            default: self.default.clone(),
            password: self.password,
            allow_empty: self.allow_empty,
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if let Some(cmd) = &self.on_open
            && cmd.trim().is_empty()
        {
            return Err("Menu on_open command cannot be empty.".into());
        }
        self.validate_section(
            "Menu",
            self.layout,
            self.command.as_deref(),
            self.submenu.as_deref(),
            self.reload,
            &self.options,
            &self.items,
        )?;
        for (sub_name, sub) in &self.submenus {
            if sub_name.trim().is_empty() {
                return Err("Submenu name cannot be empty.".into());
            }
            if let Some(cmd) = &sub.on_open
                && cmd.trim().is_empty()
            {
                return Err(format!(
                    "Submenu {sub_name:?} has an empty on_open command."
                ));
            }
            self.validate_section(
                &format!("Submenu {sub_name:?}"),
                sub.layout,
                sub.command.as_deref(),
                sub.submenu.as_deref(),
                sub.reload,
                &sub.options,
                &sub.items,
            )?;
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn validate_section(
        &self,
        scope: &str,
        layout: MenuLayout,
        command: Option<&str>,
        submenu: Option<&str>,
        reload: bool,
        options: &[String],
        items: &[MenuItem],
    ) -> Result<(), String> {
        if let Some(cmd) = command
            && cmd.trim().is_empty()
        {
            return Err(format!("{scope} has an empty command."));
        }
        if let Some(target) = submenu
            && !self.submenus.contains_key(target)
        {
            return Err(format!("{scope} references unknown submenu {target:?}."));
        }
        let has_section_action = command.is_some() || submenu.is_some() || reload;
        if layout == MenuLayout::Input && !has_section_action {
            return Err(format!(
                "{scope} with input layout requires a command, submenu, or reload action."
            ));
        }
        if options.iter().any(|opt| opt.trim().is_empty()) {
            return Err(format!("{scope} contains an empty option label."));
        }
        if !options.is_empty() && !has_section_action {
            return Err(format!(
                "{scope} with options requires a command, submenu, or reload action."
            ));
        }
        self.validate_items(items)
    }

    fn validate_items(&self, items: &[MenuItem]) -> Result<(), String> {
        for item in items {
            if item.label.trim().is_empty() {
                return Err("Menu item label cannot be empty.".into());
            }
            for (click, action) in [
                ("left_click", &item.left_click),
                ("right_click", &item.right_click),
            ] {
                let Some(action) = action else {
                    continue;
                };
                if action.as_command().is_none()
                    && action.as_submenu().is_none()
                    && !action.reloads()
                {
                    return Err(format!(
                        "Item {:?} has an empty {click} action.",
                        item.label
                    ));
                }
                if let Some(cmd) = action.as_command()
                    && cmd.trim().is_empty()
                {
                    return Err(format!(
                        "Item {:?} has an empty {click} command.",
                        item.label
                    ));
                }
                if let Some(target) = action.as_submenu()
                    && !self.submenus.contains_key(target)
                {
                    return Err(format!(
                        "Item {:?} references unknown submenu {:?} in {click}.",
                        item.label, target
                    ));
                }
                if action.options().iter().any(|opt| opt.trim().is_empty()) {
                    return Err(format!(
                        "Item {:?} has an empty option in {click}.",
                        item.label
                    ));
                }
            }
        }
        Ok(())
    }
}

fn resolve_section_items(
    items: &[MenuItem],
    options: &[String],
    command: Option<&str>,
    submenu: Option<&str>,
    reload: bool,
    loading_animation: Option<bool>,
) -> Vec<MenuItem> {
    let fallback = (command.is_some() || submenu.is_some() || reload).then(|| {
        Action::Object(ActionObject {
            command: command.map(str::to_owned),
            submenu: submenu.map(str::to_owned),
            reload,
            loading_animation,
            ..ActionObject::default()
        })
    });
    let mut out = Vec::with_capacity(items.len() + options.len());
    for item in items {
        let mut cloned = item.clone();
        if cloned.left_click.is_none() && cloned.right_click.is_none() {
            cloned.left_click = fallback.clone();
        }
        out.push(cloned);
    }
    for opt in options {
        out.push(MenuItem {
            label: opt.clone(),
            icon: String::new(),
            left_click: fallback.clone(),
            right_click: None,
        });
    }
    out
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubmenuConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(
        default,
        alias = "type",
        alias = "kind",
        skip_serializing_if = "MenuLayout::is_ring"
    )]
    pub layout: MenuLayout,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_open: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub loading_animation: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placeholder: Option<String>,
    #[serde(default, alias = "initial", skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub password: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub allow_empty: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(default, alias = "update", skip_serializing_if = "is_false")]
    pub reload: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub submenu: Option<String>,
    #[serde(default, alias = "choices", skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<String>,
    #[serde(default)]
    pub items: Vec<MenuItem>,
}

impl SubmenuConfig {
    pub fn display_title<'a>(&'a self, fallback: &'a str) -> &'a str {
        self.title
            .as_deref()
            .filter(|t| !t.trim().is_empty())
            .unwrap_or(fallback)
    }

    pub fn resolved_items(&self) -> Vec<MenuItem> {
        resolve_section_items(
            &self.items,
            &self.options,
            self.command.as_deref(),
            self.submenu.as_deref(),
            self.reload,
            self.loading_animation,
        )
    }

    pub fn input_prompt(&self, fallback_title: &str) -> InputPromptConfig {
        InputPromptConfig {
            title: Some(self.display_title(fallback_title).to_owned()),
            prompt: self.prompt.clone(),
            placeholder: self.placeholder.clone(),
            default: self.default.clone(),
            password: self.password,
            allow_empty: self.allow_empty,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MenuItem {
    #[serde(alias = "name")]
    pub label: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub icon: String,
    #[serde(default, alias = "left", skip_serializing_if = "Option::is_none")]
    pub left_click: Option<Action>,
    #[serde(default, alias = "right", skip_serializing_if = "Option::is_none")]
    pub right_click: Option<Action>,
}

impl MenuItem {
    pub fn action(&self, secondary: bool) -> Option<&Action> {
        if secondary {
            self.right_click.as_ref()
        } else {
            self.left_click.as_ref()
        }
    }

    pub fn opens_submenu(&self) -> bool {
        self.left_click.as_ref().is_some_and(|action| {
            action.as_submenu().is_some()
                || action.opens_input()
                || !action.options().is_empty()
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Action {
    Command(String),
    Object(ActionObject),
}

impl Action {
    pub fn as_command(&self) -> Option<&str> {
        match self {
            Self::Command(cmd) => Some(cmd),
            Self::Object(obj) => obj.command.as_deref(),
        }
    }

    pub fn as_submenu(&self) -> Option<&str> {
        match self {
            Self::Command(_) => None,
            Self::Object(obj) => obj.submenu.as_deref(),
        }
    }

    pub fn reloads(&self) -> bool {
        match self {
            Self::Command(_) => false,
            Self::Object(obj) => obj.reload,
        }
    }

    pub fn loading_animation(&self) -> Option<bool> {
        match self {
            Self::Command(_) => None,
            Self::Object(obj) => obj.loading_animation,
        }
    }

    pub fn opens_input(&self) -> bool {
        match self {
            Self::Command(_) => false,
            Self::Object(obj) => obj.input.as_ref().is_some_and(InputSpec::is_enabled),
        }
    }

    pub fn input_prompt(&self, fallback_title: &str) -> Option<InputPromptConfig> {
        let Self::Object(obj) = self else {
            return None;
        };
        let spec = obj.input.as_ref().filter(|s| s.is_enabled())?;
        let base = match spec {
            InputSpec::Flag(_) => InputPromptConfig::default(),
            InputSpec::Prompt(prompt) => InputPromptConfig {
                prompt: Some(prompt.clone()),
                ..InputPromptConfig::default()
            },
            InputSpec::Config(cfg) => cfg.clone(),
        };
        Some(InputPromptConfig {
            title: base
                .title
                .or_else(|| obj.title.clone())
                .filter(|t| !t.trim().is_empty())
                .or_else(|| Some(fallback_title.to_owned())),
            prompt: base.prompt.or_else(|| obj.prompt.clone()),
            placeholder: base.placeholder.or_else(|| obj.placeholder.clone()),
            default: base.default.or_else(|| obj.default.clone()),
            password: base.password || obj.password,
            allow_empty: base.allow_empty || obj.allow_empty,
        })
    }

    pub fn options(&self) -> &[String] {
        match self {
            Self::Command(_) => &[],
            Self::Object(obj) => &obj.options,
        }
    }

    pub fn inline_list_submenu(&self, fallback_title: &str) -> Option<SubmenuConfig> {
        let Self::Object(obj) = self else {
            return None;
        };
        if obj.options.is_empty() {
            return None;
        }
        Some(SubmenuConfig {
            title: obj
                .title
                .clone()
                .filter(|t| !t.trim().is_empty())
                .or_else(|| Some(fallback_title.to_owned())),
            layout: MenuLayout::List,
            on_open: None,
            loading_animation: obj.loading_animation,
            prompt: obj.prompt.clone(),
            placeholder: None,
            default: None,
            password: false,
            allow_empty: false,
            command: obj.command.clone(),
            reload: obj.reload,
            submenu: obj.submenu.clone(),
            options: obj.options.clone(),
            items: Vec::new(),
        })
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionObject {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub submenu: Option<String>,
    #[serde(default, alias = "update", skip_serializing_if = "is_false")]
    pub reload: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub loading_animation: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<InputSpec>,
    #[serde(default, alias = "choices", skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placeholder: Option<String>,
    #[serde(default, alias = "initial", skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub password: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub allow_empty: bool,
}

#[derive(Clone, Debug)]
pub enum StartTarget {
    Builtin(Mode),
    Custom { name: String, config: MenuConfig },
}

#[derive(Clone, Debug)]
pub struct ConfigStore {
    dir: PathBuf,
}

impl ConfigStore {
    pub fn open() -> Result<Self, String> {
        let base = env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
            .or_else(|| env::var_os("HOME").map(|p| PathBuf::from(p).join(".config")))
            .ok_or_else(|| "Set HOME or XDG_CONFIG_HOME for ReCircles configuration.".to_string())?;
        Ok(Self {
            dir: base.join("recircles"),
        })
    }

    #[cfg(test)]
    pub fn from_dir(dir: PathBuf) -> Self {
        Self { dir }
    }

    pub fn config_path(&self) -> PathBuf {
        self.dir.join(CONFIG_FILENAME)
    }

    pub fn menu_path(&self, name: &str) -> PathBuf {
        self.dir.join(format!("{name}.json"))
    }

    pub fn themes_dir(&self) -> PathBuf {
        self.dir.join(THEMES_DIRNAME)
    }

    pub fn theme_path(&self, id: &str) -> PathBuf {
        self.themes_dir().join(format!("{id}.json"))
    }

    pub fn generate_default_config(&self, force: bool) -> Result<PathBuf, String> {
        let path = self.config_path();
        if path.exists() && !force {
            return Err(format!(
                "Config already exists at {}. Use --force or -f to overwrite it.",
                path.display()
            ));
        }
        self.save_config(&AppConfig::default())?;
        let _ = fs::create_dir_all(self.themes_dir());
        Ok(path)
    }

    pub fn load_config(&self) -> Result<AppConfig, String> {
        let path = self.config_path();
        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(err) if err.kind() == io::ErrorKind::NotFound => {
                let default = AppConfig::default();
                self.save_config(&default)?;
                let _ = fs::create_dir_all(self.themes_dir());
                return Ok(default);
            }
            Err(err) => {
                return Err(format!("Could not read {}: {err}", path.display()));
            }
        };

        let _ = fs::create_dir_all(self.themes_dir());

        let raw_value: serde_json::Value = serde_json::from_str(&text)
            .map_err(|err| format!("Invalid config in {}: {err}", path.display()))?;
        let mut config: AppConfig = serde_json::from_value(raw_value.clone())
            .map_err(|err| format!("Invalid config in {}: {err}", path.display()))?;

        // Accept either colors.theme (canonical) or a top-level "theme" key as fallback.
        if config.colors.theme.is_none()
            && let Some(top_theme) = raw_value.get("theme").and_then(serde_json::Value::as_str)
            && !top_theme.trim().is_empty()
        {
            config.colors.theme = Some(top_theme.trim().to_owned());
        }

        if let Some(theme_name) = config.colors.theme.clone() {
            let trimmed = theme_name.trim();
            if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("custom") {
                config.colors.theme = None;
            } else if let Some(theme) = self.find_theme(trimmed) {
                let mut resolved = theme.colors;
                resolved.theme = Some(theme.id);
                if let Some(raw_colors) =
                    raw_value.get("colors").and_then(serde_json::Value::as_object)
                    && !is_full_color_dump(raw_colors)
                {
                    resolved.overlay_partial_json(raw_colors);
                }
                config.colors = resolved;
            }
        }

        config.validate()?;

        let before = config.main_menu.len();
        config.main_menu.retain(|entry| {
            parse_builtin(&entry.name).is_some()
                || (validate_custom_name(&entry.name).is_ok()
                    && self.menu_path(&entry.name).is_file())
        });
        if config.main_menu.len() != before {
            self.save_config(&config)?;
        }
        Ok(config)
    }

    pub fn save_config(&self, config: &AppConfig) -> Result<(), String> {
        let mut json = serde_json::to_string_pretty(config)
            .map_err(|err| format!("Could not serialize config: {err}"))?;
        json.push('\n');
        write_atomic(&self.dir, &self.config_path(), json.as_bytes())
    }

    pub fn list_custom_menus(&self) -> Vec<String> {
        let Ok(entries) = fs::read_dir(&self.dir) else {
            return Vec::new();
        };
        let mut names = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let Some(file_name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if file_name == CONFIG_FILENAME {
                continue;
            }
            let Some(stem) = file_name.strip_suffix(".json") else {
                continue;
            };
            if validate_custom_name(stem).is_ok() {
                names.push(stem.to_owned());
            }
        }
        names.sort();
        names
    }

    pub fn load_menu(&self, name: &str) -> Result<MenuConfig, String> {
        validate_custom_name(name)?;
        let path = self.menu_path(name);
        let text = fs::read_to_string(&path).map_err(|err| {
            if err.kind() == io::ErrorKind::NotFound {
                format!("Unknown menu {name:?} (expected {}).", path.display())
            } else {
                format!("Could not read menu {name:?} at {}: {err}", path.display())
            }
        })?;
        let menu: MenuConfig = serde_json::from_str(&text)
            .map_err(|err| format!("Invalid menu JSON in {}: {err}", path.display()))?;
        menu.validate()?;
        Ok(menu)
    }

    pub fn add_menu(
        &self,
        name: &str,
        source_path: Option<&Path>,
        main_menu_icon: Option<&str>,
    ) -> Result<(), String> {
        if let Some(icon) = main_menu_icon
            && icon.trim().is_empty()
        {
            return Err("Icon for --main-menu cannot be empty.".into());
        }

        if let Some(source) = source_path {
            validate_custom_name(name)?;
            let raw = fs::read_to_string(source)
                .map_err(|err| format!("Could not read {}: {err}", source.display()))?;
            let parsed: MenuConfig = serde_json::from_str(&raw)
                .map_err(|err| format!("Invalid menu JSON in {}: {err}", source.display()))?;
            parsed.validate()?;
            write_atomic(&self.dir, &self.menu_path(name), raw.as_bytes())?;
        } else {
            let Some(_) = main_menu_icon else {
                return Err(format!(
                    "Missing config file for menu {name:?}. Usage: recircles add <name> <config.json> [--main-menu <icon>]"
                ));
            };
            if parse_builtin(name).is_none() {
                validate_custom_name(name)?;
                if !self.menu_path(name).is_file() {
                    return Err(format!(
                        "Menu {name:?} does not exist yet. Provide a config file: recircles add {name} <config.json> --main-menu <icon>"
                    ));
                }
                self.load_menu(name)?;
            }
        }

        let mut config = self.load_config()?;
        if let Some(icon) = main_menu_icon {
            let entry_name = parse_builtin(name)
                .and_then(builtin_name)
                .unwrap_or(name)
                .to_owned();
            if let Some(existing) = config.main_menu.iter_mut().find(|entry| {
                if let Some(mode) = parse_builtin(&entry_name) {
                    parse_builtin(&entry.name) == Some(mode)
                } else {
                    entry.name == entry_name
                }
            }) {
                existing.icon = Some(icon.to_owned());
            } else {
                config.main_menu.push(MainMenuEntry {
                    name: entry_name,
                    label: None,
                    icon: Some(icon.to_owned()),
                });
            }
            self.save_config(&config)?;
        }
        Ok(())
    }

    pub fn remove_menu(&self, name: &str, main_menu_only: bool) -> Result<(), String> {
        let mut config = self.load_config()?;

        if let Some(mode) = parse_builtin(name) {
            if !main_menu_only {
                return Err(format!(
                    "Cannot delete built-in menu {name:?} from known menus. Use --main-menu to remove it from the main menu."
                ));
            }
            let before = config.main_menu.len();
            config
                .main_menu
                .retain(|entry| parse_builtin(&entry.name) != Some(mode));
            if config.main_menu.len() == before {
                return Err(format!("Menu {name:?} is not in the main menu."));
            }
            self.save_config(&config)?;
            return Ok(());
        }

        validate_custom_name(name)?;

        if main_menu_only {
            let before = config.main_menu.len();
            config.main_menu.retain(|entry| entry.name != name);
            if config.main_menu.len() == before {
                return Err(format!("Menu {name:?} is not in the main menu."));
            }
            self.save_config(&config)?;
            return Ok(());
        }

        let path = self.menu_path(name);
        let existed_file = match fs::remove_file(&path) {
            Ok(()) => true,
            Err(err) if err.kind() == io::ErrorKind::NotFound => false,
            Err(err) => {
                return Err(format!("Could not remove {}: {err}", path.display()));
            }
        };

        let before = config.main_menu.len();
        config.main_menu.retain(|entry| entry.name != name);
        let existed_main = config.main_menu.len() != before;
        if existed_main {
            self.save_config(&config)?;
        }

        if !existed_file && !existed_main {
            return Err(format!("Unknown menu {name:?}."));
        }
        Ok(())
    }

    pub fn resolve_start_target(&self, name: &str) -> Result<StartTarget, String> {
        if let Some(mode) = parse_builtin(name) {
            return Ok(StartTarget::Builtin(mode));
        }
        let config = self.load_menu(name)?;
        Ok(StartTarget::Custom {
            name: name.to_owned(),
            config,
        })
    }

    pub fn home_targets(&self, config: &AppConfig) -> Vec<Target> {
        config
            .main_menu
            .iter()
            .map(|entry| {
                if let Some(mode) = parse_builtin(&entry.name) {
                    if entry.label.is_none() && entry.icon.is_none() {
                        Target::Mode(mode)
                    } else {
                        Target::HomeEntry(HomeEntry {
                            name: entry.name.clone(),
                            label: entry
                                .label
                                .clone()
                                .filter(|l| !l.trim().is_empty())
                                .unwrap_or_else(|| mode.title().into()),
                            icon: entry.icon.clone().unwrap_or_default(),
                            builtin: Some(mode),
                        })
                    }
                } else {
                    let label = entry
                        .label
                        .clone()
                        .filter(|l| !l.trim().is_empty())
                        .or_else(|| {
                            self.load_menu(&entry.name)
                                .ok()
                                .and_then(|menu| menu.title)
                                .filter(|t| !t.trim().is_empty())
                        })
                        .unwrap_or_else(|| entry.name.clone());
                    Target::HomeEntry(HomeEntry {
                        name: entry.name.clone(),
                        label,
                        icon: entry.icon.clone().unwrap_or_default(),
                        builtin: None,
                    })
                }
            })
            .collect()
    }

    /// Import a custom menu from a standalone `.json` file or a `.zip` bundle,
    /// copying companion assets (icons, scripts) into the config directory and
    /// pinning the installed menu(s) to `main_menu`.
    pub fn import_menu_file(&self, path: &Path) -> Result<Vec<String>, String> {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase())
            .unwrap_or_default();
        match ext.as_str() {
            "json" => self.import_menu_json(path),
            "zip" => self.import_menu_zip(path),
            _ => Err("Select a .json or .zip menu file.".into()),
        }
    }

    fn import_menu_json(&self, path: &Path) -> Result<Vec<String>, String> {
        let raw = fs::read_to_string(path)
            .map_err(|err| format!("Could not read {}: {err}", path.display()))?;
        let parsed: MenuConfig = serde_json::from_str(&raw)
            .map_err(|err| format!("Invalid menu JSON in {}: {err}", path.display()))?;
        parsed.validate()?;

        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("custom");
        let name = sanitize_imported_name(stem);
        write_atomic(&self.dir, &self.menu_path(&name), raw.as_bytes())?;

        let mut config = self.load_config()?;
        pin_menu_entry(&mut config, &name, None);
        self.save_config(&config)?;
        Ok(vec![name])
    }

    fn import_menu_zip(&self, path: &Path) -> Result<Vec<String>, String> {
        static ZIP_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let n = ZIP_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let tmp_dir = env::temp_dir().join(format!(
            "recircles-import-{}-{n}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&tmp_dir);
        fs::create_dir_all(&tmp_dir)
            .map_err(|err| format!("Could not create temp dir: {err}"))?;

        let result = (|| -> Result<Vec<String>, String> {
            extract_zip_to(path, &tmp_dir)?;
            let root = unwrap_single_dir(&tmp_dir);
            let mut all_files = Vec::new();
            collect_extracted_files(&root, &root, &mut all_files)?;

            let mut menu_files: Vec<(PathBuf, String, String)> = Vec::new();
            let mut other_files: Vec<(PathBuf, PathBuf)> = Vec::new();
            for (rel, full) in all_files {
                let is_json = full
                    .extension()
                    .and_then(|e| e.to_str())
                    .is_some_and(|e| e.eq_ignore_ascii_case("json"));
                let is_main_cfg = rel
                    .file_name()
                    .and_then(|f| f.to_str())
                    .is_some_and(|f| f.eq_ignore_ascii_case(CONFIG_FILENAME));
                if is_json
                    && !is_main_cfg
                    && let Ok(raw) = fs::read_to_string(&full)
                    && let Ok(parsed) = serde_json::from_str::<MenuConfig>(&raw)
                    && parsed.validate().is_ok()
                {
                    let stem = full
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("custom");
                    let name = sanitize_imported_name(stem);
                    menu_files.push((full, name, raw));
                } else if !is_main_cfg {
                    other_files.push((rel, full));
                }
            }

            if menu_files.is_empty() {
                return Err("No valid ReCircles menu .json found in archive.".into());
            }

            fs::create_dir_all(&self.dir)
                .map_err(|err| format!("Could not create {}: {err}", self.dir.display()))?;

            let mut installed_images: Vec<PathBuf> = Vec::new();
            for (rel, src) in &other_files {
                let dest = self.dir.join(rel);
                if let Some(parent) = dest.parent() {
                    fs::create_dir_all(parent).map_err(|err| {
                        format!("Could not create {}: {err}", parent.display())
                    })?;
                }
                fs::copy(src, &dest)
                    .map_err(|err| format!("Could not copy {}: {err}", rel.display()))?;
                if let Ok(meta) = fs::metadata(src) {
                    let _ = fs::set_permissions(&dest, meta.permissions());
                }
                if is_icon_image(&dest) {
                    installed_images.push(dest);
                }
            }

            let mut config = self.load_config()?;
            let mut installed_names = Vec::with_capacity(menu_files.len());
            let single_menu = menu_files.len() == 1;
            for (_, name, raw) in menu_files {
                write_atomic(&self.dir, &self.menu_path(&name), raw.as_bytes())?;
                let icon = find_matching_icon(&name, &installed_images, single_menu);
                pin_menu_entry(&mut config, &name, icon);
                installed_names.push(name);
            }
            self.save_config(&config)?;
            Ok(installed_names)
        })();

        let _ = fs::remove_dir_all(&tmp_dir);
        result
    }

    pub fn list_themes(&self) -> Vec<Theme> {
        let mut list = themes::builtin_themes();
        let Ok(entries) = fs::read_dir(self.themes_dir()) else {
            return list;
        };
        let mut local: Vec<Theme> = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let Some(file_name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if file_name.starts_with('.') {
                continue;
            }
            let Some(stem) = file_name.strip_suffix(".json") else {
                continue;
            };
            if themes::validate_theme_id(stem).is_err() {
                continue;
            }
            let Ok(raw) = fs::read_to_string(&path) else {
                continue;
            };
            match themes::parse_theme_json(stem, &raw) {
                Ok(theme) => {
                    if let Some(existing) = list
                        .iter_mut()
                        .find(|t| t.id.eq_ignore_ascii_case(&theme.id))
                    {
                        *existing = theme;
                    } else {
                        local.push(theme);
                    }
                }
                Err(err) => {
                    eprintln!("ReCircles: skipping invalid theme {}: {err}", path.display());
                }
            }
        }
        local.sort_by(|a, b| {
            a.name
                .to_ascii_lowercase()
                .cmp(&b.name.to_ascii_lowercase())
                .then_with(|| a.id.cmp(&b.id))
        });
        list.extend(local);
        list
    }

    pub fn find_theme(&self, query: &str) -> Option<Theme> {
        let key = themes::normalize_theme_key(query);
        if key.is_empty() || key == "custom" {
            return None;
        }
        let all = self.list_themes();
        if let Some(found) = all
            .iter()
            .find(|t| themes::normalize_theme_key(&t.id) == key)
        {
            return Some(found.clone());
        }
        if let Some(found) = all
            .iter()
            .find(|t| themes::normalize_theme_key(&t.name) == key)
        {
            return Some(found.clone());
        }
        if let Some(builtin) = themes::find_builtin(query) {
            // Respect a local override if one exists for the canonical built-in ID.
            return all
                .into_iter()
                .find(|t| t.id == builtin.id)
                .or(Some(builtin));
        }
        None
    }

    pub fn active_theme(&self, colors: &ColorsConfig) -> Option<Theme> {
        let all = self.list_themes();
        if let Some(name) = colors
            .theme
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty() && !s.eq_ignore_ascii_case("custom"))
            && let Some(found) = self.find_theme(name)
            && colors.same_palette(&found.colors)
        {
            return Some(found);
        }
        if colors.theme.is_none() {
            return all.into_iter().find(|t| colors.same_palette(&t.colors));
        }
        None
    }

    /// Import a local color theme from a `.json` file or `.zip` archive into
    /// `$XDG_CONFIG_HOME/recircles/themes/`.
    pub fn import_theme_file(&self, path: &Path) -> Result<Vec<String>, String> {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase())
            .unwrap_or_default();
        match ext.as_str() {
            "json" => self.import_theme_json(path),
            "zip" => self.import_theme_zip(path),
            _ => Err("Select a .json or .zip theme file.".into()),
        }
    }

    fn import_theme_json(&self, path: &Path) -> Result<Vec<String>, String> {
        let raw = fs::read_to_string(path)
            .map_err(|err| format!("Could not read {}: {err}", path.display()))?;
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("custom_theme");
        let id = themes::sanitize_theme_id(stem);
        themes::parse_theme_json(&id, &raw)
            .map_err(|err| format!("Invalid theme JSON in {}: {err}", path.display()))?;
        write_atomic(&self.themes_dir(), &self.theme_path(&id), raw.as_bytes())?;
        Ok(vec![id])
    }

    fn import_theme_zip(&self, path: &Path) -> Result<Vec<String>, String> {
        static THEME_ZIP_COUNTER: std::sync::atomic::AtomicU64 =
            std::sync::atomic::AtomicU64::new(0);
        let n = THEME_ZIP_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let tmp_dir = env::temp_dir().join(format!(
            "recircles-theme-import-{}-{n}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&tmp_dir);
        fs::create_dir_all(&tmp_dir)
            .map_err(|err| format!("Could not create temp dir: {err}"))?;

        let result = (|| -> Result<Vec<String>, String> {
            extract_zip_to(path, &tmp_dir)?;
            let root = unwrap_single_dir(&tmp_dir);
            let mut all_files = Vec::new();
            collect_extracted_files(&root, &root, &mut all_files)?;

            let mut theme_files: Vec<(String, String)> = Vec::new();
            for (_, full) in all_files {
                let is_json = full
                    .extension()
                    .and_then(|e| e.to_str())
                    .is_some_and(|e| e.eq_ignore_ascii_case("json"));
                if !is_json {
                    continue;
                }
                let Ok(raw) = fs::read_to_string(&full) else {
                    continue;
                };
                let stem = full
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("custom_theme");
                let id = themes::sanitize_theme_id(stem);
                if themes::parse_theme_json(&id, &raw).is_ok() {
                    theme_files.push((id, raw));
                }
            }

            if theme_files.is_empty() {
                return Err("No valid ReCircles theme .json found in archive.".into());
            }

            let themes_dir = self.themes_dir();
            let mut installed = Vec::with_capacity(theme_files.len());
            for (id, raw) in theme_files {
                write_atomic(&themes_dir, &self.theme_path(&id), raw.as_bytes())?;
                if !installed.contains(&id) {
                    installed.push(id);
                }
            }
            Ok(installed)
        })();

        let _ = fs::remove_dir_all(&tmp_dir);
        result
    }
}

fn is_full_color_dump(obj: &serde_json::Map<String, serde_json::Value>) -> bool {
    let has_keys = |section: &str, keys: &[&str]| {
        obj.get(section)
            .and_then(serde_json::Value::as_object)
            .is_some_and(|m| keys.iter().all(|k| m.contains_key(*k)))
    };
    has_keys(
        "ring",
        &[
            "top",
            "mid",
            "bottom",
            "rim_top",
            "rim_bottom",
            "divider",
            "shadow",
        ],
    ) && has_keys("capsule", &["top", "bottom", "border"])
        && has_keys("text", &["primary", "capsule", "suggestion", "shadow"])
        && has_keys(
            "accents",
            &[
                "files",
                "apps",
                "windows",
                "wifi",
                "bluetooth",
                "custom",
                "action",
            ],
        )
}

fn sanitize_imported_name(stem: &str) -> String {
    let mut name: String = stem
        .trim()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect();
    while name.starts_with('.') || name.starts_with('_') {
        name.remove(0);
    }
    if name.is_empty() {
        name = "custom_menu".into();
    }
    if validate_custom_name(&name).is_err() {
        name.push_str("_menu");
    }
    name
}

fn pin_menu_entry(config: &mut AppConfig, name: &str, icon: Option<String>) {
    if let Some(existing) = config.main_menu.iter_mut().find(|e| e.name == name) {
        if icon.is_some() {
            existing.icon = icon;
        }
    } else {
        config.main_menu.push(MainMenuEntry {
            name: name.to_owned(),
            label: None,
            icon,
        });
    }
}

fn extract_zip_to(archive: &Path, dest: &Path) -> Result<(), String> {
    let bsdtar = Command::new("bsdtar")
        .arg("-xf")
        .arg(archive)
        .arg("-C")
        .arg(dest)
        .stdin(Stdio::null())
        .output();
    match bsdtar {
        Ok(out) if out.status.success() => return Ok(()),
        Ok(out) => {
            let err = String::from_utf8_lossy(&out.stderr).trim().to_owned();
            let unzip = Command::new("unzip")
                .arg("-qq")
                .arg("-o")
                .arg(archive)
                .arg("-d")
                .arg(dest)
                .stdin(Stdio::null())
                .output();
            if let Ok(u) = unzip
                && u.status.success()
            {
                return Ok(());
            }
            return Err(if err.is_empty() {
                "Could not extract zip archive.".into()
            } else {
                format!("Could not extract zip: {err}")
            });
        }
        Err(_) => {}
    }

    let unzip = Command::new("unzip")
        .arg("-qq")
        .arg("-o")
        .arg(archive)
        .arg("-d")
        .arg(dest)
        .stdin(Stdio::null())
        .output()
        .map_err(|_| "Install bsdtar or unzip to import .zip menus.".to_string())?;
    if unzip.status.success() {
        Ok(())
    } else {
        let err = String::from_utf8_lossy(&unzip.stderr).trim().to_owned();
        Err(if err.is_empty() {
            "Could not extract zip archive.".into()
        } else {
            format!("Could not extract zip: {err}")
        })
    }
}

fn unwrap_single_dir(dir: &Path) -> PathBuf {
    let Ok(entries) = fs::read_dir(dir) else {
        return dir.to_path_buf();
    };
    let visible: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| !n.starts_with('.') && n != "__MACOSX")
        })
        .collect();
    if visible.len() == 1 && visible[0].is_dir() {
        visible.into_iter().next().unwrap()
    } else {
        dir.to_path_buf()
    }
}

fn collect_extracted_files(
    root: &Path,
    current: &Path,
    out: &mut Vec<(PathBuf, PathBuf)>,
) -> Result<(), String> {
    let entries = fs::read_dir(current)
        .map_err(|err| format!("Could not read extracted files: {err}"))?;
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if name == "__MACOSX" || name == ".DS_Store" {
            continue;
        }
        if path.is_dir() {
            collect_extracted_files(root, &path, out)?;
        } else if path.is_file()
            && let Ok(rel) = path.strip_prefix(root)
        {
            out.push((rel.to_path_buf(), path));
        }
    }
    Ok(())
}

fn is_icon_image(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| {
            matches!(
                e.to_ascii_lowercase().as_str(),
                "svg" | "png" | "jpg" | "jpeg" | "webp" | "ico"
            )
        })
}

fn find_matching_icon(name: &str, images: &[PathBuf], single_menu: bool) -> Option<String> {
    let by_name = images.iter().find(|p| {
        p.file_stem()
            .and_then(|s| s.to_str())
            .is_some_and(|s| s.eq_ignore_ascii_case(name))
    });
    let by_generic = images.iter().find(|p| {
        p.file_stem()
            .and_then(|s| s.to_str())
            .is_some_and(|s| s.eq_ignore_ascii_case("icon"))
    });
    let fallback = (single_menu && images.len() == 1).then(|| &images[0]);
    by_name
        .or(by_generic)
        .or(fallback)
        .and_then(|p| p.to_str())
        .map(str::to_owned)
}

pub fn parse_builtin(name: &str) -> Option<Mode> {
    match name.to_ascii_lowercase().as_str() {
        "files" => Some(Mode::Files),
        "apps" | "applications" => Some(Mode::Apps),
        "windows" | "alt-tab" => Some(Mode::Windows),
        "wifi" | "wi-fi" => Some(Mode::Wifi),
        "bluetooth" | "bt" => Some(Mode::Bluetooth),
        "settings" => Some(Mode::Settings),
        _ => None,
    }
}

pub fn builtin_name(mode: Mode) -> Option<&'static str> {
    match mode {
        Mode::Files => Some("files"),
        Mode::Apps => Some("apps"),
        Mode::Windows => Some("windows"),
        Mode::Wifi => Some("wifi"),
        Mode::Bluetooth => Some("bluetooth"),
        Mode::Settings => Some("settings"),
        Mode::Home | Mode::Custom => None,
    }
}

pub fn validate_custom_name(name: &str) -> Result<(), String> {
    if name.trim().is_empty() {
        return Err("Menu name cannot be empty.".into());
    }
    if name == "."
        || name == ".."
        || name.starts_with('.')
        || name.contains('/')
        || name.contains('\\')
        || name.contains('\0')
    {
        return Err(format!("Invalid menu name {name:?}."));
    }
    if parse_builtin(name).is_some()
        || name.eq_ignore_ascii_case("home")
        || name.eq_ignore_ascii_case("recircles")
    {
        return Err(format!("Menu name {name:?} is reserved."));
    }
    Ok(())
}

pub fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

pub fn expand_command_input(command: &str, input: Option<&str>) -> String {
    let Some(value) = input else {
        return command.to_owned();
    };
    let quoted = shell_quote(value);
    command.replace("{input}", &quoted).replace("{}", &quoted)
}

pub fn run_command_with_input(command: &str, input: Option<&str>) -> Result<(), String> {
    let expanded = expand_command_input(command, input);
    let mut cmd = Command::new("sh");
    cmd.arg("-c")
        .arg(&expanded)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if let Some(val) = input {
        cmd.env("RECIRCLES_INPUT", val);
    }
    cmd.spawn()
        .map(|_| ())
        .map_err(|err| format!("Couldn't run command: {err}"))
}

#[cfg(test)]
pub fn run_command_wait(command: &str) -> Result<(), String> {
    run_command_wait_with_input(command, None)
}

pub fn run_command_wait_with_input(command: &str, input: Option<&str>) -> Result<(), String> {
    let expanded = expand_command_input(command, input);
    let mut cmd = Command::new("sh");
    cmd.arg("-c").arg(&expanded).stdin(Stdio::null());
    if let Some(val) = input {
        cmd.env("RECIRCLES_INPUT", val);
    }
    let output = cmd
        .output()
        .map_err(|err| format!("Couldn't run command: {err}"))?;
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr);
        let detail = detail.trim();
        return Err(if detail.is_empty() {
            format!("Command exited with status {}.", output.status)
        } else {
            format!("Command failed: {detail}")
        });
    }
    Ok(())
}

fn write_atomic(dir: &Path, target: &Path, bytes: &[u8]) -> Result<(), String> {
    fs::create_dir_all(dir)
        .map_err(|err| format!("Could not create {}: {err}", dir.display()))?;
    let file_name = target
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("recircles.tmp");
    let pending = dir.join(format!(".{file_name}.pending.{}", std::process::id()));
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&pending)
        .map_err(|err| format!("Could not write {}: {err}", target.display()))?;
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|err| {
            let _ = fs::remove_file(&pending);
            format!("Could not write {}: {err}", target.display())
        })?;
    drop(file);
    fs::rename(&pending, target).map_err(|err| {
        let _ = fs::remove_file(&pending);
        format!("Could not save {}: {err}", target.display())
    })?;
    if let Ok(dir_handle) = File::open(dir) {
        let _ = dir_handle.sync_all();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    fn temp_store(label: &str) -> (PathBuf, ConfigStore) {
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = env::temp_dir().join(format!(
            "recircles-config-{label}-{}-{n}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        (dir.clone(), ConfigStore::from_dir(dir))
    }

    const SAMPLE_POWER_JSON: &str = r#"{
  "title": "Power",
  "items": [
    {
      "label": "Lock",
      "icon": "system-lock-screen",
      "left_click": "hyprlock",
      "right_click": "loginctl lock-session"
    },
    {
      "label": "Shutdown",
      "icon": "system-shutdown",
      "left_click": { "submenu": "confirm" },
      "right_click": "systemctl poweroff"
    }
  ],
  "submenus": {
    "confirm": {
      "title": "Confirm Shutdown",
      "items": [
        {
          "label": "Power Off",
          "icon": "system-shutdown",
          "left_click": "systemctl poweroff"
        }
      ]
    }
  }
}"#;

    #[test]
    fn first_run_generates_default_config_and_config_cmd_respects_force() {
        let (dir, store) = temp_store("default");
        assert!(!store.config_path().exists());

        let loaded = store.load_config().unwrap();
        assert!(store.config_path().exists());
        assert_eq!(loaded, AppConfig::default());

        assert!(store.generate_default_config(false).is_err());

        let mut custom = loaded;
        custom.main_menu.pop();
        store.save_config(&custom).unwrap();
        assert_eq!(store.load_config().unwrap().main_menu.len(), 5);

        store.generate_default_config(true).unwrap();
        assert_eq!(store.load_config().unwrap(), AppConfig::default());

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn add_remove_and_auto_prune_work_with_cli_and_manual_files() {
        let (dir, store) = temp_store("add-remove");
        let src_dir = env::temp_dir().join(format!("recircles-src-{}", std::process::id()));
        fs::create_dir_all(&src_dir).unwrap();
        let src_file = src_dir.join("power.json");
        fs::write(&src_file, SAMPLE_POWER_JSON).unwrap();

        // Add to known menus + main menu with icon.
        store
            .add_menu("power", Some(&src_file), Some("system-shutdown"))
            .unwrap();
        assert!(store.menu_path("power").is_file());
        let cfg = store.load_config().unwrap();
        assert_eq!(cfg.main_menu.len(), 7);
        assert_eq!(cfg.main_menu[6].name, "power");
        assert_eq!(cfg.main_menu[6].icon.as_deref(), Some("system-shutdown"));

        // Remove only from main menu keeps known menu file intact.
        store.remove_menu("power", true).unwrap();
        assert!(store.menu_path("power").is_file());
        assert_eq!(store.load_config().unwrap().main_menu.len(), 6);
        assert!(store.load_menu("power").is_ok());

        // Promote existing known menu back to main menu.
        store
            .add_menu("power", None, Some("system-shutdown-symbolic"))
            .unwrap();
        assert_eq!(store.load_config().unwrap().main_menu.len(), 7);

        // Manual deletion is auto-pruned on next config load.
        fs::remove_file(store.menu_path("power")).unwrap();
        let pruned = store.load_config().unwrap();
        assert_eq!(pruned.main_menu.len(), 6);
        let disk_text = fs::read_to_string(store.config_path()).unwrap();
        assert!(!disk_text.contains("power"));

        // Full remove deletes both the file and main menu entry.
        store
            .add_menu("power", Some(&src_file), Some("system-shutdown"))
            .unwrap();
        store.remove_menu("power", false).unwrap();
        assert!(!store.menu_path("power").exists());
        assert_eq!(store.load_config().unwrap().main_menu.len(), 6);

        // Built-in menus can be removed from main menu only with --main-menu.
        assert!(store.remove_menu("windows", false).is_err());
        store.remove_menu("windows", true).unwrap();
        assert_eq!(store.load_config().unwrap().main_menu.len(), 5);

        fs::remove_dir_all(dir).unwrap();
        fs::remove_dir_all(src_dir).unwrap();
    }

    #[test]
    fn validation_rejects_broken_submenu_references_and_reserved_names() {
        let (dir, store) = temp_store("validate");
        let broken = r#"{
            "items": [
                { "label": "Oops", "left_click": { "submenu": "missing" } }
            ]
        }"#;
        let parsed: MenuConfig = serde_json::from_str(broken).unwrap();
        assert!(parsed.validate().is_err());

        assert!(validate_custom_name("files").is_err());
        assert!(validate_custom_name("wifi").is_err());
        assert!(validate_custom_name("bluetooth").is_err());
        assert!(validate_custom_name("settings").is_err());
        assert!(validate_custom_name("../evil").is_err());
        assert!(validate_custom_name("recircles").is_err());
        assert!(validate_custom_name("media").is_ok());

        let menu: MenuConfig = serde_json::from_str(SAMPLE_POWER_JSON).unwrap();
        assert!(menu.validate().is_ok());
        assert_eq!(
            menu.items[0].action(false).and_then(Action::as_command),
            Some("hyprlock")
        );
        assert_eq!(
            menu.items[0].action(true).and_then(Action::as_command),
            Some("loginctl lock-session")
        );
        assert_eq!(
            menu.items[1].action(false).and_then(Action::as_submenu),
            Some("confirm")
        );
        assert_eq!(
            menu.items[1].action(true).and_then(Action::as_command),
            Some("systemctl poweroff")
        );

        assert!(matches!(
            store.resolve_start_target("apps"),
            Ok(StartTarget::Builtin(Mode::Apps))
        ));
        assert!(matches!(
            store.resolve_start_target("wifi"),
            Ok(StartTarget::Builtin(Mode::Wifi))
        ));
        assert!(matches!(
            store.resolve_start_target("bluetooth"),
            Ok(StartTarget::Builtin(Mode::Bluetooth))
        ));
        assert!(store.resolve_start_target("nonexistent").is_err());

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn colors_and_appearance_parse_with_defaults_and_validate_hex() {
        let default = AppConfig::default();
        assert!(default.validate().is_ok());

        let (r, g, b) = parse_hex_color("#ff8000", "test").unwrap();
        assert!((r - 1.0).abs() < 1e-6);
        assert!((g - 128.0 / 255.0).abs() < 1e-6);
        assert!(b.abs() < 1e-6);

        let (r, g, b) = parse_hex_color("#0f8", "test").unwrap();
        assert!(r.abs() < 1e-6);
        assert!((g - 1.0).abs() < 1e-6);
        assert!((b - 136.0 / 255.0).abs() < 1e-6);

        assert!(parse_hex_color("not-a-color", "test").is_err());
        assert!(parse_hex_color("#12345", "test").is_err());

        // Partial config fills missing appearance, colors, and keybinds with defaults.
        let partial: AppConfig = serde_json::from_str(
            r##"{
                "appearance": { "animations": false, "3d_animation": false, "translucent": false },
                "colors": { "accents": { "files": "#00ff88" } },
                "keybinds": { "back": "Ctrl+g", "next": ["Down", "Ctrl+j"] }
            }"##,
        )
        .unwrap();
        assert!(!partial.appearance.animations);
        assert!(!partial.appearance.animation_3d);
        assert!(default.appearance.animation_3d);
        assert!(!partial.appearance.translucent);
        assert!(partial.appearance.blur);
        assert_eq!(partial.appearance.overflow_layout, OverflowLayout::Box);
        let rings: AppConfig =
            serde_json::from_str(r#"{"appearance":{"overflow_layout":"rings"}}"#).unwrap();
        assert_eq!(rings.appearance.overflow_layout, OverflowLayout::Rings);
        assert_eq!(
            serde_json::to_value(default).unwrap()["appearance"]["overflow_layout"],
            "box"
        );
        assert_eq!(partial.colors.accents.files, "#00ff88");
        assert_eq!(partial.colors.accents.apps, AccentColors::default().apps);
        assert_eq!(partial.keybinds.back.0, vec!["Ctrl+g"]);
        assert_eq!(partial.keybinds.next.0, vec!["Down", "Ctrl+j"]);
        assert!(partial.validate().is_ok());
    }

    #[test]
    fn keybinds_match_modifiers_and_reject_invalid_specs() {
        use gdk::ModifierType as Mod;
        use gdk::keys::constants as key;

        let compiled = KeybindsConfig::default().compile().unwrap();
        assert!(compiled.back.matches_parts(key::Escape, Mod::empty()));
        assert!(!compiled.back.matches_parts(key::Escape, Mod::SHIFT_MASK));
        assert!(compiled.home.matches_parts(key::Escape, Mod::SHIFT_MASK));
        assert!(compiled.previous.matches_parts(key::ISO_Left_Tab, Mod::SHIFT_MASK));
        assert!(compiled.previous.matches_parts(key::Tab, Mod::SHIFT_MASK));
        assert!(compiled.next.matches_parts(key::Tab, Mod::empty()));
        assert!(compiled.clear_query.matches_parts(key::u, Mod::CONTROL_MASK));
        assert!(compiled.clear_query.matches_parts(key::U, Mod::CONTROL_MASK));
        assert!(compiled.activate.matches_parts(key::Return, Mod::empty()));
        assert!(
            compiled
                .secondary_activate
                .matches_parts(key::Return, Mod::SHIFT_MASK)
        );
        assert!(
            compiled
                .open_current_folder
                .matches_parts(key::Return, Mod::MOD1_MASK)
        );
        assert!(
            compiled
                .open_current_folder
                .matches_parts(key::KP_Enter, Mod::MOD1_MASK)
        );

        assert!(parse_key_combo("BogusMod+a", "test").is_err());
        assert!(parse_key_combo("Ctrl+NotARealKeyName123", "test").is_err());
        assert!(parse_key_combo("", "test").is_err());
    }

    #[test]
    fn combined_command_submenu_actions_and_on_open_validate() {
        let raw = r#"{
            "title": "Dynamic",
            "on_open": "true",
            "items": [
                {
                    "label": "Refresh & Open",
                    "left_click": { "command": "true", "submenu": "sub" }
                }
            ],
            "submenus": {
                "sub": {
                    "title": "Sub",
                    "on_open": "true",
                    "items": []
                }
            }
        }"#;
        let menu: MenuConfig = serde_json::from_str(raw).unwrap();
        assert!(menu.validate().is_ok());
        assert_eq!(menu.on_open.as_deref(), Some("true"));
        assert_eq!(menu.submenus["sub"].on_open.as_deref(), Some("true"));
        let action = menu.items[0].action(false).unwrap();
        assert_eq!(action.as_command(), Some("true"));
        assert_eq!(action.as_submenu(), Some("sub"));
        assert!(menu.items[0].opens_submenu());

        let empty_obj: MenuConfig = serde_json::from_str(
            r#"{ "items": [{ "label": "Bad", "left_click": {} }] }"#,
        )
        .unwrap();
        assert!(empty_obj.validate().is_err());

        let empty_on_open: MenuConfig =
            serde_json::from_str(r#"{ "on_open": "   ", "items": [] }"#).unwrap();
        assert!(empty_on_open.validate().is_err());

        assert!(run_command_wait("true").is_ok());
        assert!(run_command_wait("false").is_err());
    }

    #[test]
    fn custom_menu_loading_animation_and_reload_actions_parse_and_validate() {
        let raw = r#"{
            "title": "Dynamic",
            "on_open": "true",
            "loading_animation": true,
            "items": [
                {
                    "label": "Scan",
                    "left_click": { "command": "true", "reload": true, "loading_animation": true },
                    "right_click": { "update": true }
                },
                {
                    "label": "Sub",
                    "left_click": { "command": "true", "submenu": "sub", "loading_animation": false }
                }
            ],
            "submenus": {
                "sub": {
                    "title": "Sub",
                    "on_open": "true",
                    "loading_animation": false,
                    "items": []
                }
            }
        }"#;
        let menu: MenuConfig = serde_json::from_str(raw).unwrap();
        assert!(menu.validate().is_ok());
        assert!(menu.loading_animation);
        assert_eq!(menu.submenus["sub"].loading_animation, Some(false));

        let scan_left = menu.items[0].action(false).unwrap();
        assert!(scan_left.reloads());
        assert_eq!(scan_left.loading_animation(), Some(true));

        let scan_right = menu.items[0].action(true).unwrap();
        assert!(scan_right.reloads());
        assert_eq!(scan_right.as_command(), None);
        assert_eq!(scan_right.loading_animation(), None);

        let sub_left = menu.items[1].action(false).unwrap();
        assert!(!sub_left.reloads());
        assert_eq!(sub_left.loading_animation(), Some(false));

        // Only setting loading_animation without command, submenu, or reload is still invalid.
        let invalid: MenuConfig = serde_json::from_str(
            r#"{ "items": [{ "label": "Bad", "left_click": { "loading_animation": true } }] }"#,
        )
        .unwrap();
        assert!(invalid.validate().is_err());
    }

    #[test]
    fn custom_menu_input_and_list_layouts_parse_validate_and_expand_commands() {
        let raw = r#"{
            "title": "Settings",
            "items": [
                {
                    "label": "Adapter Name…",
                    "left_click": {
                        "input": "Visible name for this computer",
                        "placeholder": "Type a name…",
                        "default": "testhost",
                        "command": "bluetoothctl system-alias {}"
                    }
                },
                {
                    "label": "Power Profile",
                    "left_click": {
                        "title": "Choose Profile",
                        "options": ["Performance", "Balanced", "Power Saver"],
                        "command": "powerprofilesctl set {}"
                    }
                },
                {
                    "label": "Sub Dropdown",
                    "left_click": { "submenu": "dropdown_sub" }
                },
                {
                    "label": "Sub Input",
                    "left_click": { "submenu": "input_sub" }
                }
            ],
            "submenus": {
                "dropdown_sub": {
                    "title": "Select Output",
                    "layout": "dropdown",
                    "options": ["HDMI-A-1", "DP-1"],
                    "command": "wlr-randr --output {}"
                },
                "input_sub": {
                    "title": "Set Hostname",
                    "layout": "input",
                    "prompt": "Enter new hostname",
                    "default": "arch",
                    "command": "hostnamectl set-hostname {input}"
                }
            }
        }"#;
        let menu: MenuConfig = serde_json::from_str(raw).unwrap();
        assert!(menu.validate().is_ok());

        let act0 = menu.items[0].action(false).unwrap();
        assert!(act0.opens_input());
        let prompt0 = act0.input_prompt("Adapter Name…").unwrap();
        assert_eq!(prompt0.title.as_deref(), Some("Adapter Name…"));
        assert_eq!(
            prompt0.prompt.as_deref(),
            Some("Visible name for this computer")
        );
        assert_eq!(prompt0.default.as_deref(), Some("testhost"));

        let act1 = menu.items[1].action(false).unwrap();
        let inline_list = act1.inline_list_submenu("Power Profile").unwrap();
        assert_eq!(inline_list.layout, MenuLayout::List);
        assert_eq!(inline_list.display_title("fallback"), "Choose Profile");
        let resolved = inline_list.resolved_items();
        assert_eq!(resolved.len(), 3);
        assert_eq!(resolved[0].label, "Performance");
        assert_eq!(
            resolved[0].action(false).and_then(Action::as_command),
            Some("powerprofilesctl set {}")
        );

        let sub_drop = &menu.submenus["dropdown_sub"];
        assert_eq!(sub_drop.layout, MenuLayout::List);
        assert_eq!(sub_drop.resolved_items().len(), 2);

        let sub_in = &menu.submenus["input_sub"];
        assert_eq!(sub_in.layout, MenuLayout::Input);
        assert_eq!(
            sub_in.input_prompt("input_sub").prompt.as_deref(),
            Some("Enter new hostname")
        );

        assert_eq!(
            expand_command_input("echo {} and {input}", Some("User's PC")),
            "echo 'User'\\''s PC' and 'User'\\''s PC'"
        );
        assert!(
            run_command_wait_with_input(
                "test \"$RECIRCLES_INPUT\" = \"User's PC\" && test {} = \"User's PC\"",
                Some("User's PC")
            )
            .is_ok()
        );

        // Input layout without command/submenu/reload fails validation.
        let bad_input: MenuConfig =
            serde_json::from_str(r#"{ "layout": "input", "title": "Broken" }"#).unwrap();
        assert!(bad_input.validate().is_err());

        // Options list without command/submenu/reload fails validation.
        let bad_opts: MenuConfig =
            serde_json::from_str(r#"{ "layout": "list", "options": ["A", "B"] }"#).unwrap();
        assert!(bad_opts.validate().is_err());
    }

    #[test]
    fn imports_standalone_json_and_zip_menu_bundles() {
        let (dir, store) = temp_store("import");
        let stage = env::temp_dir().join(format!("recircles-stage-{}", std::process::id()));
        let _ = fs::remove_dir_all(&stage);
        fs::create_dir_all(stage.join("power_pack")).unwrap();

        // 1. Standalone .json import
        let standalone = stage.join("quick.json");
        fs::write(&standalone, SAMPLE_POWER_JSON).unwrap();
        let imported = store.import_menu_file(&standalone).unwrap();
        assert_eq!(imported, vec!["quick"]);
        assert!(store.menu_path("quick").is_file());
        let cfg = store.load_config().unwrap();
        assert!(cfg.main_menu.iter().any(|e| e.name == "quick"));

        // 2. .zip bundle with wrapped folder, menu JSON, and matching SVG icon
        let pack_dir = stage.join("power_pack");
        fs::write(pack_dir.join("power.json"), SAMPLE_POWER_JSON).unwrap();
        fs::write(pack_dir.join("power.svg"), "<svg/>").unwrap();
        let zip_path = stage.join("power_pack.zip");
        let status = Command::new("bsdtar")
            .arg("-a")
            .arg("-cf")
            .arg(&zip_path)
            .arg("-C")
            .arg(&stage)
            .arg("power_pack")
            .status()
            .unwrap();
        assert!(status.success());

        let zip_imported = store.import_menu_file(&zip_path).unwrap();
        assert_eq!(zip_imported, vec!["power"]);
        assert!(store.menu_path("power").is_file());
        assert!(dir.join("power.svg").is_file());
        let cfg = store.load_config().unwrap();
        let entry = cfg.main_menu.iter().find(|e| e.name == "power").unwrap();
        assert_eq!(
            entry.icon.as_deref(),
            dir.join("power.svg").to_str()
        );

        let _ = fs::remove_dir_all(&stage);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn config_resolves_builtin_and_local_themes_and_imports_json_and_zip() {
        let (dir, store) = temp_store("themes");
        let stage = env::temp_dir().join(format!("recircles-theme-stage-{}", std::process::id()));
        let _ = fs::remove_dir_all(&stage);
        fs::create_dir_all(stage.join("theme_pack")).unwrap();

        // 1. Editing "theme" inside an existing full recircles.json switches palette.
        let mut cfg = store.load_config().unwrap();
        assert_eq!(cfg.colors.theme.as_deref(), Some("default"));
        assert!(store.themes_dir().is_dir());

        cfg.colors.theme = Some("light_green".into());
        store.save_config(&cfg).unwrap();
        let loaded = store.load_config().unwrap();
        let light_green = store.find_theme("light_green").unwrap();
        assert_eq!(loaded.colors.theme.as_deref(), Some("light_green"));
        assert!(loaded.colors.same_palette(&light_green.colors));
        assert_eq!(
            store.active_theme(&loaded.colors).map(|t| t.id),
            Some("light_green".into())
        );

        // 2. Partial color override on top of a named theme in recircles.json.
        fs::write(
            store.config_path(),
            r##"{"colors":{"theme":"light_green","accents":{"files":"#ff0000"}}}"##,
        )
        .unwrap();
        let partial = store.load_config().unwrap();
        assert_eq!(partial.colors.ring, light_green.colors.ring);
        assert_eq!(partial.colors.accents.files, "#ff0000");
        assert_eq!(partial.colors.accents.apps, light_green.colors.accents.apps);

        // 3. Old config without "theme" preserves custom colors untouched.
        fs::write(
            store.config_path(),
            r##"{"colors":{"ring":{"top":"#112233"}}}"##,
        )
        .unwrap();
        let legacy = store.load_config().unwrap();
        assert_eq!(legacy.colors.theme, None);
        assert_eq!(legacy.colors.ring.top, "#112233");

        // 4. Standalone .json theme import into /themes and loading it via recircles.json.
        let sample_theme = r##"{
            "name": "Matcha Glass",
            "ring": {
                "top": "#d8f5df",
                "mid": "#c2ebc9",
                "bottom": "#b0e0ba",
                "rim_top": "#ffffff",
                "rim_bottom": "#4eb36b",
                "divider": "#2e6b40",
                "shadow": "#0c2214"
            },
            "accents": {
                "files": "#1b8047"
            }
        }"##;
        let standalone = stage.join("Matcha Glass.json");
        fs::write(&standalone, sample_theme).unwrap();
        let imported = store.import_theme_file(&standalone).unwrap();
        assert_eq!(imported, vec!["matcha_glass"]);
        assert!(store.theme_path("matcha_glass").is_file());

        fs::write(
            store.config_path(),
            r#"{"colors":{"theme":"matcha_glass"}}"#,
        )
        .unwrap();
        let loaded_local = store.load_config().unwrap();
        assert_eq!(loaded_local.colors.ring.top, "#d8f5df");
        assert_eq!(
            store.active_theme(&loaded_local.colors).map(|t| t.name),
            Some("Matcha Glass".into())
        );

        // Non-theme JSON (like a custom menu) is rejected.
        let not_a_theme = stage.join("power.json");
        fs::write(&not_a_theme, SAMPLE_POWER_JSON).unwrap();
        assert!(store.import_theme_file(&not_a_theme).is_err());

        // 5. .zip theme archive import (wrapped directory, ignores non-theme files).
        let pack_dir = stage.join("theme_pack");
        fs::write(
            pack_dir.join("ocean_breeze.json"),
            r##"{"name":"Ocean Breeze","colors":{"ring":{"top":"#103048"}}}"##,
        )
        .unwrap();
        fs::write(pack_dir.join("README.txt"), "hello").unwrap();
        fs::write(pack_dir.join("power.json"), SAMPLE_POWER_JSON).unwrap();
        let zip_path = stage.join("theme_pack.zip");
        let status = Command::new("bsdtar")
            .arg("-a")
            .arg("-cf")
            .arg(&zip_path)
            .arg("-C")
            .arg(&stage)
            .arg("theme_pack")
            .status()
            .unwrap();
        assert!(status.success());

        let zip_imported = store.import_theme_file(&zip_path).unwrap();
        assert_eq!(zip_imported, vec!["ocean_breeze"]);
        assert!(store.theme_path("ocean_breeze").is_file());
        let ocean = store.find_theme("ocean_breeze").unwrap();
        assert_eq!(ocean.name, "Ocean Breeze");
        assert_eq!(ocean.colors.ring.top, "#103048");

        let _ = fs::remove_dir_all(&stage);
        let _ = fs::remove_dir_all(&dir);
    }
}
