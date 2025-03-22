use anyhow::{bail, Result};
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use serde::{Deserialize, Deserializer, Serialize};
use std::{rc::Rc, time::{Duration, Instant, SystemTime}};

use crate::{config::Config, TOKEN_ENDPOINT};

const VALIDATION_ENDPOINT: &'static str = "https://id.twitch.tv/oauth2/validate";

pub enum TokenType {
    Streamer,
    Bot,
}

// Only here so serde doesn't complain. This default will always be reset in Token::new()
impl Default for TokenType {
    fn default() -> Self {
        TokenType::Streamer 
    }
}

#[derive(Deserialize)]
pub struct Token {
    // Mandatory
    pub access_token: String,
    refresh_token: String,

    #[serde(skip)]
    cfg: Rc<Config>,
    
    #[serde(skip)]
    token_type: TokenType,

    #[serde(skip, default = "SystemTime::now")]
    last_validated: SystemTime,

    #[serde(skip)]
    pub last_refreshed: Option<Instant>,
}

impl Token {
    pub async fn new(cfg_clone: Rc<Config>, token_type: TokenType) -> Result<Token> {
        let path = match token_type {
            TokenType::Streamer => &cfg_clone.twitch_cfg.stream_token_path,
            TokenType::Bot => &cfg_clone.twitch_cfg.bot_token_path,
        };
        let file_content = std::fs::read_to_string(path)?;
        let mut token: Token = serde_json::from_str(file_content.as_str())?;
        token.cfg = cfg_clone;
        token.token_type = token_type;

        match token.validate().await {
            Ok(_) => return Ok(token),
            Err(e) => bail!(e),
        }
    }

    pub async fn refresh(&mut self) -> Result<()> {
        let params: [(&str, &str); 4] = [
            ("client_id", &self.cfg.twitch_cfg.client_id),
            ("client_secret", &self.cfg.twitch_cfg.client_secret),
            ("grant_type", "refresh_token"),
            ("refresh_token", &self.refresh_token),
        ];
        let client = reqwest::Client::new();
        let response = client
            .post(TOKEN_ENDPOINT)
            .header(CONTENT_TYPE, "x-www-form-urlencoded")
            .form(&params)
            .send()
            .await;

        match response {
            Ok(res) => {
                let status_code = res.status().as_u16();
                let response = res.text().await?;

                if status_code != 200 {
                    bail!("ERROR: Refreshing tokens:\nStatus code: {status_code}\nReason:{response}")
                }

                self.last_refreshed = Some(Instant::now());
                println!("refreshed.");

                // Write response to file
                let path = match self.token_type {
                    TokenType::Streamer => &self.cfg.twitch_cfg.stream_token_path,
                    TokenType::Bot => &self.cfg.twitch_cfg.bot_token_path,
                };
                std::fs::write(path, response.as_bytes())?;

                // Update fields
                let new_token: Token = serde_json::from_str(response.as_str())?;
                self.access_token = new_token.access_token;
                self.refresh_token = new_token.refresh_token;

                return Ok(());
            }
            Err(e) => bail!(e),
        }
    }

    pub async fn validate(&mut self) -> Result<()> {
        // Returns an error when refresh failed, or a different status code was received, or when the request itself failed.

        let client = reqwest::Client::new();
        let response = client
            .get(VALIDATION_ENDPOINT)
            .header(AUTHORIZATION, format!("OAuth {}", self.access_token))
            .send()
            .await;

        match response {
            Ok(res) => {
                self.last_validated = SystemTime::now();
                match res.status().as_u16() {
                    200 => return Ok(()),
                    401 => return self.refresh().await,
                    other => bail!("ERROR: Status code was {other} when validating, expected 200 or 401."),
                };
            }
            Err(e) => bail!(e),
        }
    }

    pub async fn validate_if_invalid(&mut self) {
        if let Ok(elapsed) = self.last_validated.elapsed() {
            if elapsed >= Duration::from_secs(3600) {
                self.validate().await.ok();
            }
        }
    }

    pub fn last_refresh_elapsed(&self) -> Option<Duration> {
        self.last_refreshed.map(|instant| instant.elapsed())
    }
}

