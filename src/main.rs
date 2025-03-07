mod bot;
mod command;
mod config;
mod message;
mod authorize;
mod stream;
mod token;

use std::{env, time::SystemTime};

use anyhow::Result;

use bot::Bot;

pub const TOKEN_ENDPOINT: &'static str = "https://id.twitch.tv/oauth2/token";

#[tokio::main]
async fn main() -> Result<()> {
    // Authorize first
    let config = config::Config::build("settings.toml")?;
    let mut auth_process = authorize::TwitchAuthProcess::create(&config.twitch_cfg);

    let yes_authorize = false;
    if yes_authorize {
        let _ = auth_process.authorize_account(&config.twitch_cfg.stream_token_path, &config.twitch_cfg.stream_scope);
        let _ = auth_process.authorize_account(&config.twitch_cfg.bot_token_path, &config.twitch_cfg.bot_scope);
    }

    // Run bot
    let mut bot = Bot::initialize().await?;
    bot.run().await?;

    Ok(())
}
