mod apps;
mod bluetooth;
mod blur;
mod config;
mod files;
mod history;
mod menu;
mod motion;
mod pointer;
mod ring;
mod settings;
mod themes;
mod ui;
mod wifi;
mod windows;

use config::ConfigStore;
use std::ffi::OsString;
use std::path::PathBuf;
use std::process::ExitCode;

const USAGE: &str = "\
ReCircles — a Wayland radial launcher

Usage:
  recircles
  recircles --run <name>
  recircles add <name> <config.json> [--main-menu <icon>]
  recircles remove <name> [--main-menu]
  recircles config [--force | -f]
  recircles reset

Choose a sector, then type to search. Esc goes back or closes the launcher.

Options:
  --run <name>        Open a built-in (files, apps, windows, wifi, bluetooth, settings) or custom menu directly
  --main-menu [icon]  With add: also add the menu to the main ring using <icon>
                      With remove: remove only from the main ring, keeping the known menu
  --force, -f         With config: overwrite an existing recircles.json
  -h, --help          Show this help message
  -V, --version       Show version information

Commands:
  add                 Add a menu JSON to ~/.config/recircles (known menus)
  remove              Remove a menu from known menus and the main ring
  config              Generate the default ~/.config/recircles/recircles.json
  reset               Clear saved file and application usage history";

#[derive(Debug, PartialEq, Eq)]
enum CliAction {
    Run(Option<String>),
    Add {
        name: String,
        config_path: Option<PathBuf>,
        main_menu_icon: Option<String>,
    },
    Remove {
        name: String,
        main_menu_only: bool,
    },
    GenerateConfig {
        force: bool,
    },
    Reset,
    Help,
    Version,
}

fn parse_args<I>(args: I) -> Result<CliAction, String>
where
    I: IntoIterator<Item = OsString>,
{
    let mut args = args.into_iter();
    let Some(first) = args.next() else {
        return Ok(CliAction::Run(None));
    };

    let first_str = first.to_string_lossy();
    match first_str.as_ref() {
        "--help" | "-h" => {
            ensure_no_extra(args)?;
            Ok(CliAction::Help)
        }
        "--version" | "-V" => {
            ensure_no_extra(args)?;
            Ok(CliAction::Version)
        }
        "reset" => {
            ensure_no_extra(args)?;
            Ok(CliAction::Reset)
        }
        "--run" => {
            let Some(name) = args.next() else {
                return Err(
                    "--run requires a menu name. Usage: recircles --run <name>".into(),
                );
            };
            let name = os_to_string(name, "menu name")?;
            if name.trim().is_empty() {
                return Err("--run requires a non-empty menu name.".into());
            }
            ensure_no_extra(args)?;
            Ok(CliAction::Run(Some(name)))
        }
        arg if arg.starts_with("--run=") => {
            let name = arg.trim_start_matches("--run=");
            if name.trim().is_empty() {
                return Err("--run requires a non-empty menu name.".into());
            }
            ensure_no_extra(args)?;
            Ok(CliAction::Run(Some(name.to_owned())))
        }
        "config" => parse_config_args(args),
        "add" => parse_add_args(args),
        "remove" => parse_remove_args(args),
        arg => Err(format!(
            "Unknown argument {arg:?}. Run recircles --help for usage."
        )),
    }
}

fn ensure_no_extra<I: Iterator<Item = OsString>>(mut args: I) -> Result<(), String> {
    if let Some(extra) = args.next() {
        let extra = extra.to_string_lossy();
        return Err(format!(
            "Unexpected argument {extra:?}. Run recircles --help for usage."
        ));
    }
    Ok(())
}

fn os_to_string(arg: OsString, label: &str) -> Result<String, String> {
    arg.into_string()
        .map_err(|_| format!("Invalid UTF-8 in {label}."))
}

fn parse_config_args<I: Iterator<Item = OsString>>(args: I) -> Result<CliAction, String> {
    let mut force = false;
    for arg in args {
        match arg.to_string_lossy().as_ref() {
            "--force" | "-f" => force = true,
            other => {
                return Err(format!(
                    "Unexpected argument {other:?} for config. Usage: recircles config [--force | -f]"
                ));
            }
        }
    }
    Ok(CliAction::GenerateConfig { force })
}

fn parse_add_args<I: Iterator<Item = OsString>>(mut args: I) -> Result<CliAction, String> {
    let mut positional: Vec<OsString> = Vec::new();
    let mut main_menu_icon: Option<String> = None;

    while let Some(arg) = args.next() {
        let lossy = arg.to_string_lossy();
        if lossy == "--main-menu" {
            if main_menu_icon.is_some() {
                return Err("--main-menu was specified more than once.".into());
            }
            let Some(icon) = args.next() else {
                return Err(
                    "--main-menu requires an icon when used with add. Usage: recircles add <name> <config.json> --main-menu <icon>"
                        .into(),
                );
            };
            main_menu_icon = Some(os_to_string(icon, "icon")?);
        } else if let Some(icon) = lossy.strip_prefix("--main-menu=") {
            if main_menu_icon.is_some() {
                return Err("--main-menu was specified more than once.".into());
            }
            main_menu_icon = Some(icon.to_owned());
        } else if lossy.starts_with('-') {
            return Err(format!(
                "Unknown option {lossy:?} for add. Usage: recircles add <name> <config.json> [--main-menu <icon>]"
            ));
        } else {
            positional.push(arg);
        }
    }

    match positional.len() {
        1 if main_menu_icon.is_some() => Ok(CliAction::Add {
            name: os_to_string(positional.remove(0), "menu name")?,
            config_path: None,
            main_menu_icon,
        }),
        2 => {
            let mut iter = positional.into_iter();
            let name = os_to_string(iter.next().unwrap(), "menu name")?;
            let config_path = PathBuf::from(iter.next().unwrap());
            Ok(CliAction::Add {
                name,
                config_path: Some(config_path),
                main_menu_icon,
            })
        }
        _ => Err("Usage: recircles add <name> <config.json> [--main-menu <icon>]".into()),
    }
}

fn parse_remove_args<I: Iterator<Item = OsString>>(args: I) -> Result<CliAction, String> {
    let mut name: Option<String> = None;
    let mut main_menu_only = false;

    for arg in args {
        let lossy = arg.to_string_lossy();
        if lossy == "--main-menu" {
            main_menu_only = true;
        } else if lossy.starts_with('-') {
            return Err(format!(
                "Unknown option {lossy:?} for remove. Usage: recircles remove <name> [--main-menu]"
            ));
        } else if name.is_none() {
            name = Some(os_to_string(arg, "menu name")?);
        } else {
            return Err(format!(
                "Unexpected argument {lossy:?}. Usage: recircles remove <name> [--main-menu]"
            ));
        }
    }

    let Some(name) = name else {
        return Err("Usage: recircles remove <name> [--main-menu]".into());
    };
    Ok(CliAction::Remove {
        name,
        main_menu_only,
    })
}

fn main() -> ExitCode {
    let action = match parse_args(std::env::args_os().skip(1)) {
        Ok(action) => action,
        Err(err) => {
            eprintln!("ReCircles: {err}");
            return ExitCode::FAILURE;
        }
    };

    match action {
        CliAction::Help => {
            println!("{USAGE}");
        }
        CliAction::Version => println!("recircles {}", env!("CARGO_PKG_VERSION")),
        CliAction::Reset => {
            if let Err(err) = history::History::reset() {
                eprintln!("ReCircles: {err}");
                return ExitCode::FAILURE;
            }
            println!("ReCircles history reset.");
        }
        CliAction::GenerateConfig { force } => {
            let result = ConfigStore::open().and_then(|s| s.generate_default_config(force));
            match result {
                Ok(path) => println!("Generated default config at {}.", path.display()),
                Err(err) => {
                    eprintln!("ReCircles: {err}");
                    return ExitCode::FAILURE;
                }
            }
        }
        CliAction::Add {
            name,
            config_path,
            main_menu_icon,
        } => {
            let result = ConfigStore::open().and_then(|store| {
                store.add_menu(&name, config_path.as_deref(), main_menu_icon.as_deref())
            });
            match result {
                Ok(()) => {
                    if main_menu_icon.is_some() {
                        println!("Added menu {name:?} and pinned it to the main menu.");
                    } else {
                        println!("Added menu {name:?} to known menus.");
                    }
                }
                Err(err) => {
                    eprintln!("ReCircles: {err}");
                    return ExitCode::FAILURE;
                }
            }
        }
        CliAction::Remove {
            name,
            main_menu_only,
        } => {
            let result =
                ConfigStore::open().and_then(|store| store.remove_menu(&name, main_menu_only));
            match result {
                Ok(()) => {
                    if main_menu_only {
                        println!("Removed menu {name:?} from the main menu.");
                    } else {
                        println!("Removed menu {name:?}.");
                    }
                }
                Err(err) => {
                    eprintln!("ReCircles: {err}");
                    return ExitCode::FAILURE;
                }
            }
        }
        CliAction::Run(target_name) => {
            let result = ConfigStore::open().and_then(|store| {
                let config = store.load_config()?;
                let start = match target_name {
                    Some(name) => Some(store.resolve_start_target(&name)?),
                    None => None,
                };
                ui::run(store, config, start)
            });
            if let Err(err) = result {
                eprintln!("ReCircles: {err}");
                return ExitCode::FAILURE;
            }
        }
    }
    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<CliAction, String> {
        parse_args(args.iter().map(OsString::from))
    }

    #[test]
    fn parses_run_and_builtin_commands() {
        assert_eq!(parse(&[]), Ok(CliAction::Run(None)));
        assert_eq!(
            parse(&["--run", "power"]),
            Ok(CliAction::Run(Some("power".into())))
        );
        assert_eq!(
            parse(&["--run=files"]),
            Ok(CliAction::Run(Some("files".into())))
        );
        assert!(parse(&["--run"]).is_err());
        assert!(parse(&["--run="]).is_err());
        assert_eq!(parse(&["reset"]), Ok(CliAction::Reset));
        assert_eq!(parse(&["--help"]), Ok(CliAction::Help));
        assert_eq!(parse(&["-V"]), Ok(CliAction::Version));
    }

    #[test]
    fn parses_add_remove_and_config_commands() {
        assert_eq!(
            parse(&["add", "power", "power.json"]),
            Ok(CliAction::Add {
                name: "power".into(),
                config_path: Some(PathBuf::from("power.json")),
                main_menu_icon: None,
            })
        );
        assert_eq!(
            parse(&["add", "power", "power.json", "--main-menu", "system-shutdown"]),
            Ok(CliAction::Add {
                name: "power".into(),
                config_path: Some(PathBuf::from("power.json")),
                main_menu_icon: Some("system-shutdown".into()),
            })
        );
        assert_eq!(
            parse(&["add", "--main-menu=system-shutdown", "power", "power.json"]),
            Ok(CliAction::Add {
                name: "power".into(),
                config_path: Some(PathBuf::from("power.json")),
                main_menu_icon: Some("system-shutdown".into()),
            })
        );
        assert_eq!(
            parse(&["add", "power", "--main-menu", "system-shutdown"]),
            Ok(CliAction::Add {
                name: "power".into(),
                config_path: None,
                main_menu_icon: Some("system-shutdown".into()),
            })
        );
        assert!(parse(&["add", "power", "power.json", "--main-menu"]).is_err());
        assert!(parse(&["add", "power"]).is_err());

        assert_eq!(
            parse(&["remove", "power"]),
            Ok(CliAction::Remove {
                name: "power".into(),
                main_menu_only: false,
            })
        );
        assert_eq!(
            parse(&["remove", "power", "--main-menu"]),
            Ok(CliAction::Remove {
                name: "power".into(),
                main_menu_only: true,
            })
        );
        assert_eq!(
            parse(&["remove", "--main-menu", "power"]),
            Ok(CliAction::Remove {
                name: "power".into(),
                main_menu_only: true,
            })
        );
        assert!(parse(&["remove"]).is_err());

        assert_eq!(
            parse(&["config"]),
            Ok(CliAction::GenerateConfig { force: false })
        );
        assert_eq!(
            parse(&["config", "-f"]),
            Ok(CliAction::GenerateConfig { force: true })
        );
        assert_eq!(
            parse(&["config", "--force"]),
            Ok(CliAction::GenerateConfig { force: true })
        );
        assert!(parse(&["config", "--unknown"]).is_err());
    }
}
