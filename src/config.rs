use serde::Deserialize;
use std::path::PathBuf;

#[derive(Deserialize, Default)]
pub struct Config {
    #[serde(default)]
    pub line_numbers: bool,
    pub level: Option<String>,
}

pub fn load_config() -> Config {
    if let Some(path) = config_path() {
        if path.exists() {
            if let Ok(contents) = std::fs::read_to_string(&path) {
                if let Ok(cfg) = toml::from_str(&contents) {
                    return cfg;
                }
            }
        }
    }
    Config::default()
}

fn config_path() -> Option<PathBuf> {
    // check current directory first
    let local = PathBuf::from(".loggrep.toml");
    if local.exists() {
        return Some(local);
    }

    // then XDG/home config
    dirs::config_dir().map(|d| d.join("loggrep").join("config.toml"))
}
