use anyhow::{bail, Result};
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use serde::{Deserialize, Deserializer};
use std::time::{Duration, Instant, SystemTime};

use crate::{config::Config, TOKEN_ENDPOINT};

const VALIDATION_ENDPOINT: &'static str = "https://id.twitch.tv/oauth2/validate";

#[derive(Deserialize)]
pub struct Token {
    // Mandatory
    pub access_token: String,
    refresh_token: String,

    #[serde(skip)]
    config: Config,

    #[serde(skip)]
    path: String,

    #[serde(skip, default = "SystemTime::now")]
    last_validated: SystemTime,

    #[serde(skip)]
    pub last_refreshed: Option<Instant>,
}

impl Token {
    pub async fn from_file(path: String, config: Config) -> Result<Token> {
        let file_content = std::fs::read_to_string(&path)?;
        let mut token: Token = serde_json::from_str(file_content.as_str())?;
        token.path = path;
        token.config = config;

        match token.validate().await {
            Ok(_) => return Ok(token),
            Err(e) => bail!(e),
        }
    }

    pub async fn refresh(&mut self) -> Result<()> {
        let params = [
            ("client_id", &self.config.twitch_cfg.client_id),
            ("client_secret", &self.config.twitch_cfg.client_secret),
            ("grant_type", &String::from("refresh_token")),
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
                std::fs::write(&self.path, response.as_bytes())?;

                // Update properties
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

