use std::{cell::RefCell, env, fs, io, path::{Path, PathBuf}, rc::Rc};

use anyhow::{bail, Result};
use futures::future::ErrInto;
use serde::{Deserialize, Serialize};
use toml;

#[derive(Debug)]
pub enum ConfigError {
    FileNotFound,
    FileNotParseable,
    PermissionDenied,
    Unknown,
}

impl From<std::io::Error> for ConfigError {
    fn from(value: std::io::Error) -> Self {
        match value.kind() {
            io::ErrorKind::NotFound => ConfigError::FileNotFound,
            io::ErrorKind::PermissionDenied => ConfigError::PermissionDenied,
            _ => ConfigError::Unknown,
        }
    }
}

impl From<toml::de::Error> for ConfigError {
    fn from(_: toml::de::Error) -> Self {
        ConfigError::FileNotParseable
    }
}


impl From<ConfigError> for anyhow::Error {
    fn from(_value: ConfigError) -> anyhow::Error {
        anyhow::Error::msg("Config failed LOOOOOOOOOOOOOOL")
    }
}


#[derive(Serialize, Deserialize, Clone)]
pub struct Config {
    pub twitch_cfg: TwitchConfig,
    // Add other config stuff here later

    #[serde(skip)]
    pub save_path: PathBuf,
}

impl Config {
    pub fn from_path(path: PathBuf) -> Result<Config, ConfigError> {
        let file_contents = std::fs::read_to_string(&path)?;
        let mut config: Config = toml::from_str(&file_contents.as_str())?;
        config.save_path = path;

        Ok(config)
    }

    pub fn try_saving_default(path: PathBuf) -> Result<Config> {
        let mut cfg = Config::default();
        cfg.save_path = path.clone();
        let Some(parent_dirs) = path.parent() else { bail!("failed to create parent dirs") };
        let _ = fs::create_dir_all(parent_dirs);
        match cfg.update_file() {
            Ok(_) => return Ok(cfg),
            _ => {
                // should remove unused directories but fuck it
                bail!("Failed to save lol");
            },
        };
    }

    pub fn update_file(&self) -> Result<()> {
        println!("Trying to write to {:?}", &self.save_path);
        match toml::to_string(self) {
            Ok(deser) => std::fs::write(&self.save_path, deser.as_bytes())?,
            Err(e) => bail!(e),
        };

        Ok(())
    }

    
}

impl Default for Config {
    fn default() -> Config {
        Config {
            twitch_cfg: TwitchConfig::default(),
            save_path: PathBuf::default(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct TwitchConfig {
    pub client_id: String,
    pub client_secret: String,
    pub redirect_uri: String,
    pub listener: String,
    pub bot_scope: String,
    pub stream_scope: String,
    pub irc_host: String,
    pub irc_port: u16,
    pub account: String,
    pub channel: String,
    pub broadcaster_id: RefCell<String>,
    pub bot_token_path: String,
    pub stream_token_path: String,
}

// only used for token parsing lol
impl Default for TwitchConfig {
    fn default() -> Self {
        Self {
            client_id: Default::default(),
            client_secret: Default::default(),
            redirect_uri: String::from("http://localhost:8777"),
            listener: String::from("127.0.0.1:8777"),
            bot_scope: String::from("chat:edit+chat:read"),
            stream_scope: String::from("channel:manage:predictions+moderation:read"),
            irc_host: String::from("irc.chat.twitch.tv"),
            irc_port: 6667,
            account: Default::default(),
            channel: Default::default(),
            broadcaster_id: Default::default(),
            bot_token_path: String::from("tokens/bot_token.json"),
            stream_token_path: String::from("tokens/stream_token.json"),
        }
    }
}

pub fn find_settings_path() -> Option<PathBuf> {
    if let Ok(appdata_path) = env::var("APPDATA") {
        let settings_path = PathBuf::from(appdata_path).join("gamba-bot/config/settings.toml");
        if settings_path.exists() {
            return Some(settings_path);
        }
    }

    let settings_path = PathBuf::from("./config/settings.toml");
    if settings_path.exists() {
        return Some(settings_path);
    }
    
    None
}

pub fn try_saving_config_here(paths: [PathBuf; 2]) -> Result<Config> {
    for path in paths {
        if let Ok(cfg) = Config::try_saving_default(path) {
            return Ok(cfg);
        }
    }

    bail!("Couldn't save config anywhere wtf");
}
