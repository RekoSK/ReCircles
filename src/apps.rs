use gtk::{gdk, gio, prelude::*};

#[derive(Clone, Debug)]
pub struct App {
    pub id: String,
    pub name: String,
    pub icon: String,
}

impl App {
    fn from_desktop(info: gio::DesktopAppInfo) -> Option<Self> {
        if !info.should_show() || info.is_hidden() {
            return None;
        }
        Some(Self {
            id: info.id()?.to_string(),
            name: info.display_name().to_string(),
            icon: info
                .string("Icon")
                .map(|icon| icon.to_string())
                .unwrap_or_default(),
        })
    }

    pub fn launch(&self) -> Result<(), String> {
        let info = gio::DesktopAppInfo::new(&self.id)
            .ok_or_else(|| format!("{} is no longer installed.", self.name))?;
        let context = if gtk::is_initialized_main_thread() {
            gdk::Display::default().and_then(|display| display.app_launch_context())
        } else {
            None
        };
        info.launch(&[], context.as_ref())
            .map_err(|err| format!("Couldn't launch {}. {err}", self.name))
    }
}

pub fn list() -> Result<Vec<App>, String> {
    // GIO resolves XDG search paths, user overrides, localized names and desktop visibility.
    Ok(gio::AppInfo::all()
        .into_iter()
        .filter_map(|info| info.downcast::<gio::DesktopAppInfo>().ok())
        .filter_map(App::from_desktop)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn desktop_visibility_name_and_icon() {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir =
            std::env::temp_dir().join(format!("recircles-apps-{}-{nonce}", std::process::id()));
        fs::create_dir(&dir).unwrap();
        let file = dir.join("test.desktop");
        let entry =
            "[Desktop Entry]\nType=Application\nExec=/bin/true\nName=My App\nIcon=my-icon\n";
        fs::write(&file, entry).unwrap();
        let app = App::from_desktop(gio::DesktopAppInfo::from_filename(&file).unwrap()).unwrap();
        assert_eq!(app.name, "My App");
        assert_eq!(app.icon, "my-icon");
        assert_eq!(app.id, "test.desktop");
        fs::write(&file, format!("{entry}NoDisplay=true\n")).unwrap();
        assert!(App::from_desktop(gio::DesktopAppInfo::from_filename(&file).unwrap()).is_none());
        fs::remove_dir_all(dir).unwrap();
    }
}
