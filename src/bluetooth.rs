use crate::ring::Icon;
use gtk::prelude::*;
use std::collections::{HashMap, HashSet};
use std::io::{Read, Write};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct AdapterState {
    pub available: bool,
    pub powered: bool,
    pub discoverable: bool,
    pub pairable: bool,
    pub discovering: bool,
    pub alias: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BluetoothDevice {
    pub mac: String,
    pub name: String,
    pub icon: String,
    pub paired: bool,
    pub trusted: bool,
    pub blocked: bool,
    pub connected: bool,
    pub battery: Option<u8>,
}

impl BluetoothDevice {
    pub fn display_name(&self) -> &str {
        if self.name.trim().is_empty() {
            &self.mac
        } else {
            &self.name
        }
    }

    pub fn detail(&self) -> String {
        let mut parts = Vec::new();
        if self.connected {
            parts.push("Connected".to_string());
        } else if self.blocked {
            parts.push("Blocked".to_string());
        } else if self.paired {
            parts.push("Paired".to_string());
        } else {
            parts.push("Available".to_string());
        }
        if let Some(pct) = self.battery {
            parts.push(format!("{pct}%"));
        }
        if self.trusted {
            parts.push("Trusted".to_string());
        }
        if !self.paired || parts.len() == 1 {
            parts.push(self.mac.clone());
        }
        parts.join(" · ")
    }

    fn is_mac_only(&self) -> bool {
        let name = self.name.trim();
        name.is_empty() || name.eq_ignore_ascii_case(&self.mac) || name.replace('-', ":").eq_ignore_ascii_case(&self.mac)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BluetoothState {
    pub adapter: AdapterState,
    pub devices: Vec<BluetoothDevice>,
}

impl BluetoothState {
    pub fn find_device(&self, mac: &str) -> Option<&BluetoothDevice> {
        self.devices
            .iter()
            .find(|dev| dev.mac.eq_ignore_ascii_case(mac))
    }

    pub fn connected_count(&self) -> usize {
        self.devices.iter().filter(|dev| dev.connected).count()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BluetoothView {
    Root,
    Settings,
    Device { mac: String, name: String },
}

impl BluetoothView {
    pub fn title(&self) -> String {
        match self {
            Self::Root => "Bluetooth".into(),
            Self::Settings => "Bluetooth Settings".into(),
            Self::Device { name, mac } => {
                if !name.trim().is_empty() {
                    name.clone()
                } else {
                    mac.clone()
                }
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BluetoothAction {
    OpenView(BluetoothView),
    SetPower(bool),
    Scan,
    SetDiscoverable(bool),
    SetPairable(bool),
    SetAlias,
    Connect { mac: String, name: String },
    Disconnect { mac: String, name: String },
    DisconnectAll,
    PairAndConnect { mac: String, name: String },
    PairOnly { mac: String, name: String },
    SetTrusted { mac: String, name: String, trusted: bool },
    SetBlocked { mac: String, name: String, blocked: bool },
    Remove { mac: String, name: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BluetoothItem {
    pub label: String,
    pub detail: String,
    pub icon: Icon,
    pub theme_icon: String,
    pub active: bool,
    pub action: BluetoothAction,
    pub secondary_action: Option<BluetoothAction>,
}

pub enum PairPrompt {
    Status(String),
    ConfirmPasskey {
        device: String,
        passkey: String,
        reply: mpsc::Sender<bool>,
    },
    RequestPin {
        device: String,
        prompt: String,
        reply: mpsc::Sender<Option<String>>,
    },
}

pub fn build_items(state: &BluetoothState, view: &BluetoothView) -> Vec<BluetoothItem> {
    match view {
        BluetoothView::Root => build_root_items(state),
        BluetoothView::Settings => build_settings_items(state),
        BluetoothView::Device { mac, name } => build_device_items(state, mac, name),
    }
}

pub fn idle_status(state: &BluetoothState, view: &BluetoothView) -> String {
    match view {
        BluetoothView::Root => {
            if !state.adapter.available {
                "No Bluetooth adapter found".into()
            } else if !state.adapter.powered {
                "Bluetooth is turned off".into()
            } else {
                let connected = state.connected_count();
                if connected > 0 {
                    format!("{connected} connected · Right-click to manage")
                } else if state.devices.is_empty() {
                    "No devices found · Select Scan".into()
                } else {
                    "Click to connect · Right-click to manage".into()
                }
            }
        }
        BluetoothView::Settings => {
            if !state.adapter.available {
                "No Bluetooth adapter found".into()
            } else if state.adapter.powered {
                if state.adapter.alias.is_empty() {
                    "Bluetooth is on".into()
                } else {
                    format!("Adapter: {}", state.adapter.alias)
                }
            } else {
                "Bluetooth is off".into()
            }
        }
        BluetoothView::Device { mac, .. } => state
            .find_device(mac)
            .map(BluetoothDevice::detail)
            .unwrap_or_else(|| mac.clone()),
    }
}

fn build_root_items(state: &BluetoothState) -> Vec<BluetoothItem> {
    if !state.adapter.available {
        return Vec::new();
    }
    let mut items = Vec::new();
    if !state.adapter.powered {
        items.push(BluetoothItem {
            label: "Turn Bluetooth On".into(),
            detail: "Power on the Bluetooth adapter".into(),
            icon: Icon::ToggleOff,
            theme_icon: String::new(),
            active: false,
            action: BluetoothAction::SetPower(true),
            secondary_action: None,
        });
        items.push(BluetoothItem {
            label: "Settings".into(),
            detail: "Adapter power, visibility and name".into(),
            icon: Icon::Settings,
            theme_icon: String::new(),
            active: false,
            action: BluetoothAction::OpenView(BluetoothView::Settings),
            secondary_action: None,
        });
        return items;
    }

    items.push(BluetoothItem {
        label: "Scan".into(),
        detail: "Discover nearby Bluetooth devices".into(),
        icon: Icon::Scan,
        theme_icon: String::new(),
        active: state.adapter.discovering,
        action: BluetoothAction::Scan,
        secondary_action: None,
    });
    items.push(BluetoothItem {
        label: "Settings".into(),
        detail: "Power, discoverable, pairable & adapter name".into(),
        icon: Icon::Settings,
        theme_icon: String::new(),
        active: false,
        action: BluetoothAction::OpenView(BluetoothView::Settings),
        secondary_action: None,
    });

    for dev in &state.devices {
        let name = dev.display_name().to_owned();
        let manage_view = BluetoothView::Device {
            mac: dev.mac.clone(),
            name: name.clone(),
        };
        let primary = if dev.connected || dev.blocked {
            BluetoothAction::OpenView(manage_view.clone())
        } else if dev.paired {
            BluetoothAction::Connect {
                mac: dev.mac.clone(),
                name: name.clone(),
            }
        } else {
            BluetoothAction::PairAndConnect {
                mac: dev.mac.clone(),
                name: name.clone(),
            }
        };
        let icon = if dev.connected {
            Icon::BluetoothConnected
        } else if dev.blocked {
            Icon::BluetoothOff
        } else {
            Icon::BluetoothDevice
        };
        items.push(BluetoothItem {
            label: name,
            detail: dev.detail(),
            icon,
            theme_icon: dev.icon.clone(),
            active: dev.connected,
            action: primary,
            secondary_action: Some(BluetoothAction::OpenView(manage_view)),
        });
    }

    items
}

fn build_settings_items(state: &BluetoothState) -> Vec<BluetoothItem> {
    let mut items = Vec::new();
    let powered = state.adapter.powered;
    items.push(BluetoothItem {
        label: if powered {
            "Bluetooth: On".into()
        } else {
            "Bluetooth: Off".into()
        },
        detail: if powered {
            "Click to turn Bluetooth off".into()
        } else {
            "Click to turn Bluetooth on".into()
        },
        icon: if powered {
            Icon::ToggleOn
        } else {
            Icon::ToggleOff
        },
        theme_icon: String::new(),
        active: powered,
        action: BluetoothAction::SetPower(!powered),
        secondary_action: None,
    });

    if powered {
        items.push(BluetoothItem {
            label: "Scan for Devices".into(),
            detail: "Scan for nearby pairable devices (6s)".into(),
            icon: Icon::Scan,
            theme_icon: String::new(),
            active: state.adapter.discovering,
            action: BluetoothAction::Scan,
            secondary_action: None,
        });
        items.push(BluetoothItem {
            label: if state.adapter.discoverable {
                "Discoverable: On".into()
            } else {
                "Discoverable: Off".into()
            },
            detail: if state.adapter.discoverable {
                "Visible to nearby Bluetooth devices".into()
            } else {
                "Hidden from nearby Bluetooth scans".into()
            },
            icon: if state.adapter.discoverable {
                Icon::ToggleOn
            } else {
                Icon::ToggleOff
            },
            theme_icon: String::new(),
            active: state.adapter.discoverable,
            action: BluetoothAction::SetDiscoverable(!state.adapter.discoverable),
            secondary_action: None,
        });
        items.push(BluetoothItem {
            label: if state.adapter.pairable {
                "Pairable: On".into()
            } else {
                "Pairable: Off".into()
            },
            detail: if state.adapter.pairable {
                "Accept incoming pairing requests".into()
            } else {
                "Reject incoming pairing requests".into()
            },
            icon: if state.adapter.pairable {
                Icon::ToggleOn
            } else {
                Icon::ToggleOff
            },
            theme_icon: String::new(),
            active: state.adapter.pairable,
            action: BluetoothAction::SetPairable(!state.adapter.pairable),
            secondary_action: None,
        });
        items.push(BluetoothItem {
            label: "Adapter Name…".into(),
            detail: if state.adapter.alias.is_empty() {
                "Set this computer's Bluetooth name".into()
            } else {
                format!("Current name: {}", state.adapter.alias)
            },
            icon: Icon::Action,
            theme_icon: String::new(),
            active: false,
            action: BluetoothAction::SetAlias,
            secondary_action: None,
        });
        let connected = state.connected_count();
        if connected > 0 {
            items.push(BluetoothItem {
                label: "Disconnect All".into(),
                detail: format!("Disconnect {connected} active device(s)"),
                icon: Icon::Disconnect,
                theme_icon: String::new(),
                active: false,
                action: BluetoothAction::DisconnectAll,
                secondary_action: None,
            });
        }
    }

    items
}

fn build_device_items(state: &BluetoothState, mac: &str, fallback_name: &str) -> Vec<BluetoothItem> {
    let dev = state.find_device(mac);
    let name = dev
        .map(|d| d.display_name().to_owned())
        .unwrap_or_else(|| fallback_name.to_owned());
    let connected = dev.is_some_and(|d| d.connected);
    let paired = dev.is_some_and(|d| d.paired);
    let trusted = dev.is_some_and(|d| d.trusted);
    let blocked = dev.is_some_and(|d| d.blocked);

    let mut items = Vec::new();
    if connected {
        items.push(BluetoothItem {
            label: "Disconnect".into(),
            detail: format!("Disconnect {name}"),
            icon: Icon::Disconnect,
            theme_icon: String::new(),
            active: false,
            action: BluetoothAction::Disconnect {
                mac: mac.to_owned(),
                name: name.clone(),
            },
            secondary_action: None,
        });
    } else if paired {
        items.push(BluetoothItem {
            label: "Connect".into(),
            detail: format!("Connect to {name}"),
            icon: Icon::BluetoothConnected,
            theme_icon: String::new(),
            active: false,
            action: BluetoothAction::Connect {
                mac: mac.to_owned(),
                name: name.clone(),
            },
            secondary_action: None,
        });
    } else {
        items.push(BluetoothItem {
            label: "Pair & Connect".into(),
            detail: format!("Pair, trust and connect to {name}"),
            icon: Icon::BluetoothConnected,
            theme_icon: String::new(),
            active: false,
            action: BluetoothAction::PairAndConnect {
                mac: mac.to_owned(),
                name: name.clone(),
            },
            secondary_action: None,
        });
        items.push(BluetoothItem {
            label: "Pair Only".into(),
            detail: format!("Pair with {name} without connecting"),
            icon: Icon::Add,
            theme_icon: String::new(),
            active: false,
            action: BluetoothAction::PairOnly {
                mac: mac.to_owned(),
                name: name.clone(),
            },
            secondary_action: None,
        });
    }

    items.push(BluetoothItem {
        label: if trusted {
            "Trusted: On".into()
        } else {
            "Trusted: Off".into()
        },
        detail: if trusted {
            "Allow automatic reconnection from this device".into()
        } else {
            "Mark device as trusted for auto-reconnect".into()
        },
        icon: if trusted {
            Icon::ToggleOn
        } else {
            Icon::ToggleOff
        },
        theme_icon: String::new(),
        active: trusted,
        action: BluetoothAction::SetTrusted {
            mac: mac.to_owned(),
            name: name.clone(),
            trusted: !trusted,
        },
        secondary_action: None,
    });

    items.push(BluetoothItem {
        label: if blocked {
            "Unblock Device".into()
        } else {
            "Block Device".into()
        },
        detail: if blocked {
            format!("Allow connections from {name}")
        } else {
            format!("Reject connections from {name}")
        },
        icon: if blocked {
            Icon::ToggleOn
        } else {
            Icon::BluetoothOff
        },
        theme_icon: String::new(),
        active: blocked,
        action: BluetoothAction::SetBlocked {
            mac: mac.to_owned(),
            name: name.clone(),
            blocked: !blocked,
        },
        secondary_action: None,
    });

    items.push(BluetoothItem {
        label: if paired {
            "Forget Device".into()
        } else {
            "Remove Device".into()
        },
        detail: format!("Remove {name} ({mac}) from BlueZ"),
        icon: Icon::Forget,
        theme_icon: String::new(),
        active: false,
        action: BluetoothAction::Remove {
            mac: mac.to_owned(),
            name,
        },
        secondary_action: None,
    });

    items
}

pub fn load_state() -> Result<BluetoothState, String> {
    let show_out = match run_bluetoothctl(&["show"]) {
        Ok(out) => out,
        Err(err) if err.to_ascii_lowercase().contains("no default controller") => {
            return Ok(BluetoothState {
                adapter: AdapterState::default(),
                devices: Vec::new(),
            });
        }
        Err(err) => return Err(err),
    };
    let adapter = parse_adapter_show(&show_out);
    if !adapter.available || !adapter.powered {
        return Ok(BluetoothState {
            adapter,
            devices: Vec::new(),
        });
    }

    let (all_out, connected_out, paired_out, trusted_out) = std::thread::scope(|scope| {
        let h_all = scope.spawn(|| run_bluetoothctl(&["devices"]).unwrap_or_default());
        let h_conn = scope.spawn(|| run_bluetoothctl(&["devices", "Connected"]).unwrap_or_default());
        let h_paired = scope.spawn(|| run_bluetoothctl(&["devices", "Paired"]).unwrap_or_default());
        let h_trusted =
            scope.spawn(|| run_bluetoothctl(&["devices", "Trusted"]).unwrap_or_default());
        (
            h_all.join().unwrap_or_default(),
            h_conn.join().unwrap_or_default(),
            h_paired.join().unwrap_or_default(),
            h_trusted.join().unwrap_or_default(),
        )
    });

    let connected_macs: HashSet<String> = parse_device_list(&connected_out)
        .into_iter()
        .map(|(mac, _)| mac)
        .collect();
    let paired_macs: HashSet<String> = parse_device_list(&paired_out)
        .into_iter()
        .map(|(mac, _)| mac)
        .collect();
    let trusted_macs: HashSet<String> = parse_device_list(&trusted_out)
        .into_iter()
        .map(|(mac, _)| mac)
        .collect();

    let mut base_devices = Vec::new();
    let mut seen = HashSet::new();
    for (mac, name) in parse_device_list(&all_out) {
        if !seen.insert(mac.clone()) {
            continue;
        }
        let connected = connected_macs.contains(&mac);
        let paired = paired_macs.contains(&mac);
        let trusted = trusted_macs.contains(&mac);
        base_devices.push(BluetoothDevice {
            mac,
            name,
            icon: String::new(),
            paired,
            trusted,
            blocked: false,
            connected,
            battery: None,
        });
    }

    // Fetch rich info (icon, battery, blocked state, alias) for paired/connected devices in parallel.
    let info_map: HashMap<String, BluetoothDevice> = std::thread::scope(|scope| {
        let handles: Vec<_> = base_devices
            .iter()
            .filter(|d| d.connected || d.paired)
            .take(16)
            .map(|d| {
                let mac = d.mac.clone();
                scope.spawn(move || {
                    run_bluetoothctl(&["info", &mac])
                        .ok()
                        .and_then(|out| parse_device_info(&mac, &out))
                        .map(|dev| (mac, dev))
                })
            })
            .collect();
        handles
            .into_iter()
            .filter_map(|h| h.join().ok().flatten())
            .collect()
    });

    let mut devices: Vec<BluetoothDevice> = base_devices
        .into_iter()
        .map(|base| {
            if let Some(mut rich) = info_map.get(&base.mac).cloned() {
                if rich.name.is_empty() {
                    rich.name = base.name;
                }
                rich
            } else {
                base
            }
        })
        .collect();

    sort_devices(&mut devices);
    Ok(BluetoothState { adapter, devices })
}

pub fn refresh_device(mac: &str) -> Result<Option<BluetoothDevice>, String> {
    let out = run_bluetoothctl(&["info", mac])?;
    Ok(parse_device_info(mac, &out))
}

pub fn set_power(enable: bool) -> Result<(), String> {
    let arg = if enable { "on" } else { "off" };
    match run_bluetoothctl(&["power", arg]) {
        Ok(_) => Ok(()),
        Err(err) if enable && err.to_ascii_lowercase().contains("blocked") => {
            let _ = Command::new("rfkill")
                .args(["unblock", "bluetooth"])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
            run_bluetoothctl(&["power", "on"]).map(|_| ())
        }
        Err(err) => Err(err),
    }
}

pub fn scan(duration_secs: u64) -> Result<(), String> {
    let secs = duration_secs.clamp(2, 15);
    let timeout_arg = secs.to_string();
    let output = run_command_with_timeout(
        "bluetoothctl",
        &["--timeout", &timeout_arg, "scan", "on"],
        Duration::from_secs(secs + 4),
    )?;
    let combined = format!(
        "{}\n{}",
        strip_ansi(&output.stdout),
        strip_ansi(&output.stderr)
    );
    if combined.contains("Failed to start discovery") {
        let detail = combined
            .lines()
            .map(str::trim)
            .find(|l| l.contains("Failed to start discovery"))
            .unwrap_or("Failed to start Bluetooth discovery.");
        return Err(detail.to_owned());
    }
    Ok(())
}

pub fn set_discoverable(enable: bool) -> Result<(), String> {
    run_bluetoothctl(&["discoverable", if enable { "on" } else { "off" }]).map(|_| ())
}

pub fn set_pairable(enable: bool) -> Result<(), String> {
    run_bluetoothctl(&["pairable", if enable { "on" } else { "off" }]).map(|_| ())
}

pub fn set_alias(alias: &str) -> Result<(), String> {
    run_bluetoothctl(&["system-alias", alias]).map(|_| ())
}

pub fn connect(mac: &str) -> Result<(), String> {
    let out = run_bluetoothctl_timeout(15, &["connect", mac])?;
    if out.contains("Failed to connect") {
        return Err(extract_bt_error(&out, "Could not connect to device."));
    }
    Ok(())
}

pub fn disconnect(mac: &str) -> Result<(), String> {
    let out = run_bluetoothctl_timeout(10, &["disconnect", mac])?;
    if out.contains("Failed to disconnect") {
        return Err(extract_bt_error(&out, "Could not disconnect device."));
    }
    Ok(())
}

pub fn disconnect_all(devices: &[BluetoothDevice]) -> Result<(), String> {
    let mut first_err = None;
    for dev in devices.iter().filter(|d| d.connected) {
        if let Err(err) = disconnect(&dev.mac)
            && first_err.is_none()
        {
            first_err = Some(err);
        }
    }
    match first_err {
        Some(err) => Err(err),
        None => Ok(()),
    }
}

pub fn set_trusted(mac: &str, trusted: bool) -> Result<(), String> {
    run_bluetoothctl(&[if trusted { "trust" } else { "untrust" }, mac]).map(|_| ())
}

pub fn set_blocked(mac: &str, blocked: bool) -> Result<(), String> {
    run_bluetoothctl(&[if blocked { "block" } else { "unblock" }, mac]).map(|_| ())
}

pub fn remove(mac: &str) -> Result<(), String> {
    run_bluetoothctl(&["remove", mac]).map(|_| ())
}

/// Pairs with a Bluetooth device using an interactive `bluetoothctl` agent so
/// Just Works, Passkey confirmation, PIN entry, and keyboard passkey display work
/// even when no external desktop Bluetooth agent is running.
pub fn pair_interactive(
    mac: &str,
    device_name: &str,
    prompts: &mpsc::Sender<PairPrompt>,
) -> Result<(), String> {
    let mut child = Command::new("stdbuf")
        .args(["-o0", "bluetoothctl", "--agent", "KeyboardDisplay"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .or_else(|_| {
            Command::new("bluetoothctl")
                .args(["--agent", "KeyboardDisplay"])
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
        })
        .map_err(bluetoothctl_spawn_err)?;

    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| "Could not open stdin for bluetoothctl.".to_string())?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "Could not open stdout for bluetoothctl.".to_string())?;
    let stderr = child.stderr.take();

    // Stream stdout and stderr chunks on helper threads so pipes never block
    // and the pairing loop can enforce a timeout.
    let (chunk_tx, chunk_rx) = mpsc::channel::<String>();
    let tx_out = chunk_tx.clone();
    std::thread::spawn(move || {
        let mut reader = stdout;
        let mut buf = [0u8; 1024];
        loop {
            match reader.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    let text = String::from_utf8_lossy(&buf[..n]).into_owned();
                    if tx_out.send(text).is_err() {
                        break;
                    }
                }
            }
        }
    });
    if let Some(mut err_reader) = stderr {
        std::thread::spawn(move || {
            let mut buf = [0u8; 1024];
            loop {
                match err_reader.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        let text = String::from_utf8_lossy(&buf[..n]).into_owned();
                        if chunk_tx.send(text).is_err() {
                            break;
                        }
                    }
                }
            }
        });
    } else {
        drop(chunk_tx);
    }

    // Wait briefly for bluetoothctl to connect to D-Bus and register its agent
    // before issuing `pair <mac>`, avoiding premature "Device not available" races.
    let ready_deadline = Instant::now() + Duration::from_millis(1500);
    let mut startup = String::new();
    while Instant::now() < ready_deadline {
        let wait = ready_deadline
            .saturating_duration_since(Instant::now())
            .min(Duration::from_millis(100));
        match chunk_rx.recv_timeout(wait) {
            Ok(chunk) => {
                startup.push_str(&strip_ansi(&chunk));
                if startup.contains("Agent registered") {
                    std::thread::sleep(Duration::from_millis(200));
                    break;
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }

    let _ = writeln!(stdin, "default-agent");
    let _ = stdin.flush();
    std::thread::sleep(Duration::from_millis(150));
    let _ = writeln!(stdin, "pair {mac}");
    let _ = stdin.flush();

    let deadline = Instant::now() + Duration::from_secs(45);
    let mut buffer = String::new();
    let mut handled_passkey = false;
    let mut result = Err("Pairing timed out.".to_string());

    while Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(Instant::now());
        let wait = remaining.min(Duration::from_millis(250));
        match chunk_rx.recv_timeout(wait) {
            Ok(chunk) => {
                buffer.push_str(&strip_ansi(&chunk));

                if let Some(err) = check_pair_outcome(&buffer) {
                    result = err;
                    break;
                }

                if let Some(passkey) = detect_confirm_passkey(&buffer) {
                    buffer.clear();
                    let (reply_tx, reply_rx) = mpsc::channel();
                    let _ = prompts.send(PairPrompt::ConfirmPasskey {
                        device: device_name.to_owned(),
                        passkey,
                        reply: reply_tx,
                    });
                    let accepted = reply_rx.recv().unwrap_or(false);
                    let _ = writeln!(stdin, "{}", if accepted { "yes" } else { "no" });
                    let _ = stdin.flush();
                    continue;
                }

                if let Some(prompt_text) = detect_pin_prompt(&buffer) {
                    buffer.clear();
                    let (reply_tx, reply_rx) = mpsc::channel();
                    let _ = prompts.send(PairPrompt::RequestPin {
                        device: device_name.to_owned(),
                        prompt: prompt_text,
                        reply: reply_tx,
                    });
                    if let Ok(Some(pin)) = reply_rx.recv() {
                        let _ = writeln!(stdin, "{pin}");
                    } else {
                        let _ = writeln!(stdin, "cancel-pairing {mac}");
                    }
                    let _ = stdin.flush();
                    continue;
                }

                if !handled_passkey
                    && let Some(code) = detect_display_passkey(&buffer)
                {
                    handled_passkey = true;
                    let _ = prompts.send(PairPrompt::Status(format!(
                        "Type passkey {code} on {device_name} and press Enter"
                    )));
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if let Ok(Some(_)) = child.try_wait() {
                    if let Some(outcome) = check_pair_outcome(&buffer) {
                        result = outcome;
                    }
                    break;
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                if let Some(outcome) = check_pair_outcome(&buffer) {
                    result = outcome;
                }
                break;
            }
        }
    }

    let _ = writeln!(stdin, "quit");
    let _ = stdin.flush();
    drop(stdin);
    let _ = child.kill();
    let _ = child.wait();
    result
}

pub fn prompt_confirm_passkey(device_name: &str, passkey: &str) -> bool {
    let dialog = gtk::MessageDialog::new(
        None::<&gtk::Window>,
        gtk::DialogFlags::MODAL,
        gtk::MessageType::Question,
        gtk::ButtonsType::YesNo,
        "Confirm Bluetooth Pairing",
    );
    dialog.set_secondary_text(Some(&format!(
        "Does \"{device_name}\" show passkey {passkey}?"
    )));
    dialog.set_default_response(gtk::ResponseType::Yes);
    let response = dialog.run();
    dialog.close();
    response == gtk::ResponseType::Yes
}

fn check_pair_outcome(buffer: &str) -> Option<Result<(), String>> {
    if buffer.contains("Pairing successful") || buffer.contains("Already Exists") {
        return Some(Ok(()));
    }
    for line in buffer.lines() {
        let trimmed = line.trim();
        if trimmed.contains("Failed to pair")
            || trimmed.contains("AuthenticationFailed")
            || trimmed.contains("AuthenticationCanceled")
            || trimmed.contains("AuthenticationRejected")
            || trimmed.contains("not available")
        {
            return Some(Err(extract_bt_error(trimmed, "Bluetooth pairing failed.")));
        }
    }
    None
}

fn detect_confirm_passkey(buffer: &str) -> Option<String> {
    if !buffer.contains("(yes/no)") {
        return None;
    }
    for line in buffer.lines().rev() {
        if let Some(idx) = line.find("Confirm passkey") {
            let rest = &line[idx + "Confirm passkey".len()..];
            if let Some(code) = rest.split_whitespace().next() {
                return Some(code.to_owned());
            }
        }
        if line.contains("Authorize service") {
            return Some("service authorization".into());
        }
    }
    None
}

fn detect_pin_prompt(buffer: &str) -> Option<String> {
    for line in buffer.lines().rev() {
        let trimmed = line.trim();
        if trimmed.contains("Enter PIN code:") {
            return Some("Enter PIN code:".into());
        }
        if trimmed.contains("Enter passkey") {
            return Some("Enter passkey:".into());
        }
    }
    None
}

fn detect_display_passkey(buffer: &str) -> Option<String> {
    for line in buffer.lines().rev() {
        if let Some(idx) = line.find("Passkey:") {
            let code = line[idx + "Passkey:".len()..].trim();
            if !code.is_empty() && code.chars().all(|c| c.is_ascii_digit()) {
                return Some(code.to_owned());
            }
        }
    }
    None
}

struct CmdOutput {
    success: bool,
    stdout: String,
    stderr: String,
}

fn run_command_with_timeout(
    program: &str,
    args: &[&str],
    timeout: Duration,
) -> Result<CmdOutput, String> {
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(bluetoothctl_spawn_err)?;

    let mut stdout_pipe = child.stdout.take();
    let mut stderr_pipe = child.stderr.take();

    let out_handle = std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(ref mut pipe) = stdout_pipe {
            let _ = pipe.read_to_end(&mut buf);
        }
        String::from_utf8_lossy(&buf).into_owned()
    });
    let err_handle = std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(ref mut pipe) = stderr_pipe {
            let _ = pipe.read_to_end(&mut buf);
        }
        String::from_utf8_lossy(&buf).into_owned()
    });

    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    break None;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(err) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("Could not wait for {program}: {err}"));
            }
        }
    };

    let stdout = out_handle.join().unwrap_or_default();
    let stderr = err_handle.join().unwrap_or_default();

    let Some(status) = status else {
        return Err("Bluetooth operation timed out.".into());
    };
    Ok(CmdOutput {
        success: status.success(),
        stdout,
        stderr,
    })
}

fn run_bluetoothctl(args: &[&str]) -> Result<String, String> {
    run_bluetoothctl_timeout(6, args)
}

fn run_bluetoothctl_timeout(timeout_secs: u64, args: &[&str]) -> Result<String, String> {
    // Do NOT pass `--timeout` to bluetoothctl for one-shot commands: BlueZ's
    // `--timeout` waits the entire duration instead of exiting on completion.
    let output = run_command_with_timeout("bluetoothctl", args, Duration::from_secs(timeout_secs))?;
    let stdout = strip_ansi(&output.stdout);
    let stderr = strip_ansi(&output.stderr);
    if output.success {
        if stdout.contains("No default controller available")
            || stderr.contains("No default controller available")
        {
            return Err("No default controller available.".into());
        }
        Ok(stdout)
    } else {
        let combined = format!("{stderr}\n{stdout}");
        Err(extract_bt_error(
            &combined,
            "Bluetooth service (bluetoothd) is unavailable.",
        ))
    }
}

fn bluetoothctl_spawn_err(err: std::io::Error) -> String {
    if err.kind() == std::io::ErrorKind::NotFound {
        "BlueZ (bluetoothctl) is not installed.".into()
    } else {
        format!("Could not run bluetoothctl: {err}")
    }
}

fn extract_bt_error(text: &str, fallback: &str) -> String {
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('[') && trimmed.ends_with('#') {
            continue;
        }
        if let Some((_, rest)) = trimmed.split_once("org.bluez.Error.") {
            return format!("Bluetooth error: {rest}");
        }
        if trimmed.starts_with("Failed") || trimmed.contains("No default controller") {
            return trimmed.to_owned();
        }
    }
    text.lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or(fallback)
        .to_owned()
}

/// Strips ANSI CSI/OSC escape sequences and carriage returns from `bluetoothctl` output.
pub fn strip_ansi(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\x1b' {
            match chars.peek().copied() {
                Some('[') => {
                    chars.next();
                    for c in chars.by_ref() {
                        if ('@'..='~').contains(&c) {
                            break;
                        }
                    }
                }
                Some(']') => {
                    chars.next();
                    for c in chars.by_ref() {
                        if c == '\x07' {
                            break;
                        }
                    }
                }
                _ => {}
            }
        } else if ch != '\r' && ch != '\x01' && ch != '\x02' {
            out.push(ch);
        }
    }
    out
}

fn parse_adapter_show(output: &str) -> AdapterState {
    let mut state = AdapterState::default();
    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("Controller ") {
            state.available = true;
            continue;
        }
        if let Some((key, value)) = trimmed.split_once(':') {
            let val = value.trim();
            match key.trim() {
                "Alias" => state.alias = val.to_owned(),
                "Name" if state.alias.is_empty() => state.alias = val.to_owned(),
                "Powered" => state.powered = val.eq_ignore_ascii_case("yes"),
                "Discoverable" => state.discoverable = val.eq_ignore_ascii_case("yes"),
                "Pairable" => state.pairable = val.eq_ignore_ascii_case("yes"),
                "Discovering" => state.discovering = val.eq_ignore_ascii_case("yes"),
                _ => {}
            }
        }
    }
    state
}

fn parse_device_list(output: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for line in output.lines() {
        let trimmed = line.trim();
        let Some(rest) = trimmed.strip_prefix("Device ") else {
            continue;
        };
        let mut parts = rest.splitn(2, ' ');
        let Some(mac) = parts.next() else {
            continue;
        };
        if !looks_like_mac(mac) {
            continue;
        }
        let name = parts.next().unwrap_or("").trim().to_owned();
        out.push((mac.to_ascii_uppercase(), name));
    }
    out
}

fn looks_like_mac(s: &str) -> bool {
    let bytes = s.as_bytes();
    bytes.len() == 17
        && bytes.iter().enumerate().all(|(i, &b)| {
            if i % 3 == 2 {
                b == b':'
            } else {
                b.is_ascii_hexdigit()
            }
        })
}

fn parse_device_info(mac: &str, output: &str) -> Option<BluetoothDevice> {
    let mut found = false;
    let mut fields: HashMap<&str, &str> = HashMap::new();
    let mut battery = None;
    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("Device ") {
            found = true;
            continue;
        }
        if let Some((key, value)) = trimmed.split_once(':') {
            let key = key.trim();
            let val = value.trim();
            if key == "Battery Percentage" {
                battery = parse_battery_percentage(val);
            } else {
                fields.insert(key, val);
            }
        }
    }
    if !found {
        return None;
    }
    let name = fields
        .get("Alias")
        .or_else(|| fields.get("Name"))
        .copied()
        .unwrap_or("")
        .to_owned();
    let yes = |k: &str| fields.get(k).is_some_and(|v| v.eq_ignore_ascii_case("yes"));
    Some(BluetoothDevice {
        mac: mac.to_ascii_uppercase(),
        name,
        icon: fields.get("Icon").copied().unwrap_or("").to_owned(),
        paired: yes("Paired"),
        trusted: yes("Trusted"),
        blocked: yes("Blocked"),
        connected: yes("Connected"),
        battery,
    })
}

fn parse_battery_percentage(value: &str) -> Option<u8> {
    if let Some(start) = value.find('(')
        && let Some(end) = value[start + 1..].find(')')
        && let Ok(pct) = value[start + 1..start + 1 + end].trim().parse::<u8>()
    {
        return Some(pct.min(100));
    }
    value.trim().parse::<u8>().ok().map(|p| p.min(100))
}

fn sort_devices(devices: &mut [BluetoothDevice]) {
    devices.sort_by_cached_key(|d| {
        (
            !d.connected,
            !d.paired,
            d.blocked,
            d.is_mac_only(),
            d.display_name().to_lowercase(),
            d.mac.clone(),
        )
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_ansi_sequences_from_bluetoothctl_output() {
        let raw = "\x1b[0;94m[bluetooth]\x1b[0m# \x1b[KDevice AA:BB:CC:DD:EE:FF Headphones\r\n";
        assert_eq!(
            strip_ansi(raw),
            "[bluetooth]# Device AA:BB:CC:DD:EE:FF Headphones\n"
        );
    }

    #[test]
    fn parses_adapter_state_and_device_info() {
        let show = "\
Controller 10:20:30:40:50:60 (public)
\tName: arch-laptop
\tAlias: Test Laptop
\tPowered: yes
\tDiscoverable: no
\tPairable: yes
\tDiscovering: no
";
        let adapter = parse_adapter_show(show);
        assert!(adapter.available);
        assert!(adapter.powered);
        assert!(!adapter.discoverable);
        assert!(adapter.pairable);
        assert_eq!(adapter.alias, "Test Laptop");

        let info = "\
Device AA:BB:CC:DD:EE:FF (public)
\tName: WH-1000XM5
\tAlias: Sony XM5
\tIcon: audio-headset
\tPaired: yes
\tTrusted: yes
\tBlocked: no
\tConnected: yes
\tBattery Percentage: 0x55 (85)
";
        let dev = parse_device_info("aa:bb:cc:dd:ee:ff", info).unwrap();
        assert_eq!(dev.mac, "AA:BB:CC:DD:EE:FF");
        assert_eq!(dev.name, "Sony XM5");
        assert_eq!(dev.icon, "audio-headset");
        assert!(dev.paired && dev.trusted && dev.connected && !dev.blocked);
        assert_eq!(dev.battery, Some(85));
        assert_eq!(dev.detail(), "Connected · 85% · Trusted");
    }

    #[test]
    fn parses_and_sorts_devices_and_builds_submenus() {
        let list = "\
Device 11:22:33:44:55:66 11-22-33-44-55-66
Device AA:BB:CC:DD:EE:FF Keychron K2
Device 22:33:44:55:66:77 MX Master 3S
";
        let parsed = parse_device_list(list);
        assert_eq!(parsed.len(), 3);

        let mut devices = vec![
            BluetoothDevice {
                mac: "11:22:33:44:55:66".into(),
                name: "11-22-33-44-55-66".into(),
                icon: String::new(),
                paired: false,
                trusted: false,
                blocked: false,
                connected: false,
                battery: None,
            },
            BluetoothDevice {
                mac: "AA:BB:CC:DD:EE:FF".into(),
                name: "Keychron K2".into(),
                icon: "input-keyboard".into(),
                paired: true,
                trusted: true,
                blocked: false,
                connected: false,
                battery: None,
            },
            BluetoothDevice {
                mac: "22:33:44:55:66:77".into(),
                name: "MX Master 3S".into(),
                icon: "input-mouse".into(),
                paired: true,
                trusted: true,
                blocked: false,
                connected: true,
                battery: Some(92),
            },
        ];
        sort_devices(&mut devices);
        let ordered: Vec<_> = devices.iter().map(|d| d.name.as_str()).collect();
        assert_eq!(ordered, ["MX Master 3S", "Keychron K2", "11-22-33-44-55-66"]);

        let state = BluetoothState {
            adapter: AdapterState {
                available: true,
                powered: true,
                discoverable: false,
                pairable: true,
                discovering: false,
                alias: "arch".into(),
            },
            devices,
        };

        let root = build_items(&state, &BluetoothView::Root);
        let root_labels: Vec<_> = root.iter().map(|i| i.label.as_str()).collect();
        assert_eq!(
            root_labels,
            ["Scan", "Settings", "MX Master 3S", "Keychron K2", "11-22-33-44-55-66"]
        );
        assert!(matches!(
            root[2].action,
            BluetoothAction::OpenView(BluetoothView::Device { .. })
        ));
        assert!(matches!(root[3].action, BluetoothAction::Connect { .. }));
        assert!(matches!(
            root[4].action,
            BluetoothAction::PairAndConnect { .. }
        ));

        let dev_sub = build_items(
            &state,
            &BluetoothView::Device {
                mac: "22:33:44:55:66:77".into(),
                name: "MX Master 3S".into(),
            },
        );
        let sub_labels: Vec<_> = dev_sub.iter().map(|i| i.label.as_str()).collect();
        assert_eq!(
            sub_labels,
            ["Disconnect", "Trusted: On", "Block Device", "Forget Device"]
        );
    }

    #[test]
    fn detects_interactive_pairing_prompts_and_outcomes() {
        assert_eq!(
            detect_confirm_passkey("[agent] Confirm passkey 482910 (yes/no): "),
            Some("482910".into())
        );
        assert_eq!(
            detect_pin_prompt("[agent] Enter PIN code: "),
            Some("Enter PIN code:".into())
        );
        assert_eq!(
            detect_display_passkey("[agent] Passkey: 739104\n"),
            Some("739104".into())
        );
        assert!(matches!(
            check_pair_outcome("Attempting to pair...\nPairing successful\n"),
            Some(Ok(()))
        ));
        assert!(matches!(
            check_pair_outcome("Failed to pair: org.bluez.Error.AuthenticationFailed\n"),
            Some(Err(_))
        ));
    }

    #[test]
    fn command_timeout_returns_immediately_on_fast_exit_and_kills_stalled_processes() {
        let start = Instant::now();
        let out = run_command_with_timeout("true", &[], Duration::from_secs(5)).unwrap();
        assert!(out.success);
        assert!(start.elapsed() < Duration::from_secs(2));

        let err = run_command_with_timeout("sleep", &["5"], Duration::from_millis(80))
            .err()
            .unwrap();
        assert!(err.contains("timed out"));
    }
}
