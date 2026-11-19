use std::{
    env, fs,
    io::{Read, Write},
    os::unix::net::UnixStream,
    path::{Path, PathBuf},
    time::Duration,
};

/// Hyprland's cursor position in global logical coordinates, including negative output origins.
pub fn hyprland_position() -> Option<(f64, f64)> {
    // An inherited Wayland fd takes precedence over WAYLAND_DISPLAY and cannot be
    // matched to a socket pathname here. Use the surface's pointer event instead.
    if env::var_os("WAYLAND_SOCKET").is_some() {
        return None;
    }
    let runtime = PathBuf::from(env::var_os("XDG_RUNTIME_DIR")?);
    let signature = PathBuf::from(env::var_os("HYPRLAND_INSTANCE_SIGNATURE")?);
    if signature.file_name()? != signature.as_os_str() {
        return None;
    }
    let instance = runtime.join("hypr").join(signature);
    // The instance lock records its PID and Wayland display name on separate lines.
    // A nested compositor can inherit both the Hyprland signature and desktop name.
    let lock = fs::read_to_string(instance.join("hyprland.lock")).ok()?;
    let display = env::var_os("WAYLAND_DISPLAY").unwrap_or_else(|| "wayland-0".into());
    let socket = runtime.join(display);
    if !same_display(&runtime, &socket, &lock) {
        return None;
    }

    let mut ipc = UnixStream::connect(instance.join(".socket.sock")).ok()?;
    let timeout = Some(Duration::from_millis(70));
    ipc.set_read_timeout(timeout).ok()?;
    ipc.set_write_timeout(timeout).ok()?;
    ipc.write_all(b"/cursorpos").ok()?;
    let mut reply = String::new();
    ipc.take(128).read_to_string(&mut reply).ok()?;
    parse_position(&reply)
}

fn same_display(runtime: &Path, display: &Path, lock: &str) -> bool {
    let Some(name) = lock.lines().nth(1).filter(|line| !line.is_empty()) else {
        return false;
    };
    runtime.join(name) == display
}

fn parse_position(reply: &str) -> Option<(f64, f64)> {
    let (x, y) = reply.split_once(',')?;
    Some((
        x.trim().parse::<i32>().ok()?.into(),
        y.trim().parse::<i32>().ok()?.into(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cursor_reply_preserves_signed_coordinates() {
        assert_eq!(parse_position("-617, 43\n"), Some((-617., 43.)));
        assert_eq!(parse_position("0,0"), Some((0., 0.)));
        assert_eq!(parse_position("unknown request"), None);
        assert_eq!(parse_position("5, NaN"), None);
        assert_eq!(parse_position("4, 8, 10"), None);
    }

    #[test]
    fn nested_display_with_same_basename_does_not_match() {
        let runtime = Path::new("/run/user/1000");
        let lock = "143\nwayland-1\n";
        assert!(same_display(runtime, &runtime.join("wayland-1"), lock));
        assert!(!same_display(runtime, &runtime.join("wayland-2"), lock));
        assert!(!same_display(
            runtime,
            Path::new("/tmp/nested/wayland-1"),
            lock
        ));
    }
}
