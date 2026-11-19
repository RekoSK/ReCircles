use crate::{apps, bluetooth, config, files, history::History, ring, settings, wifi, windows};

pub const PAGE_SIZE: usize = 8;
pub const GRID_COLUMNS: usize = 4;
pub const GRID_ROWS: usize = 3;
pub const GRID_SIZE: usize = GRID_COLUMNS * GRID_ROWS;
pub const LIST_ROWS: usize = 6;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Home,
    Files,
    Apps,
    Windows,
    Wifi,
    Bluetooth,
    Settings,
    Custom,
}

impl Mode {
    pub fn title(self) -> &'static str {
        match self {
            Self::Home | Self::Custom => "",
            Self::Files => "Files",
            Self::Apps => "Applications",
            Self::Windows => "Alt-Tab",
            Self::Wifi => "Wi-Fi",
            Self::Bluetooth => "Bluetooth",
            Self::Settings => "Settings",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HomeEntry {
    pub name: String,
    pub label: String,
    pub icon: String,
    pub builtin: Option<Mode>,
}

#[derive(Clone)]
pub enum Target {
    Mode(Mode),
    HomeEntry(HomeEntry),
    CustomItem(config::MenuItem),
    File(files::FileEntry),
    App(apps::App),
    Window(windows::Window),
    Wifi(wifi::WifiItem),
    Bluetooth(bluetooth::BluetoothItem),
    Settings(settings::SettingsItem),
}

impl Target {
    pub fn label(&self) -> String {
        match self {
            Self::Mode(mode) => mode.title().into(),
            Self::HomeEntry(entry) => entry.label.clone(),
            Self::CustomItem(item) => item.label.clone(),
            Self::File(file) => file.label(),
            Self::App(app) => app.name.clone(),
            Self::Window(window) if window.title.is_empty() => window.app_id.clone(),
            Self::Window(window) => window.title.clone(),
            Self::Wifi(item) => item.label.clone(),
            Self::Bluetooth(item) => item.label.clone(),
            Self::Settings(item) => item.label.clone(),
        }
    }

    pub fn icon(&self) -> ring::Icon {
        use ring::Icon;
        match self {
            Self::Mode(Mode::Files) => Icon::Files,
            Self::Mode(Mode::Apps) => Icon::Applications,
            Self::Mode(Mode::Wifi) => Icon::Wifi,
            Self::Mode(Mode::Bluetooth) => Icon::Bluetooth,
            Self::Mode(Mode::Settings) => Icon::Settings,
            Self::Mode(_) => Icon::Windows,
            Self::HomeEntry(entry) => match entry.builtin {
                Some(Mode::Files) => Icon::Files,
                Some(Mode::Apps) => Icon::Applications,
                Some(Mode::Windows) => Icon::Windows,
                Some(Mode::Wifi) => Icon::Wifi,
                Some(Mode::Bluetooth) => Icon::Bluetooth,
                Some(Mode::Settings) => Icon::Settings,
                _ => Icon::CustomMenu,
            },
            Self::CustomItem(item) if item.opens_submenu() => Icon::Submenu,
            Self::CustomItem(_) => Icon::Action,
            Self::File(file) if file.is_home => Icon::Home,
            Self::File(file) if file.is_dir => Icon::Folder,
            Self::File(_) => Icon::File,
            Self::App(_) => Icon::App,
            Self::Window(_) => Icon::Window,
            Self::Wifi(item) => item.icon,
            Self::Bluetooth(item) => item.icon,
            Self::Settings(item) => item.icon,
        }
    }

    pub fn is_active(&self) -> bool {
        match self {
            Self::Wifi(item) => item.active,
            Self::Bluetooth(item) => item.active,
            Self::Settings(item) => item.active,
            _ => false,
        }
    }

    fn completion(&self) -> Option<&str> {
        match self {
            Self::Mode(_) | Self::HomeEntry(_) => None,
            Self::CustomItem(item) => Some(item.label.as_str()),
            Self::File(file) => file.completion.as_deref(),
            Self::App(app) => Some(app.name.as_str()),
            Self::Window(window) if window.title.is_empty() => Some(window.app_id.as_str()),
            Self::Window(window) => Some(window.title.as_str()),
            Self::Wifi(item) => Some(item.label.as_str()),
            Self::Bluetooth(item) => Some(item.label.as_str()),
            Self::Settings(item) => Some(item.label.as_str()),
        }
        .filter(|name| !name.is_empty())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct PreCompletion {
    query: String,
    first_visible: usize,
    selected: Option<usize>,
}

pub struct Menu {
    pub mode: Mode,
    pub layout: config::MenuLayout,
    pub title: String,
    pub query: String,
    pub entries: Vec<Target>,
    pub first_visible: usize,
    overflow_layout: config::OverflowLayout,
    pub selected: Option<usize>,
    pub pressed: Option<(usize, u32)>,
    pub message: String,
    pub idle_message: String,
    completion_index: Option<usize>,
    pre_completion: Option<PreCompletion>,
}

impl Menu {
    #[cfg(test)]
    pub fn home() -> Self {
        Self::home_with(
            [Mode::Files, Mode::Apps, Mode::Windows]
                .map(Target::Mode)
                .into(),
            config::OverflowLayout::Rings,
        )
    }

    pub fn home_with(entries: Vec<Target>, overflow_layout: config::OverflowLayout) -> Self {
        Self {
            mode: Mode::Home,
            layout: config::MenuLayout::Ring,
            title: String::new(),
            query: String::new(),
            entries,
            first_visible: 0,
            overflow_layout,
            selected: None,
            pressed: None,
            message: String::new(),
            idle_message: String::new(),
            completion_index: None,
            pre_completion: None,
        }
    }

    pub fn set_overflow_layout(&mut self, overflow_layout: config::OverflowLayout) {
        self.overflow_layout = overflow_layout;
        self.first_visible = 0;
        self.selected = None;
        self.pressed = None;
    }

    pub fn title(&self) -> &str {
        if !self.title.is_empty() {
            &self.title
        } else {
            self.mode.title()
        }
    }

    pub fn visible(&self) -> &[Target] {
        let start = self.first_visible;
        &self.entries[start..(start + self.capacity()).min(self.entries.len())]
    }

    pub fn uses_list(&self) -> bool {
        self.layout == config::MenuLayout::List
    }

    pub fn uses_box(&self) -> bool {
        !self.uses_list()
            && self.overflow_layout == config::OverflowLayout::Box
            && self.entries.len() > PAGE_SIZE
    }

    pub fn page(&self) -> usize {
        self.first_visible / self.capacity()
    }

    pub fn pages(&self) -> usize {
        self.entries.len().div_ceil(self.capacity())
    }

    fn capacity(&self) -> usize {
        if self.uses_list() {
            LIST_ROWS
        } else if self.uses_box() {
            GRID_SIZE
        } else {
            PAGE_SIZE
        }
    }

    pub fn results(&mut self, entries: Vec<Target>) {
        self.entries = entries;
        self.first_visible = 0;
        self.selected = None;
        self.pressed = None;
        self.message.clear();
        self.end_completion();
        self.pre_completion = None;
    }

    pub fn suggestion(&self) -> Option<(usize, &str)> {
        if self.query.is_empty() || self.completion_index.is_some() {
            return None;
        }
        let query = self.query.to_lowercase();
        let prefix_chars = self.query.chars().count();
        self.entries.iter().enumerate().find_map(|(index, entry)| {
            let name = entry.completion()?;
            let (end, _) = name.char_indices().nth(prefix_chars)?;
            (name[..end].to_lowercase() == query).then_some((index, &name[end..]))
        })
    }

    #[cfg(test)]
    pub fn cycle_completion(&mut self, direction: isize) -> Option<String> {
        let count = self.entries.len() as isize;
        let start = match self.completion_index {
            Some(index) => index as isize + direction,
            None if direction < 0 => count - 1,
            None => self
                .suggestion()
                .map(|(index, _)| index)
                .or_else(|| {
                    self.entries
                        .iter()
                        .position(|entry| entry.completion() == Some(self.query.as_str()))
                })
                .unwrap_or(0) as isize,
        };
        let index = (0..count)
            .map(|offset| (start + offset * direction).rem_euclid(count) as usize)
            .find(|&index| self.entries[index].completion().is_some())?;
        let name = self.accept_completion(index)?;
        self.completion_index = Some(index);
        Some(name)
    }

    pub fn accept_completion(&mut self, index: usize) -> Option<String> {
        let name = self.entries.get(index)?.completion()?;
        if self.pre_completion.is_none() {
            self.pre_completion = Some(PreCompletion {
                query: self.query.clone(),
                first_visible: self.first_visible,
                selected: self.selected,
            });
        }
        self.query = name.to_owned();
        self.select_entry(index);
        Some(self.query.clone())
    }

    pub fn undo_completion(&mut self) -> Option<String> {
        let prev = self.pre_completion.take()?;
        self.query = prev.query;
        self.first_visible = prev.first_visible;
        self.selected = prev.selected;
        self.pressed = None;
        self.end_completion();
        Some(self.query.clone())
    }

    pub fn end_completion(&mut self) {
        self.completion_index = None;
    }

    pub fn step(&mut self, delta: isize) {
        if self.entries.is_empty() {
            return;
        }
        let next = match self.selected {
            Some(index) => (self.first_visible + index) as isize + delta,
            None if delta < 0 => self.first_visible as isize - 1,
            None => self.first_visible as isize,
        }
        .rem_euclid(self.entries.len() as isize) as usize;
        self.select_entry(next);
    }

    fn select_entry(&mut self, index: usize) {
        self.first_visible = if self.uses_list() {
            self.first_visible
                .clamp((index + 1).saturating_sub(LIST_ROWS), index)
        } else if self.uses_box() {
            let row = index / GRID_COLUMNS;
            let first_row = (self.first_visible / GRID_COLUMNS)
                .clamp((row + 1).saturating_sub(GRID_ROWS), row);
            first_row * GRID_COLUMNS
        } else {
            index / PAGE_SIZE * PAGE_SIZE
        };
        self.selected = Some(index - self.first_visible);
        self.pressed = None;
    }

    pub fn scroll_list_to(&mut self, first: usize) {
        let last = self.entries.len().saturating_sub(LIST_ROWS);
        let first = first.min(last);
        if first != self.first_visible {
            self.first_visible = first;
            self.selected = None;
            self.pressed = None;
            self.end_completion();
        }
    }

    pub fn scroll_box_to(&mut self, first: usize) {
        let last_row = self.entries.len().div_ceil(GRID_COLUMNS).saturating_sub(GRID_ROWS);
        let first = (first / GRID_COLUMNS).min(last_row) * GRID_COLUMNS;
        if first != self.first_visible {
            self.first_visible = first;
            self.selected = None;
            self.pressed = None;
            self.end_completion();
        }
    }

    pub fn turn_page(&mut self, delta: isize) {
        if self.pages() < 2 {
            return;
        }
        if self.uses_list() {
            let last = self.entries.len() - LIST_ROWS;
            self.first_visible = match (self.first_visible, delta) {
                (0, -1) => last,
                (first, 1) if first == last => 0,
                (first, _) => first
                    .saturating_add_signed(delta * LIST_ROWS as isize)
                    .min(last),
            };
        } else if self.uses_box() {
            let last = (self.entries.len().div_ceil(GRID_COLUMNS) - GRID_ROWS) * GRID_COLUMNS;
            self.first_visible = match (self.first_visible, delta) {
                (0, -1) => last,
                (first, 1) if first == last => 0,
                (first, _) => first
                    .saturating_add_signed(delta * GRID_SIZE as isize)
                    .min(last),
            };
        } else {
            let page = (self.page() as isize + delta).rem_euclid(self.pages() as isize) as usize;
            self.first_visible = page * PAGE_SIZE;
        }
        self.selected = None;
        self.pressed = None;
        self.end_completion();
    }

    pub fn status(&self) -> String {
        if !self.message.is_empty() {
            return self.message.clone();
        }
        if let Some(entry) = self.selected.and_then(|i| self.visible().get(i)) {
            return match entry {
                Target::Mode(_) | Target::HomeEntry(_) => String::new(),
                Target::File(f) => f.path.to_string_lossy().into_owned(),
                Target::Window(w) if !w.app_id.is_empty() => {
                    format!("{} · {}", w.app_id, entry.label())
                }
                Target::Wifi(item) if !item.detail.is_empty() => item.detail.clone(),
                Target::Bluetooth(item) if !item.detail.is_empty() => item.detail.clone(),
                Target::Settings(item) if !item.detail.is_empty() => item.detail.clone(),
                _ => entry.label(),
            };
        }
        if !self.idle_message.is_empty()
            && self.query.is_empty()
            && !(self.mode == Mode::Custom && self.entries.is_empty())
        {
            return self.idle_message.clone();
        }
        match (self.mode, self.query.is_empty(), self.entries.is_empty()) {
            (Mode::Home, _, _) => String::new(),
            (Mode::Files, true, true) => "Type a folder path to get started".into(),
            (Mode::Files, true, false) => "Frequent folders · Type a path".into(),
            (Mode::Apps, true, false) => "Type an application name".into(),
            (Mode::Windows, true, true) => "No open windows".into(),
            (Mode::Windows, true, false) => "Search by window title or app".into(),
            (Mode::Wifi, true, true) => "No Wi-Fi networks".into(),
            (Mode::Wifi, true, false) => "Type a network name".into(),
            (Mode::Bluetooth, true, true) => "No Bluetooth devices".into(),
            (Mode::Bluetooth, true, false) => "Type a device name".into(),
            (Mode::Settings, true, true) => "No options".into(),
            (Mode::Settings, true, false) => "Type to filter settings".into(),
            (Mode::Custom, true, true) => String::new(),
            (Mode::Custom, true, false) if self.uses_list() => {
                "Click to choose · Esc to go back".into()
            }
            (Mode::Custom, true, false) => "Type to filter".into(),
            (_, _, true) => "No matches".into(),
            _ => String::new(),
        }
    }
}

pub fn app_matches(catalog: &[apps::App], query: &str, history: &History) -> Vec<Target> {
    let terms = search_terms(query);
    let mut matches: Vec<_> = catalog
        .iter()
        .filter(|a| {
            let name = a.name.to_lowercase();
            terms.iter().all(|term| name.contains(term))
        })
        .collect();
    matches.sort_by_cached_key(|a| {
        (
            std::cmp::Reverse(history.app_count(&a.id)),
            a.name.to_lowercase(),
            &a.id,
        )
    });
    matches.into_iter().cloned().map(Target::App).collect()
}

pub fn window_matches(catalog: &[windows::Window], query: &str) -> Vec<Target> {
    let terms = search_terms(query);
    catalog
        .iter()
        .filter(|w| {
            let name = format!("{} {}", w.app_id, w.title).to_lowercase();
            terms.iter().all(|term| name.contains(term))
        })
        .cloned()
        .map(Target::Window)
        .collect()
}

pub fn custom_matches(items: &[config::MenuItem], query: &str) -> Vec<Target> {
    let terms = search_terms(query);
    items
        .iter()
        .filter(|item| {
            let label = item.label.to_lowercase();
            terms.iter().all(|term| label.contains(term))
        })
        .cloned()
        .map(Target::CustomItem)
        .collect()
}

pub fn wifi_matches(items: &[wifi::WifiItem], query: &str) -> Vec<Target> {
    let terms = search_terms(query);
    items
        .iter()
        .filter(|item| {
            let haystack = format!("{} {}", item.label, item.detail).to_lowercase();
            terms.iter().all(|term| haystack.contains(term))
        })
        .cloned()
        .map(Target::Wifi)
        .collect()
}

pub fn bluetooth_matches(items: &[bluetooth::BluetoothItem], query: &str) -> Vec<Target> {
    let terms = search_terms(query);
    items
        .iter()
        .filter(|item| {
            let haystack = format!("{} {}", item.label, item.detail).to_lowercase();
            terms.iter().all(|term| haystack.contains(term))
        })
        .cloned()
        .map(Target::Bluetooth)
        .collect()
}

pub fn settings_matches(items: &[settings::SettingsItem], query: &str) -> Vec<Target> {
    let terms = search_terms(query);
    items
        .iter()
        .filter(|item| {
            let haystack = format!("{} {}", item.label, item.detail).to_lowercase();
            terms.iter().all(|term| haystack.contains(term))
        })
        .cloned()
        .map(Target::Settings)
        .collect()
}

fn search_terms(query: &str) -> Vec<String> {
    query.split_whitespace().map(str::to_lowercase).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_ring_ranks_launches_then_alphabetical_fill_and_searches_the_catalog() {
        let dir = std::env::temp_dir().join(format!("recircles-menu-{}", std::process::id()));
        let mut history = History::from_dir(dir.clone()).unwrap();
        let names = [
            "Zebra", "Editor", "Beta", "Atlas", "Gamma", "Eta", "Kappa", "Iota", "Delta", "Theta",
        ];
        let apps: Vec<_> = names
            .iter()
            .map(|name| apps::App {
                id: name.to_string(),
                name: name.to_string(),
                icon: String::new(),
            })
            .collect();
        history.record_app("Editor").unwrap();
        history.record_app("Zebra").unwrap();
        history.record_app("Zebra").unwrap();
        let matches = app_matches(&apps, "", &history);
        assert_eq!(matches.len(), 10);
        let labels: Vec<_> = matches.iter().take(4).map(Target::label).collect();
        assert_eq!(labels, ["Zebra", "Editor", "Atlas", "Beta"]);
        assert_eq!(app_matches(&apps, "THETA", &history)[0].label(), "Theta");
        let mut menu = Menu::home();
        menu.results(matches);
        assert_eq!(menu.pages(), 2);
        assert_eq!(menu.visible().len(), 8);
        menu.turn_page(1);
        assert_eq!(menu.visible().len(), 2);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn overflow_follows_each_result_set_and_keeps_shortcuts_relative_to_visible_rows() {
        let entries: Vec<_> = (0..19)
            .map(|i| {
                Target::App(apps::App {
                    id: i.to_string(),
                    name: format!("App {i}"),
                    icon: String::new(),
                })
            })
            .collect();
        let mut menu = Menu::home_with(entries.clone(), config::OverflowLayout::Box);
        for mode in [
            Mode::Home,
            Mode::Files,
            Mode::Apps,
            Mode::Windows,
            Mode::Custom,
            Mode::Wifi,
            Mode::Bluetooth,
        ] {
            menu.mode = mode;
            menu.results(entries[..9].to_vec());
            assert!(menu.uses_box());
            menu.results(entries[..8].to_vec());
            assert!(!menu.uses_box());
        }
        menu.results(entries);
        menu.scroll_box_to(4);
        assert_eq!(menu.visible()[0].label(), "App 4");
        assert_eq!(menu.visible()[7].label(), "App 11");
        menu.selected = Some(7);
        menu.step(1);
        assert_eq!(menu.first_visible, 4);
        assert_eq!(menu.visible()[menu.selected.unwrap()].label(), "App 12");
        menu.turn_page(1);
        assert_eq!(menu.first_visible, 8);
        assert_eq!(menu.visible().len(), 11);
        menu.accept_completion(0).unwrap();
        assert_eq!((menu.first_visible, menu.selected), (0, Some(0)));
        menu.undo_completion().unwrap();
        assert_eq!((menu.first_visible, menu.selected), (8, None));
        menu.turn_page(1);
        assert_eq!(menu.first_visible, 0);
        menu.results(Vec::new());
        assert!(!menu.uses_box());
    }

    #[test]
    fn paging_and_keyboard_navigation_reach_every_result() {
        let mut menu = Menu::home();
        menu.results(vec![Target::Mode(Mode::Files); 19]);
        menu.step(-1);
        assert_eq!((menu.page(), menu.selected), (2, Some(2)));
        assert_eq!(menu.visible().len(), 3);
        for i in 0..19 {
            menu.step(1);
            assert_eq!((menu.page(), menu.selected), (i / 8, Some(i % 8)));
        }
        menu.turn_page(1);
        assert_eq!((menu.page(), menu.selected), (0, None));
    }

    #[test]
    fn suggestion_and_tab_completion_preserve_the_original_matches() {
        let mut menu = Menu::home();
        menu.mode = Mode::Apps;
        menu.query = "éD".into();
        menu.results(
            ["Text Éditeur", "Éditeur", "Édition"]
                .into_iter()
                .map(|name| {
                    Target::App(apps::App {
                        id: name.into(),
                        name: name.into(),
                        icon: String::new(),
                    })
                })
                .collect(),
        );

        assert_eq!(menu.suggestion(), Some((1, "iteur")));
        let suggested = menu.suggestion().unwrap().0;
        assert_eq!(
            menu.accept_completion(suggested).as_deref(),
            Some("Éditeur")
        );
        assert_eq!(menu.selected, Some(1));

        menu.query = "éD".into();
        assert_eq!(menu.cycle_completion(1).as_deref(), Some("Éditeur"));
        assert_eq!(menu.cycle_completion(1).as_deref(), Some("Édition"));
        assert_eq!(menu.cycle_completion(1).as_deref(), Some("Text Éditeur"));
        assert_eq!(menu.selected, Some(0));
        assert_eq!(menu.cycle_completion(-1).as_deref(), Some("Édition"));

        menu.end_completion();
        assert_eq!(menu.cycle_completion(1).as_deref(), Some("Édition"));

        menu.end_completion();
        menu.query.clear();
        assert_eq!(menu.cycle_completion(1).as_deref(), Some("Text Éditeur"));
        assert_eq!(menu.query, "Text Éditeur");
    }

    #[test]
    fn undo_completion_restores_pre_completion_query_page_and_selection() {
        let mut menu = Menu::home();
        menu.mode = Mode::Apps;
        menu.query = "app".into();
        let entries: Vec<_> = (0..12)
            .map(|i| {
                Target::App(apps::App {
                    id: format!("app-{i}"),
                    name: format!("App {i:02}"),
                    icon: String::new(),
                })
            })
            .collect();
        menu.results(entries);
        menu.turn_page(1);
        menu.selected = Some(2);

        assert_eq!(menu.cycle_completion(1).as_deref(), Some("App 00"));
        assert_eq!(menu.cycle_completion(1).as_deref(), Some("App 01"));
        assert_eq!((menu.page(), menu.selected), (0, Some(1)));

        assert_eq!(menu.undo_completion().as_deref(), Some("app"));
        assert_eq!((menu.page(), menu.selected), (1, Some(2)));
        assert_eq!(menu.undo_completion(), None);
    }

    #[test]
    fn custom_menu_filtering_preserves_declared_order() {
        let items = vec![
            config::MenuItem {
                label: "Suspend".into(),
                icon: String::new(),
                left_click: Some(config::Action::Command("systemctl suspend".into())),
                right_click: None,
            },
            config::MenuItem {
                label: "Lock Screen".into(),
                icon: String::new(),
                left_click: Some(config::Action::Command("hyprlock".into())),
                right_click: None,
            },
            config::MenuItem {
                label: "Shut Down".into(),
                icon: String::new(),
                left_click: None,
                right_click: None,
            },
        ];
        let all: Vec<_> = custom_matches(&items, "")
            .iter()
            .map(Target::label)
            .collect();
        assert_eq!(all, ["Suspend", "Lock Screen", "Shut Down"]);

        let filtered: Vec<_> = custom_matches(&items, "s")
            .iter()
            .map(Target::label)
            .collect();
        assert_eq!(filtered, ["Suspend", "Lock Screen", "Shut Down"]);

        let narrow: Vec<_> = custom_matches(&items, "screen lock")
            .iter()
            .map(Target::label)
            .collect();
        assert_eq!(narrow, ["Lock Screen"]);
    }

    #[test]
    fn list_layout_scrolls_row_by_row_and_never_switches_to_grid_box() {
        let entries: Vec<_> = (0..11)
            .map(|i| {
                Target::CustomItem(config::MenuItem {
                    label: format!("Option {i}"),
                    icon: String::new(),
                    left_click: Some(config::Action::Command(format!("echo {i}"))),
                    right_click: None,
                })
            })
            .collect();
        let mut menu = Menu::home_with(Vec::new(), config::OverflowLayout::Box);
        menu.mode = Mode::Custom;
        menu.layout = config::MenuLayout::List;
        menu.results(entries);

        assert!(menu.uses_list());
        assert!(!menu.uses_box());
        assert_eq!(menu.visible().len(), LIST_ROWS);
        assert_eq!(menu.status(), "Click to choose · Esc to go back");

        menu.scroll_list_to(3);
        assert_eq!(menu.first_visible, 3);
        assert_eq!(menu.visible()[0].label(), "Option 3");
        assert_eq!(menu.visible()[LIST_ROWS - 1].label(), "Option 8");

        menu.selected = Some(LIST_ROWS - 1);
        menu.step(1);
        assert_eq!(menu.first_visible, 4);
        assert_eq!(menu.visible()[menu.selected.unwrap()].label(), "Option 9");

        menu.turn_page(1);
        assert_eq!(menu.first_visible, 11 - LIST_ROWS);
        menu.turn_page(1);
        assert_eq!(menu.first_visible, 0);

        menu.results(Vec::new());
        assert_eq!(menu.status(), "");
    }
}
