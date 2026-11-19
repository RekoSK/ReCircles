use crate::ring::Icon;
use std::collections::HashMap;
use std::process::Command;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WifiNetwork {
    pub ssid: String,
    pub bssid: String,
    pub signal: u8,
    pub security: String,
    pub freq: String,
    pub active: bool,
    pub saved_uuid: Option<String>,
    pub saved_name: Option<String>,
    pub autoconnect: bool,
}

impl WifiNetwork {
    pub fn is_secured(&self) -> bool {
        let sec = self.security.trim();
        !sec.is_empty() && sec != "--"
    }

    pub fn is_saved(&self) -> bool {
        self.saved_uuid.is_some()
    }

    pub fn band_label(&self) -> Option<&'static str> {
        let mhz: u32 = self
            .freq
            .split_whitespace()
            .next()
            .and_then(|s| s.parse().ok())?;
        if mhz >= 5900 {
            Some("6 GHz")
        } else if mhz >= 4900 {
            Some("5 GHz")
        } else if mhz >= 2400 {
            Some("2.4 GHz")
        } else {
            None
        }
    }

    pub fn detail(&self) -> String {
        let mut parts = Vec::new();
        if self.active {
            parts.push("Connected".to_string());
        } else if self.is_saved() {
            parts.push("Saved".to_string());
        }
        parts.push(format!("{}%", self.signal));
        if self.is_secured() {
            parts.push(self.security.clone());
        } else {
            parts.push("Open".to_string());
        }
        if let Some(band) = self.band_label() {
            parts.push(band.to_string());
        }
        parts.join(" · ")
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SavedNetwork {
    pub name: String,
    pub uuid: String,
    pub autoconnect: bool,
    pub active: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WifiState {
    pub adapter_available: bool,
    pub enabled: bool,
    pub device_name: Option<String>,
    pub active_connection: Option<String>,
    pub networks: Vec<WifiNetwork>,
    pub saved: Vec<SavedNetwork>,
}

impl WifiState {
    pub fn connected_network(&self) -> Option<&WifiNetwork> {
        self.networks.iter().find(|net| net.active)
    }

    pub fn find_network(&self, ssid: &str) -> Option<&WifiNetwork> {
        self.networks.iter().find(|net| net.ssid == ssid)
    }

    pub fn find_saved(&self, uuid: &str) -> Option<&SavedNetwork> {
        self.saved.iter().find(|saved| saved.uuid == uuid)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WifiView {
    Root,
    Settings,
    Saved,
    Network {
        ssid: String,
        saved_uuid: Option<String>,
        saved_name: Option<String>,
    },
}

impl WifiView {
    pub fn title(&self) -> String {
        match self {
            Self::Root => "Wi-Fi".into(),
            Self::Settings => "Wi-Fi Settings".into(),
            Self::Saved => "Saved Networks".into(),
            Self::Network {
                ssid, saved_name, ..
            } => {
                if !ssid.is_empty() {
                    ssid.clone()
                } else {
                    saved_name.clone().unwrap_or_else(|| "Network".into())
                }
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WifiAction {
    OpenView(WifiView),
    ToggleRadio(bool),
    Rescan,
    Connect(WifiNetwork),
    ConnectSaved {
        uuid: String,
        name: String,
    },
    ConnectWithPassword {
        ssid: String,
        saved_uuid: Option<String>,
    },
    ConnectHidden,
    Disconnect {
        uuid: Option<String>,
        device: Option<String>,
        label: String,
    },
    SetAutoconnect {
        uuid: String,
        name: String,
        enable: bool,
    },
    Forget {
        uuid: String,
        name: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WifiItem {
    pub label: String,
    pub detail: String,
    pub icon: Icon,
    pub active: bool,
    pub action: WifiAction,
    pub secondary_action: Option<WifiAction>,
}

pub fn build_items(state: &WifiState, view: &WifiView) -> Vec<WifiItem> {
    match view {
        WifiView::Root => build_root_items(state),
        WifiView::Settings => build_settings_items(state),
        WifiView::Saved => build_saved_items(state),
        WifiView::Network {
            ssid,
            saved_uuid,
            saved_name,
        } => build_network_items(state, ssid, saved_uuid.as_deref(), saved_name.as_deref()),
    }
}

pub fn idle_status(state: &WifiState, view: &WifiView) -> String {
    match view {
        WifiView::Root => {
            if !state.adapter_available {
                "No Wi-Fi adapter found".into()
            } else if !state.enabled {
                "Wi-Fi is turned off".into()
            } else if let Some(net) = state.connected_network() {
                format!("Connected to {} · Right-click to manage", net.ssid)
            } else if let Some(conn) = &state.active_connection {
                format!("Connected to {conn} · Right-click to manage")
            } else if state.networks.is_empty() {
                "No networks found · Select Rescan".into()
            } else {
                "Click to connect · Right-click to manage".into()
            }
        }
        WifiView::Settings => {
            if !state.adapter_available {
                "No Wi-Fi adapter found".into()
            } else if state.enabled {
                "Wi-Fi is on · Manage radio and profiles".into()
            } else {
                "Wi-Fi is off".into()
            }
        }
        WifiView::Saved => {
            if state.saved.is_empty() {
                "No saved Wi-Fi networks".into()
            } else {
                "Click to manage or connect".into()
            }
        }
        WifiView::Network {
            ssid, saved_uuid, ..
        } => {
            if let Some(net) = state.find_network(ssid) {
                net.detail()
            } else if let Some(saved) = saved_uuid.as_deref().and_then(|u| state.find_saved(u)) {
                let mut parts = Vec::new();
                if saved.active {
                    parts.push("Connected");
                } else {
                    parts.push("Saved");
                }
                if saved.autoconnect {
                    parts.push("Auto-connect on");
                } else {
                    parts.push("Auto-connect off");
                }
                parts.join(" · ")
            } else {
                "Manage network".into()
            }
        }
    }
}

fn build_root_items(state: &WifiState) -> Vec<WifiItem> {
    if !state.adapter_available {
        return Vec::new();
    }
    let mut items = Vec::new();
    if !state.enabled {
        items.push(WifiItem {
            label: "Turn Wi-Fi On".into(),
            detail: "Enable Wi-Fi radio and scan for networks".into(),
            icon: Icon::ToggleOff,
            active: false,
            action: WifiAction::ToggleRadio(true),
            secondary_action: None,
        });
        items.push(WifiItem {
            label: "Saved Networks".into(),
            detail: format!("{} saved profile(s)", state.saved.len()),
            icon: Icon::Submenu,
            active: false,
            action: WifiAction::OpenView(WifiView::Saved),
            secondary_action: None,
        });
        items.push(WifiItem {
            label: "Settings".into(),
            detail: "Wi-Fi radio, hidden network and profiles".into(),
            icon: Icon::Settings,
            active: false,
            action: WifiAction::OpenView(WifiView::Settings),
            secondary_action: None,
        });
        return items;
    }

    items.push(WifiItem {
        label: "Rescan".into(),
        detail: "Scan for nearby Wi-Fi networks".into(),
        icon: Icon::Scan,
        active: false,
        action: WifiAction::Rescan,
        secondary_action: None,
    });
    items.push(WifiItem {
        label: "Settings".into(),
        detail: "Wi-Fi on/off, hidden network & saved profiles".into(),
        icon: Icon::Settings,
        active: false,
        action: WifiAction::OpenView(WifiView::Settings),
        secondary_action: None,
    });

    for net in &state.networks {
        let icon = if net.active {
            Icon::WifiConnected
        } else if net.is_secured() {
            Icon::WifiLocked
        } else {
            Icon::WifiNetwork
        };
        let manage_view = WifiView::Network {
            ssid: net.ssid.clone(),
            saved_uuid: net.saved_uuid.clone(),
            saved_name: net.saved_name.clone(),
        };
        let primary = if net.active {
            WifiAction::OpenView(manage_view.clone())
        } else {
            WifiAction::Connect(net.clone())
        };
        items.push(WifiItem {
            label: net.ssid.clone(),
            detail: net.detail(),
            icon,
            active: net.active,
            action: primary,
            secondary_action: Some(WifiAction::OpenView(manage_view)),
        });
    }

    items
}

fn build_settings_items(state: &WifiState) -> Vec<WifiItem> {
    let mut items = Vec::new();
    items.push(WifiItem {
        label: if state.enabled {
            "Wi-Fi: On".into()
        } else {
            "Wi-Fi: Off".into()
        },
        detail: if state.enabled {
            "Click to turn Wi-Fi radio off".into()
        } else {
            "Click to turn Wi-Fi radio on".into()
        },
        icon: if state.enabled {
            Icon::ToggleOn
        } else {
            Icon::ToggleOff
        },
        active: state.enabled,
        action: WifiAction::ToggleRadio(!state.enabled),
        secondary_action: None,
    });

    if state.enabled {
        items.push(WifiItem {
            label: "Rescan Networks".into(),
            detail: "Trigger a fresh Wi-Fi scan".into(),
            icon: Icon::Scan,
            active: false,
            action: WifiAction::Rescan,
            secondary_action: None,
        });
        items.push(WifiItem {
            label: "Hidden Network…".into(),
            detail: "Connect to a network that hides its SSID".into(),
            icon: Icon::Add,
            active: false,
            action: WifiAction::ConnectHidden,
            secondary_action: None,
        });
        if let Some(active) = state.connected_network() {
            items.push(WifiItem {
                label: "Disconnect".into(),
                detail: format!("Disconnect from {}", active.ssid),
                icon: Icon::Disconnect,
                active: false,
                action: WifiAction::Disconnect {
                    uuid: active.saved_uuid.clone(),
                    device: state.device_name.clone(),
                    label: active.ssid.clone(),
                },
                secondary_action: None,
            });
        } else if let Some(conn) = &state.active_connection {
            items.push(WifiItem {
                label: "Disconnect".into(),
                detail: format!("Disconnect from {conn}"),
                icon: Icon::Disconnect,
                active: false,
                action: WifiAction::Disconnect {
                    uuid: None,
                    device: state.device_name.clone(),
                    label: conn.clone(),
                },
                secondary_action: None,
            });
        }
    }

    items.push(WifiItem {
        label: "Saved Networks".into(),
        detail: format!("Manage {} saved Wi-Fi profile(s)", state.saved.len()),
        icon: Icon::Submenu,
        active: false,
        action: WifiAction::OpenView(WifiView::Saved),
        secondary_action: None,
    });

    items
}

fn build_saved_items(state: &WifiState) -> Vec<WifiItem> {
    state
        .saved
        .iter()
        .map(|saved| {
            let mut parts = Vec::new();
            if saved.active {
                parts.push("Connected");
            }
            if saved.autoconnect {
                parts.push("Auto-connect");
            } else {
                parts.push("Manual");
            }
            let manage_view = WifiView::Network {
                ssid: saved.name.clone(),
                saved_uuid: Some(saved.uuid.clone()),
                saved_name: Some(saved.name.clone()),
            };
            let primary = if saved.active {
                WifiAction::OpenView(manage_view.clone())
            } else {
                WifiAction::ConnectSaved {
                    uuid: saved.uuid.clone(),
                    name: saved.name.clone(),
                }
            };
            WifiItem {
                label: saved.name.clone(),
                detail: parts.join(" · "),
                icon: if saved.active {
                    Icon::WifiConnected
                } else {
                    Icon::WifiNetwork
                },
                active: saved.active,
                action: primary,
                secondary_action: Some(WifiAction::OpenView(manage_view)),
            }
        })
        .collect()
}

fn build_network_items(
    state: &WifiState,
    ssid: &str,
    saved_uuid: Option<&str>,
    saved_name: Option<&str>,
) -> Vec<WifiItem> {
    let live = state.find_network(ssid);
    let saved = saved_uuid
        .and_then(|u| state.find_saved(u))
        .or_else(|| live.and_then(|n| n.saved_uuid.as_deref()).and_then(|u| state.find_saved(u)))
        .or_else(|| state.saved.iter().find(|s| s.name == ssid));

    let is_active = live.is_some_and(|n| n.active) || saved.is_some_and(|s| s.active);
    let is_secured = live.is_none_or(WifiNetwork::is_secured);
    let display_name = if !ssid.is_empty() {
        ssid.to_owned()
    } else {
        saved
            .map(|s| s.name.clone())
            .or_else(|| saved_name.map(str::to_owned))
            .unwrap_or_else(|| "Network".into())
    };
    let uuid = saved
        .map(|s| s.uuid.clone())
        .or_else(|| saved_uuid.map(str::to_owned));

    let mut items = Vec::new();
    if is_active {
        items.push(WifiItem {
            label: "Disconnect".into(),
            detail: format!("Disconnect from {display_name}"),
            icon: Icon::Disconnect,
            active: false,
            action: WifiAction::Disconnect {
                uuid: uuid.clone(),
                device: state.device_name.clone(),
                label: display_name.clone(),
            },
            secondary_action: None,
        });
    } else if let Some(net) = live {
        items.push(WifiItem {
            label: "Connect".into(),
            detail: net.detail(),
            icon: Icon::WifiConnected,
            active: false,
            action: WifiAction::Connect(net.clone()),
            secondary_action: None,
        });
    } else if let Some(ref uuid) = uuid {
        items.push(WifiItem {
            label: "Connect".into(),
            detail: format!("Activate saved profile {display_name}"),
            icon: Icon::WifiConnected,
            active: false,
            action: WifiAction::ConnectSaved {
                uuid: uuid.clone(),
                name: display_name.clone(),
            },
            secondary_action: None,
        });
    }

    if is_secured && !ssid.is_empty() {
        items.push(WifiItem {
            label: if uuid.is_some() {
                "Change Password…".into()
            } else {
                "Enter Password…".into()
            },
            detail: format!("Set password and connect to {display_name}"),
            icon: Icon::WifiLocked,
            active: false,
            action: WifiAction::ConnectWithPassword {
                ssid: ssid.to_owned(),
                saved_uuid: uuid.clone(),
            },
            secondary_action: None,
        });
    }

    if let Some(saved) = saved {
        items.push(WifiItem {
            label: if saved.autoconnect {
                "Auto-Connect: On".into()
            } else {
                "Auto-Connect: Off".into()
            },
            detail: if saved.autoconnect {
                "Automatically join when in range".into()
            } else {
                "Only connect when selected manually".into()
            },
            icon: if saved.autoconnect {
                Icon::ToggleOn
            } else {
                Icon::ToggleOff
            },
            active: saved.autoconnect,
            action: WifiAction::SetAutoconnect {
                uuid: saved.uuid.clone(),
                name: saved.name.clone(),
                enable: !saved.autoconnect,
            },
            secondary_action: None,
        });
        items.push(WifiItem {
            label: "Forget Network".into(),
            detail: format!("Delete saved profile for {}", saved.name),
            icon: Icon::Forget,
            active: false,
            action: WifiAction::Forget {
                uuid: saved.uuid.clone(),
                name: saved.name.clone(),
            },
            secondary_action: None,
        });
    }

    items
}

pub fn load_state(rescan: bool) -> Result<WifiState, String> {
    let radio_out = run_nmcli(&["radio", "wifi"])?;
    let enabled = radio_out.trim().eq_ignore_ascii_case("enabled");

    let dev_out = run_nmcli(&["-t", "-f", "DEVICE,TYPE,STATE,CONNECTION", "device", "status"])?;
    let (device_name, active_connection) = parse_wifi_device(&dev_out);
    let adapter_available = device_name.is_some();

    let saved_out = run_nmcli(&[
        "-t",
        "-f",
        "NAME,UUID,TYPE,AUTOCONNECT,ACTIVE",
        "connection",
        "show",
    ])?;
    let saved = parse_saved_connections(&saved_out);

    let networks = if adapter_available && enabled {
        let rescan_arg = if rescan { "yes" } else { "auto" };
        let list_out = run_nmcli(&[
            "-t",
            "-f",
            "ACTIVE,SSID,SIGNAL,SECURITY,FREQ,BSSID",
            "device",
            "wifi",
            "list",
            "--rescan",
            rescan_arg,
        ])?;
        parse_wifi_list(&list_out, &saved)
    } else {
        Vec::new()
    };

    Ok(WifiState {
        adapter_available,
        enabled,
        device_name,
        active_connection,
        networks,
        saved,
    })
}

pub fn set_radio(enable: bool) -> Result<(), String> {
    let arg = if enable { "on" } else { "off" };
    run_nmcli(&["radio", "wifi", arg]).map(|_| ())
}

pub fn connect_saved_or_open(network: &WifiNetwork) -> Result<(), String> {
    if let Some(uuid) = &network.saved_uuid {
        return connect_saved(uuid);
    }
    run_nmcli(&["device", "wifi", "connect", &network.ssid]).map(|_| ())
}

pub fn connect_saved(uuid: &str) -> Result<(), String> {
    run_nmcli(&["connection", "up", "uuid", uuid]).map(|_| ())
}

pub fn connect_with_password(
    ssid: &str,
    saved_uuid: Option<&str>,
    password: &str,
) -> Result<(), String> {
    if let Some(uuid) = saved_uuid {
        let modified = run_nmcli(&[
            "connection",
            "modify",
            "uuid",
            uuid,
            "802-11-wireless-security.psk",
            password,
        ]);
        if modified.is_ok() {
            return run_nmcli(&["connection", "up", "uuid", uuid]).map(|_| ());
        }
    }
    run_nmcli(&["device", "wifi", "connect", ssid, "password", password]).map(|_| ())
}

pub fn connect_hidden(ssid: &str, password: Option<&str>) -> Result<(), String> {
    let mut args = vec!["device", "wifi", "connect", ssid, "hidden", "yes"];
    if let Some(pw) = password
        && !pw.is_empty()
    {
        args.push("password");
        args.push(pw);
    }
    run_nmcli(&args).map(|_| ())
}

pub fn disconnect(uuid: Option<&str>, device: Option<&str>) -> Result<(), String> {
    if let Some(uuid) = uuid {
        return run_nmcli(&["connection", "down", "uuid", uuid]).map(|_| ());
    }
    if let Some(dev) = device {
        return run_nmcli(&["device", "disconnect", dev]).map(|_| ());
    }
    Err("No active Wi-Fi connection to disconnect.".into())
}

pub fn set_autoconnect(uuid: &str, enable: bool) -> Result<(), String> {
    let val = if enable { "yes" } else { "no" };
    run_nmcli(&["connection", "modify", "uuid", uuid, "connection.autoconnect", val]).map(|_| ())
}

pub fn forget(uuid: &str) -> Result<(), String> {
    run_nmcli(&["connection", "delete", "uuid", uuid]).map(|_| ())
}

fn run_nmcli(args: &[&str]) -> Result<String, String> {
    let output = Command::new("nmcli").args(args).output().map_err(|err| {
        if err.kind() == std::io::ErrorKind::NotFound {
            "NetworkManager (nmcli) is not installed.".to_string()
        } else {
            format!("Could not run nmcli: {err}")
        }
    })?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let detail = stderr
            .lines()
            .chain(stdout.lines())
            .map(str::trim)
            .find(|line| !line.is_empty())
            .unwrap_or("NetworkManager request failed.");
        Err(clean_nmcli_error(detail))
    }
}

fn clean_nmcli_error(raw: &str) -> String {
    raw.strip_prefix("Error: ").unwrap_or(raw).trim().to_owned()
}

/// Splits an `nmcli -t` colon-delimited line while respecting `\:` and `\\` escapes.
pub fn split_nmcli_terse(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut current = String::new();
    let mut escaped = false;
    for ch in line.chars() {
        if escaped {
            current.push(ch);
            escaped = false;
        } else if ch == '\\' {
            escaped = true;
        } else if ch == ':' {
            fields.push(std::mem::take(&mut current));
        } else {
            current.push(ch);
        }
    }
    if escaped {
        current.push('\\');
    }
    fields.push(current);
    fields
}

fn parse_wifi_device(output: &str) -> (Option<String>, Option<String>) {
    let mut found_device = None;
    let mut active_conn = None;
    for line in output.lines() {
        let fields = split_nmcli_terse(line);
        if fields.len() < 4 || fields[1] != "wifi" {
            continue;
        }
        let dev = fields[0].trim();
        if dev.is_empty() {
            continue;
        }
        let state = fields[2].trim();
        let conn = fields[3].trim();
        if state == "connected" && !conn.is_empty() && conn != "--" {
            return (Some(dev.to_owned()), Some(conn.to_owned()));
        }
        if found_device.is_none() {
            found_device = Some(dev.to_owned());
            if !conn.is_empty() && conn != "--" {
                active_conn = Some(conn.to_owned());
            }
        }
    }
    (found_device, active_conn)
}

fn parse_saved_connections(output: &str) -> Vec<SavedNetwork> {
    let mut saved = Vec::new();
    for line in output.lines() {
        let fields = split_nmcli_terse(line);
        if fields.len() < 5 || fields[2] != "802-11-wireless" {
            continue;
        }
        let name = fields[0].trim();
        let uuid = fields[1].trim();
        if name.is_empty() || uuid.is_empty() {
            continue;
        }
        saved.push(SavedNetwork {
            name: name.to_owned(),
            uuid: uuid.to_owned(),
            autoconnect: fields[3].eq_ignore_ascii_case("yes"),
            active: fields[4].eq_ignore_ascii_case("yes"),
        });
    }
    saved.sort_by_cached_key(|s| (!s.active, s.name.to_lowercase(), s.uuid.clone()));
    saved
}

fn parse_wifi_list(output: &str, saved: &[SavedNetwork]) -> Vec<WifiNetwork> {
    let saved_by_name: HashMap<&str, &SavedNetwork> =
        saved.iter().map(|s| (s.name.as_str(), s)).collect();
    let mut dedup: HashMap<String, WifiNetwork> = HashMap::new();

    for line in output.lines() {
        let fields = split_nmcli_terse(line);
        if fields.len() < 6 {
            continue;
        }
        let ssid = fields[1].trim();
        if ssid.is_empty() {
            continue;
        }
        let active = fields[0].eq_ignore_ascii_case("yes");
        let signal = fields[2].trim().parse::<u8>().unwrap_or(0).min(100);
        let security = fields[3].trim().to_owned();
        let freq = fields[4].trim().to_owned();
        let bssid = fields[5].trim().to_owned();
        let matched = saved_by_name.get(ssid).copied();

        let candidate = WifiNetwork {
            ssid: ssid.to_owned(),
            bssid,
            signal,
            security,
            freq,
            active,
            saved_uuid: matched.map(|s| s.uuid.clone()),
            saved_name: matched.map(|s| s.name.clone()),
            autoconnect: matched.is_some_and(|s| s.autoconnect),
        };

        match dedup.get_mut(ssid) {
            Some(existing) => {
                let replace = (candidate.active && !existing.active)
                    || (candidate.active == existing.active && candidate.signal > existing.signal);
                if replace {
                    *existing = candidate;
                }
            }
            None => {
                dedup.insert(ssid.to_owned(), candidate);
            }
        }
    }

    let mut networks: Vec<_> = dedup.into_values().collect();
    networks.sort_by_cached_key(|n| {
        (
            !n.active,
            !n.is_saved(),
            std::cmp::Reverse(n.signal),
            n.ssid.to_lowercase(),
        )
    });
    networks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_nmcli_terse_fields_with_escaped_colons_and_backslashes() {
        assert_eq!(
            split_nmcli_terse(r"yes:My\:Home\\Net:88:WPA2:5180 MHz:AA\:BB\:CC\:DD\:EE\:FF"),
            vec![
                "yes",
                r"My:Home\Net",
                "88",
                "WPA2",
                "5180 MHz",
                "AA:BB:CC:DD:EE:FF"
            ]
        );
        assert_eq!(split_nmcli_terse("no::40::2412 MHz:"), vec!["no", "", "40", "", "2412 MHz", ""]);
    }

    #[test]
    fn parses_deduplicates_and_ranks_wifi_networks() {
        let saved_raw = "\
HomeNet:uuid-home:802-11-wireless:yes:no
Cafe:uuid-cafe:802-11-wireless:no:yes
Wired:uuid-eth:802-3-ethernet:yes:yes
";
        let saved = parse_saved_connections(saved_raw);
        assert_eq!(saved.len(), 2);
        assert_eq!(saved[0].name, "Cafe");
        assert!(saved[0].active);
        assert_eq!(saved[1].name, "HomeNet");
        assert!(saved[1].autoconnect);

        let list_raw = r"no:HomeNet:52:WPA2:2437 MHz:11\:22\:33\:44\:55\:66
no:HomeNet:84:WPA2:5180 MHz:11\:22\:33\:44\:55\:67
no::99:WPA2:5200 MHz:AA\:BB\:CC\:DD\:EE\:FF
yes:Cafe:60:--:2412 MHz:22\:33\:44\:55\:66\:77
no:Neighbor:95:WPA2 WPA3:5500 MHz:33\:44\:55\:66\:77\:88
";
        let networks = parse_wifi_list(list_raw, &saved);
        assert_eq!(networks.len(), 3);
        // Active first, then saved, then unsaved by signal.
        assert_eq!(networks[0].ssid, "Cafe");
        assert!(networks[0].active);
        assert!(!networks[0].is_secured());

        assert_eq!(networks[1].ssid, "HomeNet");
        assert_eq!(networks[1].signal, 84);
        assert_eq!(networks[1].band_label(), Some("5 GHz"));
        assert_eq!(networks[1].saved_uuid.as_deref(), Some("uuid-home"));

        assert_eq!(networks[2].ssid, "Neighbor");
        assert!(networks[2].is_secured());
        assert!(!networks[2].is_saved());
    }

    #[test]
    fn parses_wifi_device_status() {
        let out = "\
enp3s0:ethernet:disconnected:--
wlan0:wifi:connected:HomeNet
lo:loopback:connected:lo
";
        assert_eq!(
            parse_wifi_device(out),
            (Some("wlan0".into()), Some("HomeNet".into()))
        );
    }

    #[test]
    fn builds_root_settings_and_network_submenu_items() {
        let state = WifiState {
            adapter_available: true,
            enabled: true,
            device_name: Some("wlan0".into()),
            active_connection: Some("HomeNet".into()),
            networks: vec![
                WifiNetwork {
                    ssid: "HomeNet".into(),
                    bssid: "AA:BB:CC:DD:EE:FF".into(),
                    signal: 90,
                    security: "WPA2".into(),
                    freq: "5180 MHz".into(),
                    active: true,
                    saved_uuid: Some("uuid-1".into()),
                    saved_name: Some("HomeNet".into()),
                    autoconnect: true,
                },
                WifiNetwork {
                    ssid: "Guest".into(),
                    bssid: "11:22:33:44:55:66".into(),
                    signal: 65,
                    security: "".into(),
                    freq: "2412 MHz".into(),
                    active: false,
                    saved_uuid: None,
                    saved_name: None,
                    autoconnect: false,
                },
            ],
            saved: vec![SavedNetwork {
                name: "HomeNet".into(),
                uuid: "uuid-1".into(),
                autoconnect: true,
                active: true,
            }],
        };

        let root = build_items(&state, &WifiView::Root);
        let labels: Vec<_> = root.iter().map(|i| i.label.as_str()).collect();
        assert_eq!(labels, ["Rescan", "Settings", "HomeNet", "Guest"]);
        assert!(root[2].active);
        assert!(matches!(root[2].action, WifiAction::OpenView(WifiView::Network { .. })));
        assert!(matches!(root[3].action, WifiAction::Connect(_)));

        let settings = build_items(&state, &WifiView::Settings);
        let setting_labels: Vec<_> = settings.iter().map(|i| i.label.as_str()).collect();
        assert_eq!(
            setting_labels,
            ["Wi-Fi: On", "Rescan Networks", "Hidden Network…", "Disconnect", "Saved Networks"]
        );

        let net_sub = build_items(
            &state,
            &WifiView::Network {
                ssid: "HomeNet".into(),
                saved_uuid: Some("uuid-1".into()),
                saved_name: Some("HomeNet".into()),
            },
        );
        let sub_labels: Vec<_> = net_sub.iter().map(|i| i.label.as_str()).collect();
        assert_eq!(
            sub_labels,
            ["Disconnect", "Change Password…", "Auto-Connect: On", "Forget Network"]
        );
    }
}
