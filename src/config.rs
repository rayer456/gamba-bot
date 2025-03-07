use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use toml;

#[derive(Serialize, Deserialize, Clone)]
pub struct Config {
    pub twitch_cfg: TwitchConfig,
    // Add other config stuff here later
}

impl Config {
    pub fn build(path: &str) -> Result<Config> {
        let file_contents = std::fs::read_to_string(path)?;
        let config: Config = toml::from_str(&file_contents.as_str())?;

        Ok(config)
    }

    pub fn update_file(&self) -> Result<()> {
        match toml::to_string(self) {
            Ok(deser) => std::fs::write("settings.toml", deser.as_bytes())?,
            Err(e) => bail!(e),
        };

        Ok(())
    }
}

impl Default for Config {
    fn default() -> Config {
        Config {
            twitch_cfg: TwitchConfig::default(),
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
    pub broadcaster_id: String,
    pub bot_token_path: String,
    pub stream_token_path: String,
}

impl Default for TwitchConfig {
    fn default() -> Self {
        Self {
            client_id: Default::default(),
            client_secret: Default::default(),
            redirect_uri: Default::default(),
            listener: Default::default(),
            bot_scope: Default::default(),
            stream_scope: Default::default(),
            irc_host: Default::default(),
            irc_port: Default::default(),
            account: Default::default(),
            channel: Default::default(),
            broadcaster_id: Default::default(),
            bot_token_path: Default::default(),
            stream_token_path: Default::default(),
        }
    }
}
