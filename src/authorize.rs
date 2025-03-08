use core::panic;
use std::{
    env, io::{BufRead, BufReader, Read, Write}, net::{TcpListener, TcpStream}
};

use anyhow::{bail, Result};
use reqwest::{header::{AUTHORIZATION, CONTENT_LENGTH, CONTENT_TYPE}, Client};
use urlencoding::encode;

use config::{TwitchConfig};

use crate::{config, helpers};

const TWITCH_AUTH_URL: &'static str = "https://id.twitch.tv/oauth2/authorize";
const TWITCH_TOKEN_URL: &'static str = "https://id.twitch.tv/oauth2/token";

pub struct TwitchAuthProcess {
    cfg: TwitchConfig,
    tokens_path: String,
    http_client: Client,
    state: String,
}

impl TwitchAuthProcess {
    pub fn create(cfg: &TwitchConfig) -> TwitchAuthProcess {
        TwitchAuthProcess {
            cfg: cfg.clone(),
            tokens_path: String::new(),
            http_client: Client::new(),
            state: String::new(),
        }
    }

    pub async fn authorize_account(&mut self, tokens_path: &str, scope: &str) -> Result<()> {
        self.tokens_path = tokens_path.to_string();
        self.state = helpers::get_rand_string(50);
        let url = self.construct_auth_url(scope, &self.state);

        match open::that(&url) {
            Ok(_) => println!("Opened authorization page successfully in browser"),
            Err(_) => println!("Couldn't open authorization page. Go to {url} and authorize"),
        }
        self.start_server().await
    }

    fn construct_auth_url(&self, scope: &str, state: &str) -> String {
        format!("{TWITCH_AUTH_URL}?client_id={client_id}&redirect_uri={redirect_uri}&response_type=code&scope={scope}&force_verify=true&state={state}",
            client_id = self.cfg.client_id,
            redirect_uri = self.cfg.redirect_uri.as_str(),
        )
    }

    async fn start_server(&mut self) -> Result<()> {
        let listener = TcpListener::bind(&self.cfg.listener)?;
        for stream in listener.incoming() {
            match self.parse_inc_request(stream?).await {
                Ok(_) => break,
                Err(e) => {
                    println!("{e}");
                    continue
                },
            }
        }
        Ok(())
    }

    async fn parse_inc_request(&mut self, mut stream: TcpStream) -> Result<Option<()>> {
        let mut reader = BufReader::new(stream.try_clone()?);

        let mut array_of_bytes = [0; 1024];
        reader.read(&mut array_of_bytes)?;

        let request_as_str = String::from_utf8(array_of_bytes.to_vec()).unwrap();
        let lines: Vec<&str> = request_as_str.lines().collect();
        let first_line: Vec<&str> = lines[0].split(' ').collect();

        // sometimes happens, don't know why
        if first_line.len() == 1 {
            println!("{:?}", lines);
            bail!("While parsing request: Unclear HTTP request.")
        }

        let url = first_line[1];
        let parameters = helpers::extract_parameters(url);

        let response: String;
        if let Some(auth_code) = parameters.get("code") {
            response = format!(
                "HTTP/1.1 200 OK\r\n\r\n<html>
            <body>
            <h1>Code parameter found: {auth_code}</h1>
            </body>
            </html>"
            );
        
            // check state
            let Some(received_state) = parameters.get("state") else { bail!("state wasn't returned") };
            if *received_state != self.state { bail!("returned state is wrong") };

            let tokens_txt = self.get_tokens_with_code(auth_code.to_string()).await?;
            let _ = std::fs::create_dir("./tokens");
            std::fs::write(&self.tokens_path, tokens_txt.as_bytes())?;
        } else if let Some(e) = parameters.get("error") {
            bail!("{e}");
        }
        else {
            bail!("shit request");
        }

        let _ = stream.write(response.as_bytes());

        Ok(None)
    }

    async fn get_tokens_with_code(&mut self, code: String) -> Result<String> {
        let params = [
            ("client_id", &self.cfg.client_id),
            ("client_secret", &self.cfg.client_secret),
            ("code", &code),
            ("grant_type", &"authorization_code".to_string()),
            ("redirect_uri", &self.cfg.redirect_uri),
        ];
        let response = self.http_client
            .post(TWITCH_TOKEN_URL)
            .header(CONTENT_TYPE, "x-www-form-urlencoded")
            .form(&params)
            .send().await;

        match response {
            Ok(res) => {
                let status_code = res.status().as_u16();
                if status_code != 200 {
                    println!("{code}");
                    bail!("No tokens dumbass: {}", res.text().await?);
                }

                return Ok(res.text().await?);
            }
            Err(e) => bail!("{e}"),
        };
    }
}