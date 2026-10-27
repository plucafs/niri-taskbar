use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Position {
    #[default]
    Top,
    Bottom,
    Left,
    Right,
}

impl Position {
    pub const ALL: [Position; 4] = [
        Position::Top,
        Position::Bottom,
        Position::Left,
        Position::Right,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            Position::Top => "Top",
            Position::Bottom => "Bottom",
            Position::Left => "Left",
            Position::Right => "Right",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum IconPosition {
    #[default]
    Left,
    Center,
    Right,
}

impl IconPosition {
    pub const ALL: [IconPosition; 3] = [
        IconPosition::Left,
        IconPosition::Center,
        IconPosition::Right,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            IconPosition::Left => "Left",
            IconPosition::Center => "Center",
            IconPosition::Right => "Right",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum GroupBy {
    #[default]
    App,
    Window,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum SortMode {
    #[default]
    LastOpened,
    Alphabetical,
    Custom,
}

impl SortMode {
    pub const ALL: [SortMode; 3] = [
        SortMode::LastOpened,
        SortMode::Alphabetical,
        SortMode::Custom,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            SortMode::LastOpened => "Last opened at right",
            SortMode::Alphabetical => "Alphabetical",
            SortMode::Custom => "Custom",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum HoverInfo {
    #[default]
    Tooltip,
    Preview,
    Off,
}

impl HoverInfo {
    pub const ALL: [HoverInfo; 3] = [HoverInfo::Tooltip, HoverInfo::Preview, HoverInfo::Off];

    pub fn label(&self) -> &'static str {
        match self {
            HoverInfo::Tooltip => "Tooltip",
            HoverInfo::Preview => "Window list",
            HoverInfo::Off => "Off",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum MiddleClick {
    #[default]
    Close,
    NewWindow,
    Focus,
}

impl MiddleClick {
    pub const ALL: [MiddleClick; 3] =
        [MiddleClick::Close, MiddleClick::NewWindow, MiddleClick::Focus];

    pub fn label(&self) -> &'static str {
        match self {
            MiddleClick::Close => "Close window",
            MiddleClick::NewWindow => "Open new window",
            MiddleClick::Focus => "Focus window",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum IconFallback {
    #[default]
    Default,
    Initials,
}

impl IconFallback {
    pub const ALL: [IconFallback; 2] = [IconFallback::Default, IconFallback::Initials];

    pub fn label(&self) -> &'static str {
        match self {
            IconFallback::Default => "Default icon",
            IconFallback::Initials => "First two letters",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum BarLayer {
    Top,
    #[default]
    Bottom,
}

impl BarLayer {
    pub const ALL: [BarLayer; 2] = [BarLayer::Top, BarLayer::Bottom];

    pub fn label(&self) -> &'static str {
        match self {
            BarLayer::Top => "Top (stays over overview)",
            BarLayer::Bottom => "Bottom (zooms with overview)",
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct Config {
    pub position: Position,
    pub icon_position: IconPosition,
    pub bar_layer: BarLayer,
    pub thickness: u32,
    pub icon_size: u32,
    pub group_by: GroupBy,
    pub sort_mode: SortMode,
    pub custom_order: Vec<String>,
    pub show_settings_icon: bool,
    pub hover_info: HoverInfo,
    pub middle_click: MiddleClick,
    pub pinned_apps: Vec<String>,
    pub icon_fallback: IconFallback,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            position: Position::Top,
            icon_position: IconPosition::Left,
            bar_layer: BarLayer::Bottom,
            thickness: 32,
            icon_size: 22,
            group_by: GroupBy::App,
            sort_mode: SortMode::LastOpened,
            custom_order: Vec::new(),
            show_settings_icon: true,
            hover_info: HoverInfo::Tooltip,
            middle_click: MiddleClick::Close,
            pinned_apps: Vec::new(),
            icon_fallback: IconFallback::Default,
        }
    }
}

impl Config {
    pub fn load() -> Self {
        let path = config_path();
        match std::fs::read_to_string(&path) {
            Ok(text) => match toml::from_str(&text) {
                Ok(config) => config,
                Err(err) => {
                    eprintln!("niri-taskbar: {}: {err}", path.display());
                    Self::default()
                }
            },
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self) {
        let path = config_path();
        if let Some(dir) = path.parent() {
            if let Err(err) = std::fs::create_dir_all(dir) {
                eprintln!("niri-taskbar: cannot create {}: {err}", dir.display());
                return;
            }
        }
        match toml::to_string_pretty(self) {
            Ok(text) => {
                if let Err(err) = std::fs::write(&path, text) {
                    eprintln!("niri-taskbar: cannot write {}: {err}", path.display());
                }
            }
            Err(err) => eprintln!("niri-taskbar: cannot serialize config: {err}"),
        }
    }

    pub fn is_horizontal(&self) -> bool {
        matches!(self.position, Position::Top | Position::Bottom)
    }

    pub fn gear_size(&self) -> i32 {
        self.icon_size as i32 + 14
    }
}

fn config_path() -> std::path::PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            let home = std::env::var_os("HOME").unwrap_or_default();
            std::path::Path::new(&home).join(".config")
        });
    base.join("niri-taskbar").join("config.toml")
}
