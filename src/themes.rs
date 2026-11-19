use crate::config::{AccentColors, CapsuleColors, ColorsConfig, RingColors, TextColors};
use serde::Deserialize;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Theme {
    pub id: String,
    pub name: String,
    pub colors: ColorsConfig,
    pub builtin: bool,
}

impl Theme {
    fn builtin(id: &str, name: &str, mut colors: ColorsConfig) -> Self {
        colors.theme = Some(id.to_owned());
        Self {
            id: id.to_owned(),
            name: name.to_owned(),
            colors,
            builtin: true,
        }
    }
}

pub fn builtin_themes() -> Vec<Theme> {
    vec![
        Theme::builtin("default", "Amber (Default)", ColorsConfig::default()),
        Theme::builtin(
            "light_green",
            "Light Green",
            ColorsConfig {
                theme: Some("light_green".into()),
                ring: RingColors {
                    top: "#dff5e4".into(),
                    mid: "#c7ebd0".into(),
                    bottom: "#b6e2c2".into(),
                    rim_top: "#ffffff".into(),
                    rim_bottom: "#5eb878".into(),
                    divider: "#347349".into(),
                    shadow: "#0d2415".into(),
                },
                capsule: CapsuleColors {
                    top: "#ecfbf0".into(),
                    bottom: "#d4f0dc".into(),
                    border: "#4fa368".into(),
                },
                text: TextColors {
                    primary: "#102819".into(),
                    capsule: "#102819".into(),
                    suggestion: "#386649".into(),
                    shadow: "#e9faed".into(),
                },
                accents: AccentColors {
                    files: "#1b8047".into(),
                    apps: "#298f66".into(),
                    windows: "#cc5635".into(),
                    wifi: "#147d8c".into(),
                    bluetooth: "#2566cc".into(),
                    custom: "#1f8a52".into(),
                    action: "#3b8f33".into(),
                },
            },
        ),
        Theme::builtin(
            "forest",
            "Forest",
            ColorsConfig {
                theme: Some("forest".into()),
                ring: RingColors {
                    top: "#163326".into(),
                    mid: "#0e2219".into(),
                    bottom: "#122a1f".into(),
                    rim_top: "#a8f0c6".into(),
                    rim_bottom: "#45b878".into(),
                    divider: "#98e8ba".into(),
                    shadow: "#010804".into(),
                },
                capsule: CapsuleColors {
                    top: "#173326".into(),
                    bottom: "#0d1f17".into(),
                    border: "#9cebc0".into(),
                },
                text: TextColors {
                    primary: "#f0fff5".into(),
                    capsule: "#edfff3".into(),
                    suggestion: "#9bc9ae".into(),
                    shadow: "#020a05".into(),
                },
                accents: AccentColors {
                    files: "#5ee092".into(),
                    apps: "#9be870".into(),
                    windows: "#ff8c6b".into(),
                    wifi: "#4ad9c5".into(),
                    bluetooth: "#6bb4ff".into(),
                    custom: "#7ae6a8".into(),
                    action: "#6cd97e".into(),
                },
            },
        ),
        Theme::builtin(
            "midnight",
            "Midnight Blue",
            ColorsConfig {
                theme: Some("midnight".into()),
                ring: RingColors {
                    top: "#182842".into(),
                    mid: "#101a2e".into(),
                    bottom: "#132038".into(),
                    rim_top: "#a8d4ff".into(),
                    rim_bottom: "#4a8ef5".into(),
                    divider: "#9cc8ff".into(),
                    shadow: "#010308".into(),
                },
                capsule: CapsuleColors {
                    top: "#192942".into(),
                    bottom: "#0f192b".into(),
                    border: "#9ecbff".into(),
                },
                text: TextColors {
                    primary: "#f0f6ff".into(),
                    capsule: "#eef5ff".into(),
                    suggestion: "#9db6d6".into(),
                    shadow: "#02060d".into(),
                },
                accents: AccentColors {
                    files: "#5cb8ff".into(),
                    apps: "#7ae0c8".into(),
                    windows: "#ff7888".into(),
                    wifi: "#4ee0b8".into(),
                    bluetooth: "#6ea8ff".into(),
                    custom: "#8fa8ff".into(),
                    action: "#5cc8ff".into(),
                },
            },
        ),
        Theme::builtin(
            "rose",
            "Rose Quartz",
            ColorsConfig {
                theme: Some("rose".into()),
                ring: RingColors {
                    top: "#3d1c2e".into(),
                    mid: "#26111d".into(),
                    bottom: "#301625".into(),
                    rim_top: "#ffc7dc".into(),
                    rim_bottom: "#e8649b".into(),
                    divider: "#ffbfd8".into(),
                    shadow: "#080105".into(),
                },
                capsule: CapsuleColors {
                    top: "#381a2b".into(),
                    bottom: "#210f19".into(),
                    border: "#ffc4dc".into(),
                },
                text: TextColors {
                    primary: "#fff2f7".into(),
                    capsule: "#fff0f6".into(),
                    suggestion: "#d4a5ba".into(),
                    shadow: "#0a0206".into(),
                },
                accents: AccentColors {
                    files: "#ff8cb4".into(),
                    apps: "#ffbf75".into(),
                    windows: "#ff6b78".into(),
                    wifi: "#68dbb8".into(),
                    bluetooth: "#85a8ff".into(),
                    custom: "#f598d8".into(),
                    action: "#ff7aa8".into(),
                },
            },
        ),
        Theme::builtin(
            "violet",
            "Amethyst",
            ColorsConfig {
                theme: Some("violet".into()),
                ring: RingColors {
                    top: "#291c45".into(),
                    mid: "#1a112e".into(),
                    bottom: "#211638".into(),
                    rim_top: "#d8c4ff".into(),
                    rim_bottom: "#8c61ff".into(),
                    divider: "#cfbaff".into(),
                    shadow: "#04010a".into(),
                },
                capsule: CapsuleColors {
                    top: "#261a40".into(),
                    bottom: "#160f26".into(),
                    border: "#d4bfff".into(),
                },
                text: TextColors {
                    primary: "#f6f2ff".into(),
                    capsule: "#f4efff".into(),
                    suggestion: "#b6a5d9".into(),
                    shadow: "#06020f".into(),
                },
                accents: AccentColors {
                    files: "#b38cff".into(),
                    apps: "#ff9be0".into(),
                    windows: "#ff7a8a".into(),
                    wifi: "#5ce0c8".into(),
                    bluetooth: "#75b0ff".into(),
                    custom: "#c799ff".into(),
                    action: "#9e7aff".into(),
                },
            },
        ),
        Theme::builtin(
            "monochrome",
            "Monochrome",
            ColorsConfig {
                theme: Some("monochrome".into()),
                ring: RingColors {
                    top: "#2e3035".into(),
                    mid: "#1c1e22".into(),
                    bottom: "#23252a".into(),
                    rim_top: "#e6e9ef".into(),
                    rim_bottom: "#88909e".into(),
                    divider: "#d4d8e0".into(),
                    shadow: "#000000".into(),
                },
                capsule: CapsuleColors {
                    top: "#292b30".into(),
                    bottom: "#191b1f".into(),
                    border: "#d8dce3".into(),
                },
                text: TextColors {
                    primary: "#ffffff".into(),
                    capsule: "#f7f8fa".into(),
                    suggestion: "#a8b0bd".into(),
                    shadow: "#050608".into(),
                },
                accents: AccentColors {
                    files: "#d8dde6".into(),
                    apps: "#eceff4".into(),
                    windows: "#b8c0cc".into(),
                    wifi: "#9ee0cc".into(),
                    bluetooth: "#9ec0ff".into(),
                    custom: "#cfd5e0".into(),
                    action: "#e0e4ec".into(),
                },
            },
        ),
    ]
}

pub fn normalize_theme_key(input: &str) -> String {
    input
        .trim()
        .to_ascii_lowercase()
        .replace(['-', ' '], "_")
}

pub fn find_builtin(query: &str) -> Option<Theme> {
    let key = normalize_theme_key(query);
    let canonical = match key.as_str() {
        "amber" | "amber_(default)" => "default",
        "midnight_blue" => "midnight",
        "rose_quartz" => "rose",
        "amethyst" => "violet",
        other => other,
    };
    builtin_themes()
        .into_iter()
        .find(|t| normalize_theme_key(&t.id) == canonical || normalize_theme_key(&t.name) == key)
}

pub fn validate_theme_id(id: &str) -> Result<(), String> {
    if id.trim().is_empty() {
        return Err("Theme name cannot be empty.".into());
    }
    if id == "."
        || id == ".."
        || id.starts_with('.')
        || id.contains('/')
        || id.contains('\\')
        || id.contains('\0')
    {
        return Err(format!("Invalid theme name {id:?}."));
    }
    if id.eq_ignore_ascii_case("custom") {
        return Err("Theme name \"custom\" is reserved.".into());
    }
    Ok(())
}

pub fn sanitize_theme_id(stem: &str) -> String {
    let mut id: String = stem
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
    while id.starts_with('.') || id.starts_with('_') {
        id.remove(0);
    }
    if id.is_empty() {
        id = "custom_theme".into();
    }
    if validate_theme_id(&id).is_err() {
        id.push_str("_theme");
    }
    id
}

pub fn display_name_from_id(id: &str) -> String {
    let words: Vec<String> = id
        .split(['_', '-'])
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut chars = w.chars();
            match chars.next() {
                Some(first) => format!("{}{}", first.to_ascii_uppercase(), chars.as_str()),
                None => String::new(),
            }
        })
        .collect();
    if words.is_empty() {
        id.to_owned()
    } else {
        words.join(" ")
    }
}

#[derive(Deserialize)]
struct ThemeColorsBlock {
    #[serde(default)]
    ring: Option<RingColors>,
    #[serde(default)]
    capsule: Option<CapsuleColors>,
    #[serde(default)]
    text: Option<TextColors>,
    #[serde(default)]
    accents: Option<AccentColors>,
}

impl ThemeColorsBlock {
    fn is_empty(&self) -> bool {
        self.ring.is_none()
            && self.capsule.is_none()
            && self.text.is_none()
            && self.accents.is_none()
    }
}

#[derive(Deserialize)]
struct ThemeFileRaw {
    #[serde(default, alias = "title")]
    name: Option<String>,
    #[serde(default)]
    colors: Option<ThemeColorsBlock>,
    #[serde(default)]
    ring: Option<RingColors>,
    #[serde(default)]
    capsule: Option<CapsuleColors>,
    #[serde(default)]
    text: Option<TextColors>,
    #[serde(default)]
    accents: Option<AccentColors>,
}

/// Parse and validate a theme JSON file.
///
/// Accepts either top-level `ring` / `capsule` / `text` / `accents` objects or a
/// nested `colors` object (like the `colors` block in `recircles.json`), plus an
/// optional display `name` (or `title`). At least one color group must be present,
/// and all hex colors must be valid `#RGB` or `#RRGGBB` strings.
pub fn parse_theme_json(id: &str, raw: &str) -> Result<Theme, String> {
    validate_theme_id(id)?;
    let parsed: ThemeFileRaw =
        serde_json::from_str(raw).map_err(|err| format!("Invalid theme JSON: {err}"))?;

    let block = if let Some(colors) = parsed.colors
        && !colors.is_empty()
    {
        colors
    } else {
        ThemeColorsBlock {
            ring: parsed.ring,
            capsule: parsed.capsule,
            text: parsed.text,
            accents: parsed.accents,
        }
    };

    if block.is_empty() {
        return Err(
            "Invalid theme JSON: expected ring, capsule, text, or accents color sections.".into(),
        );
    }

    let capsule = block.capsule.unwrap_or_else(|| match &block.ring {
        Some(ring) => CapsuleColors {
            top: ring.top.clone(),
            bottom: ring.bottom.clone(),
            border: ring.rim_top.clone(),
        },
        None => CapsuleColors::default(),
    });
    let colors = ColorsConfig {
        theme: Some(id.to_owned()),
        ring: block.ring.unwrap_or_default(),
        capsule,
        text: block.text.unwrap_or_default(),
        accents: block.accents.unwrap_or_default(),
    };
    colors.to_palette()?;

    let name = parsed
        .name
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| display_name_from_id(id));

    Ok(Theme {
        id: id.to_owned(),
        name,
        colors,
        builtin: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn builtin_themes_are_valid_unique_and_include_light_green() {
        let themes = builtin_themes();
        assert!(themes.len() >= 5);
        let mut ids = HashSet::new();
        for theme in &themes {
            assert!(ids.insert(theme.id.clone()), "duplicate theme id {}", theme.id);
            assert!(!theme.name.is_empty());
            assert!(theme.colors.to_palette().is_ok(), "invalid palette in {}", theme.id);
            assert_eq!(theme.colors.theme.as_deref(), Some(theme.id.as_str()));
        }
        let lg = find_builtin("light_green").expect("light_green theme must exist");
        assert_eq!(lg.name, "Light Green");
        assert_eq!(find_builtin("Light Green").unwrap().id, "light_green");
        assert_eq!(find_builtin("light-green").unwrap().id, "light_green");
        assert_eq!(find_builtin("amber").unwrap().id, "default");
    }

    #[test]
    fn parses_flat_wrapped_and_partial_theme_json_and_rejects_non_themes() {
        let flat = r##"{
            "name": "Matcha Mint",
            "ring": { "top": "#e0f5e5", "mid": "#c9ebd2", "bottom": "#b8e3c4" },
            "accents": { "files": "#1f854b" }
        }"##;
        let t = parse_theme_json("matcha", flat).unwrap();
        assert_eq!(t.id, "matcha");
        assert_eq!(t.name, "Matcha Mint");
        assert_eq!(t.colors.ring.top, "#e0f5e5");
        assert_eq!(t.colors.accents.files, "#1f854b");
        assert_eq!(t.colors.capsule.top, t.colors.ring.top);
        assert_eq!(t.colors.capsule.bottom, t.colors.ring.bottom);
        assert_eq!(t.colors.capsule.border, t.colors.ring.rim_top);

        let wrapped = r##"{
            "colors": {
                "text": { "primary": "#ffffff" }
            }
        }"##;
        let t2 = parse_theme_json("solar_flare", wrapped).unwrap();
        assert_eq!(t2.name, "Solar Flare");
        assert_eq!(t2.colors.text.primary, "#ffffff");

        // Menu JSON has no color sections and must be rejected.
        assert!(parse_theme_json("power", r#"{"title":"Power","items":[]}"#).is_err());
        assert!(parse_theme_json("empty", r#"{"colors":{}}"#).is_err());
        // Invalid hex must be rejected.
        assert!(
            parse_theme_json("bad", r#"{"ring":{"top":"not-a-hex"}}"#).is_err()
        );
        // Reserved id "custom" must be rejected.
        assert!(validate_theme_id("custom").is_err());
        assert_eq!(sanitize_theme_id("Custom"), "custom_theme");
    }
}
