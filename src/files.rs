use crate::history::History;
use crate::menu::PAGE_SIZE;
use gtk::{gdk, gio, prelude::*};
use std::cmp::Reverse;
use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileEntry {
    pub path: PathBuf,
    pub is_dir: bool,
    pub is_home: bool,
    pub completion: Option<String>,
}

impl FileEntry {
    pub fn label(&self) -> String {
        if self.is_home {
            return "Home".into();
        }
        self.path
            .file_name()
            .unwrap_or(self.path.as_os_str())
            .to_string_lossy()
            .into_owned()
    }
}

fn resolve_query_path(query: &str) -> io::Result<PathBuf> {
    let input = Path::new(query);
    if input.is_absolute() {
        Ok(input.to_owned())
    } else {
        let home = env::var_os("HOME").ok_or_else(|| io::Error::other("HOME is not set"))?;
        Ok(PathBuf::from(home).join(input.strip_prefix("~").unwrap_or(input)))
    }
}

pub fn current_folder(query: &str) -> io::Result<Option<FileEntry>> {
    if query.is_empty() {
        return Ok(None);
    }
    let path = resolve_query_path(query)?;
    let directory = if path.is_dir() || query.ends_with('/') {
        path
    } else {
        path.parent()
            .ok_or_else(|| io::Error::other("No parent directory"))?
            .to_path_buf()
    };
    let directory = directory.canonicalize()?;
    if !directory.is_dir() {
        return Err(io::Error::other("Not a directory"));
    }
    Ok(Some(FileEntry {
        path: directory,
        is_dir: true,
        is_home: false,
        completion: None,
    }))
}

fn dir_entry(path: PathBuf, is_home: bool) -> FileEntry {
    let completion = path
        .to_str()
        .map(|path| format!("{}/", path.trim_end_matches('/')));
    FileEntry {
        path,
        is_dir: true,
        is_home,
        completion,
    }
}

pub fn entries(query: &str, history: &History) -> io::Result<Vec<FileEntry>> {
    if query.is_empty() {
        let home = env::var_os("HOME")
            .map(PathBuf::from)
            .filter(|path| path.is_dir());
        let home_canon = home.as_deref().and_then(|path| path.canonicalize().ok());
        let mut items: Vec<_> = history
            .path_counts()
            .filter(|(p, _)| {
                p.is_dir()
                    && home.as_deref() != Some(*p)
                    && (home_canon.is_none() || p.canonicalize().ok() != home_canon)
            })
            .collect();
        items.sort_unstable_by_key(|&(path, count)| (Reverse(count), path));
        let mut result = Vec::with_capacity(PAGE_SIZE);
        if let Some(home) = home {
            result.push(dir_entry(home, true));
        }
        let remaining = PAGE_SIZE.saturating_sub(result.len());
        result.extend(
            items
                .into_iter()
                .take(remaining)
                .map(|(path, _)| dir_entry(path.to_owned(), false)),
        );
        return Ok(result);
    }

    let path = resolve_query_path(query)?;

    // A fully typed directory displays its children immediately. A partial final
    // component filters the parent listing. Preserve all literal whitespace.
    let (directory, prefix, completion_base) = if path.is_dir() || query.ends_with('/') {
        let base = if query.ends_with('/') {
            query.to_owned()
        } else {
            format!("{query}/")
        };
        (path.as_path(), String::new(), base)
    } else {
        let parent = path
            .parent()
            .ok_or_else(|| io::Error::other("No parent directory"))?;
        let prefix = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_lowercase();
        let base = query.rfind('/').map_or("", |i| &query[..=i]);
        (parent, prefix, base.to_owned())
    };
    let directory = directory.canonicalize()?;
    let mut ranked = Vec::new();
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let filename = entry.file_name();
        let label = filename.to_string_lossy().to_lowercase();
        if (label.starts_with('.') && !prefix.starts_with('.')) || !label.starts_with(&prefix) {
            continue;
        }
        let path = entry.path();
        let Ok(metadata) = fs::metadata(&path) else {
            continue;
        };
        let is_dir = metadata.is_dir();
        if !is_dir && !metadata.is_file() {
            continue;
        }
        let count = history.path_count(&path);
        let mtime = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);
        let completion = filename.to_str().map(|name| {
            let slash = if is_dir { "/" } else { "" };
            format!("{completion_base}{name}{slash}")
        });
        let key = (!is_dir, Reverse(count), Reverse(mtime), label, filename);
        ranked.push((
            key,
            FileEntry {
                path,
                is_dir,
                is_home: false,
                completion,
            },
        ));
    }
    ranked.sort_unstable_by(|a, b| a.0.cmp(&b.0));
    Ok(ranked.into_iter().map(|(_, entry)| entry).collect())
}

/// Returns the query string for the parent folder of `query`, or `None` if already at root/empty.
pub fn parent_query(query: &str) -> Option<String> {
    let trimmed = query.trim_end_matches('/');
    if trimmed.is_empty() {
        return None;
    }
    if trimmed == "~" {
        let home = env::var_os("HOME")?;
        let parent = Path::new(&home).parent()?;
        let s = parent.to_str()?;
        return Some(if s == "/" {
            "/".into()
        } else {
            format!("{s}/")
        });
    }
    let parent = Path::new(trimmed).parent()?;
    if parent.as_os_str().is_empty() {
        return Some("~/".into());
    }
    let s = parent.to_str()?;
    Some(if s == "/" {
        "/".into()
    } else {
        format!("{s}/")
    })
}

fn choose_app(file: &gio::File, context: Option<&gdk::AppLaunchContext>) -> io::Result<bool> {
    let chooser = gtk::AppChooserDialog::new(None::<&gtk::Window>, gtk::DialogFlags::MODAL, file);
    let use_as_default = gtk::CheckButton::with_label("Use as default for this file type");
    use_as_default.set_margin_start(12);
    use_as_default.set_margin_end(12);
    use_as_default.set_margin_bottom(8);
    chooser
        .content_area()
        .pack_start(&use_as_default, false, false, 0);
    use_as_default.show();
    let response = chooser.run();
    let selected = chooser.app_info();
    let content_type = chooser.content_type();
    let persist = use_as_default.is_active();
    chooser.close();
    if response != gtk::ResponseType::Ok {
        return Ok(false);
    }
    let app =
        selected.ok_or_else(|| io::Error::other("Select an application to open this file"))?;
    app.launch(std::slice::from_ref(file), context)
        .map_err(io::Error::other)?;
    if persist {
        let ty =
            content_type.ok_or_else(|| io::Error::other("Could not determine the file type"))?;
        app.set_as_default_for_type(&ty).map_err(io::Error::other)?;
    }
    Ok(true)
}

pub fn open(entry: &FileEntry, secondary: bool, history: &mut History) -> io::Result<()> {
    let file = gio::File::for_path(&entry.path);
    let context = gdk::Display::default().and_then(|display| display.app_launch_context());
    if let Some(ctx) = &context {
        ctx.set_timestamp(gtk::current_event_time());
    }
    if secondary {
        if !choose_app(&file, context.as_ref())? {
            return Ok(());
        }
    } else {
        gio::AppInfo::launch_default_for_uri(&file.uri(), context.as_ref())
            .map_err(io::Error::other)?;
    }
    // The launch succeeded even if the state directory has since become unwritable.
    if let Err(err) = history.record_path(&entry.path) {
        eprintln!("ReCircles: could not save file usage: {err}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn literal_paths_folder_priority_and_usage_survive_restarts() {
        let base = env::temp_dir().join(format!("recircles-files-{}", std::process::id()));
        let root = base.join("Downloads and photos");
        fs::create_dir_all(&root).unwrap();
        let mut history = History::from_dir(base.join("state")).unwrap();
        let home = env::var_os("HOME")
            .map(PathBuf::from)
            .filter(|path| path.is_dir())
            .map(|path| dir_entry(path, true));
        let initial = entries("", &history).unwrap();
        assert_eq!(initial, home.iter().cloned().collect::<Vec<_>>());
        for i in 0..10 {
            let file = fs::File::create(root.join(format!("file {i}"))).unwrap();
            let modified = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(i + 1);
            file.set_modified(modified).unwrap();
        }
        for name in ["old albums", "new albums"] {
            fs::create_dir(root.join(name)).unwrap();
        }
        history.record_path(&root.join("old albums")).unwrap();
        history.record_path(&root.join("file 2")).unwrap();
        let matches = entries(base.join("Downloads and").to_str().unwrap(), &history).unwrap();
        assert_eq!(matches.len(), 1);
        assert!(matches[0].is_dir);
        let completed_path = matches[0].completion.as_deref().unwrap();
        let items = entries(completed_path, &history).unwrap();
        assert_eq!(items.len(), 12);
        assert_eq!(items[0].label(), "old albums");
        assert_eq!(items[1].label(), "new albums");
        assert_eq!(items[2].label(), "file 2");
        assert_eq!(items[3].label(), "file 9");
        assert_eq!(items[4].label(), "file 8");
        assert_eq!(items[11].label(), "file 0");
        let filtered = entries(root.join("old ").to_str().unwrap(), &history).unwrap();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].label(), "old albums");
        assert_eq!(
            filtered[0].completion.as_deref(),
            Some(format!("{}/old albums/", root.display()).as_str())
        );
        let completed_file = format!("{}/file 2", root.display());
        assert_eq!(
            items[2].completion.as_deref(),
            Some(completed_file.as_str())
        );
        let empty = entries(root.join("old albums").to_str().unwrap(), &history).unwrap();
        assert!(empty.is_empty());
        let history = History::from_dir(base.join("state")).unwrap();
        let mut expected: Vec<_> = home.into_iter().collect();
        expected.extend(filtered);
        assert_eq!(entries("", &history).unwrap(), expected);
        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn frequent_folders_caps_at_eight_entries_with_home_first() {
        let base = env::temp_dir().join(format!("recircles-frequent-{}", std::process::id()));
        fs::create_dir_all(&base).unwrap();
        let mut history = History::from_dir(base.join("state")).unwrap();
        if let Some(home) = env::var_os("HOME").map(PathBuf::from).filter(|p| p.is_dir()) {
            for _ in 0..20 {
                history.record_path(&home).unwrap();
            }
        }
        for i in 0..10 {
            let dir = base.join(format!("dir-{i:02}"));
            fs::create_dir(&dir).unwrap();
            for _ in 0..=i {
                history.record_path(&dir).unwrap();
            }
        }
        let frequent = entries("", &history).unwrap();
        assert_eq!(frequent.len(), 8);
        if let Some(home) = env::var_os("HOME").map(PathBuf::from).filter(|p| p.is_dir()) {
            assert_eq!(frequent[0], dir_entry(home, true));
            assert_eq!(frequent[0].label(), "Home");
            assert_eq!(frequent[1].label(), "dir-09");
            assert_eq!(frequent[7].label(), "dir-03");
        } else {
            assert_eq!(frequent[0].label(), "dir-09");
            assert_eq!(frequent[7].label(), "dir-02");
        }
        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn parent_query_steps_up_to_parent_folder() {
        assert_eq!(parent_query(""), None);
        assert_eq!(parent_query("/"), None);
        assert_eq!(
            parent_query("/home/example/Downloads/").as_deref(),
            Some("/home/example/")
        );
        assert_eq!(
            parent_query("/home/example/Down").as_deref(),
            Some("/home/example/")
        );
        assert_eq!(parent_query("/home/").as_deref(), Some("/"));
        assert_eq!(
            parent_query("~/Documents/Projects/").as_deref(),
            Some("~/Documents/")
        );
        assert_eq!(parent_query("~/Documents/").as_deref(), Some("~/"));
        assert_eq!(parent_query("Downloads/").as_deref(), Some("~/"));
    }

    #[test]
    fn current_folder_resolves_open_directory_and_partial_filter() {
        let base = env::temp_dir().join(format!("recircles-current-folder-{}", std::process::id()));
        let sub = base.join("Albums");
        fs::create_dir_all(&sub).unwrap();
        let file = base.join("notes.txt");
        fs::File::create(&file).unwrap();
        let canon_base = base.canonicalize().unwrap();
        let canon_sub = sub.canonicalize().unwrap();

        assert_eq!(current_folder("").unwrap(), None);
        assert_eq!(
            current_folder(&format!("{}/", base.display())).unwrap().unwrap().path,
            canon_base
        );
        assert_eq!(
            current_folder(sub.to_str().unwrap()).unwrap().unwrap().path,
            canon_sub
        );
        assert_eq!(
            current_folder(&format!("{}/Alb", base.display())).unwrap().unwrap().path,
            canon_base
        );
        assert_eq!(
            current_folder(file.to_str().unwrap()).unwrap().unwrap().path,
            canon_base
        );
        assert!(current_folder(&format!("{}/", file.display())).is_err());

        fs::remove_dir_all(base).unwrap();
    }
}
