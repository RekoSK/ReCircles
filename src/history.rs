use std::collections::BTreeMap;
use std::env;
use std::ffi::{OsStr, OsString};
use std::fs::{self, DirBuilder, File, OpenOptions};
use std::io::{self, BufWriter, Write};
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct History {
    directory: PathBuf,
    apps: BTreeMap<OsString, u64>,
    paths: BTreeMap<OsString, u64>,
}

impl History {
    fn state_dir() -> io::Result<PathBuf> {
        let state = env::var_os("XDG_STATE_HOME")
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
            .or_else(|| env::var_os("HOME").map(|p| PathBuf::from(p).join(".local/state")))
            .ok_or_else(|| io::Error::other("Set HOME or XDG_STATE_HOME for launcher history"))?;
        Ok(state.join("recircles"))
    }

    pub fn load() -> io::Result<Self> {
        Self::from_dir(Self::state_dir()?)
    }

    pub fn reset() -> io::Result<()> {
        Self::reset_dir(&Self::state_dir()?)
    }

    pub(crate) fn reset_dir(directory: &Path) -> io::Result<()> {
        match fs::metadata(directory) {
            Ok(_) => {}
            Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(err) => return Err(err),
        }

        let lock = Self::writable_options().open(directory.join("usage.lock"))?;
        lock.lock()?;
        for name in ["usage", "usage.pending"] {
            match fs::remove_file(directory.join(name)) {
                Ok(()) => {}
                Err(err) if err.kind() == io::ErrorKind::NotFound => {}
                Err(err) => return Err(err),
            }
        }
        drop(lock);

        match fs::remove_file(directory.join("usage.lock")) {
            Ok(()) => {}
            Err(err) if err.kind() == io::ErrorKind::NotFound => {}
            Err(err) => return Err(err),
        }
        let _ = fs::remove_dir(directory);
        Ok(())
    }

    fn writable_options() -> OpenOptions {
        let mut opts = OpenOptions::new();
        opts.write(true).create(true).mode(0o600);
        opts
    }

    pub(crate) fn from_dir(directory: PathBuf) -> io::Result<Self> {
        let mut history = Self {
            directory,
            apps: BTreeMap::new(),
            paths: BTreeMap::new(),
        };
        let bytes = match fs::read(history.directory.join("usage")) {
            Ok(bytes) => bytes,
            Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(history),
            Err(err) => return Err(err),
        };
        // NUL delimits records. Every other byte, including tabs and newlines, is legal in a path.
        let invalid = || io::Error::new(io::ErrorKind::InvalidData, "Invalid ReCircles history");
        for record in bytes.split_inclusive(|&b| b == 0) {
            let record = record.strip_suffix(&[0]).ok_or_else(invalid)?;
            let tab = record
                .iter()
                .position(|&b| b == b'\t')
                .ok_or_else(invalid)?;
            let value: u64 = std::str::from_utf8(&record[..tab])
                .ok()
                .and_then(|n| n.parse().ok())
                .ok_or_else(invalid)?;
            let (kind, key) = record[tab + 1..].split_first().ok_or_else(invalid)?;
            let table = match kind {
                b'a' => &mut history.apps,
                b'p' => &mut history.paths,
                _ => return Err(invalid()),
            };
            table.insert(OsString::from_vec(key.to_vec()), value);
        }
        Ok(history)
    }

    pub fn app_count(&self, id: &str) -> u64 {
        self.apps.get(OsStr::new(id)).copied().unwrap_or(0)
    }

    pub fn record_app(&mut self, id: &str) -> io::Result<()> {
        self.record(OsStr::new(id), true)
    }

    pub(crate) fn path_counts(&self) -> impl Iterator<Item = (&Path, u64)> {
        self.paths.iter().map(|(p, &n)| (Path::new(p), n))
    }

    pub(crate) fn path_count(&self, path: &Path) -> u64 {
        self.paths.get(path.as_os_str()).copied().unwrap_or(0)
    }

    pub(crate) fn record_path(&mut self, path: &Path) -> io::Result<()> {
        self.record(path.as_os_str(), false)
    }

    fn record(&mut self, key: &OsStr, app: bool) -> io::Result<()> {
        DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&self.directory)?;
        // Keep the lock inode stable across renames. Reload under lock so two launchers
        // that started with the same history never overwrite one another's choices.
        let lock = Self::writable_options().open(self.directory.join("usage.lock"))?;
        lock.lock()?;
        let mut latest = Self::from_dir(self.directory.clone())?;
        let table = if app {
            &mut latest.apps
        } else {
            &mut latest.paths
        };
        let count = table.entry(key.to_owned()).or_default();
        *count = count.saturating_add(1);

        let pending = self.directory.join("usage.pending");
        let mut writer = BufWriter::new(Self::writable_options().truncate(true).open(&pending)?);
        for (kind, table) in [('a', &latest.apps), ('p', &latest.paths)] {
            for (key, count) in table {
                write!(writer, "{count}\t{kind}")?;
                writer.write_all(key.as_bytes())?;
                writer.write_all(&[0])?;
            }
        }
        writer.flush()?;
        writer.get_ref().sync_all()?;
        fs::rename(pending, self.directory.join("usage"))?;
        File::open(&self.directory)?.sync_all()?;
        *self = latest;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stale_instances_merge_and_preserve_unix_filenames() {
        let dir = env::temp_dir().join(format!("recircles-history-{}", std::process::id()));
        let mut first = History::from_dir(dir.clone()).unwrap();
        let mut second = History::from_dir(dir.clone()).unwrap();
        let path = Path::new(OsStr::from_bytes(b"/folder /tab\tnewline\n\xff"));
        first.record_path(path).unwrap();
        second.record_app("app.desktop").unwrap();
        second.record_path(path).unwrap();
        first.record_app("app.desktop").unwrap();
        let stored = History::from_dir(dir.clone()).unwrap();
        assert_eq!(stored.path_count(path), 2);
        assert_eq!(stored.app_count("app.desktop"), 2);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn reset_clears_saved_history_and_handles_missing_dir() {
        let dir = env::temp_dir().join(format!("recircles-reset-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        History::reset_dir(&dir).unwrap();

        let mut history = History::from_dir(dir.clone()).unwrap();
        let path = Path::new("/tmp/example");
        history.record_app("app.desktop").unwrap();
        history.record_path(path).unwrap();

        History::reset_dir(&dir).unwrap();
        assert!(!dir.exists());

        let reloaded = History::from_dir(dir).unwrap();
        assert_eq!(reloaded.app_count("app.desktop"), 0);
        assert_eq!(reloaded.path_count(path), 0);
    }
}
