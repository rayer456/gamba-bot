mod bot;
mod command;
mod config;
mod message;
mod authorize;
mod stream;
mod token;
mod helpers;
mod prediction;
mod twitch;
mod signal;

use std::{backtrace, env, time::Duration};

use anyhow::Result;

use bot::Bot;
use tokio::{spawn, time::sleep};

pub const TOKEN_ENDPOINT: &'static str = "https://id.twitch.tv/oauth2/token";

#[tokio::main]
async fn main() -> Result<()> {
    // Authorize first
    env::set_var("RUST_BACKTRACE", "0");
    let config = config::Config::build("settings.toml")?;
    let mut auth_process = authorize::TwitchAuthProcess::create(&config.twitch_cfg);

    let yes_authorize = false;
    if yes_authorize {
        match auth_process.authorize_account(&config.twitch_cfg.stream_token_path, &config.twitch_cfg.stream_scope).await {
            Ok(_) => println!("Successfully authorized Streamer\n"),
            Err(e) => println!("Couldn't authorize Streamer\nReason:{e}\n"),
        };
        match auth_process.authorize_account(&config.twitch_cfg.bot_token_path, &config.twitch_cfg.bot_scope).await {
            Ok(_) => println!("Successfully authorized Bot\n"),
            Err(e) => println!("Couldn't authorize Bot\nReason:{e}\n"),
        };
    }

    // Run bot
    println!("Starting bot...");
    let mut bot = Bot::initialize().await?;
    bot.run().await;

    Ok(())
}
