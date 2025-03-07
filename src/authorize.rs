use std::{
    io::{BufRead, BufReader, Read, Write},
    net::{TcpListener, TcpStream},
};

use anyhow::{bail, Result};
use reqwest::header::{AUTHORIZATION, CONTENT_LENGTH, CONTENT_TYPE};

use config::{TwitchConfig};

use crate::config;

const TWITCH_AUTH_URL: &'static str = "https://id.twitch.tv/oauth2/authorize";
const TWITCH_TOKEN_URL: &'static str = "https://id.twitch.tv/oauth2/token";

pub struct TwitchAuthProcess {
    cfg: TwitchConfig,
    tokens_path: String,
}

impl TwitchAuthProcess {
    pub fn create(cfg: &TwitchConfig) -> TwitchAuthProcess {
        TwitchAuthProcess {
            cfg: cfg.clone(),
            tokens_path: String::new(),
        }
    }

    pub fn authorize_account(&mut self, tokens_path: &str, scope: &str) -> Result<()> {
        self.tokens_path = tokens_path.to_string();

        let url = self.construct_auth_url(scope);
        match open::that(&url) {
            Ok(_) => println!("Opened authorization page successfully in browser"),
            Err(_) => println!("Couldn't open authorization page. Go to {url} and authorize"),
        }
        self.start_server()
    }

    fn construct_auth_url(&self, scope: &str) -> String {
        format!("{TWITCH_AUTH_URL}?client_id={client_id}&redirect_uri={redirect_uri}&response_type=code&scope={scope}&force_verify=true",
        client_id = self.cfg.client_id,
        redirect_uri = self.cfg.redirect_uri)
    }

    fn start_server(&self) -> Result<()> {
        let listener = TcpListener::bind(&self.cfg.listener)?;
        for stream in listener.incoming() {
            match self.parse_inc_request(stream?) {
                Ok(_) => break,
                Err(_) => continue,
            }
        }
        Ok(())
    }

    fn parse_inc_request(&self, mut stream: TcpStream) -> Result<Option<()>> {
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

        // assuming the code param is first
        let first_param: String = first_line[1]
            .chars()
            .skip_while(|c| *c != '?')
            .skip(1)
            .take_while(|c| *c != '&')
            .collect();
        let first_param: Vec<&str> = first_param.split('=').collect();

        let response: String;
        let mut token_received = false;
        if first_param[0] != "code" {
            response = format!(
                "HTTP/1.1 200 OK\r\n\r\n<html>
            <body>
            <h1>No code parameter found in URL!</h1>
            </body>
            </html>"
            );
        } else {
            let auth_code = first_param[1];
            response = format!(
                "HTTP/1.1 200 OK\r\n\r\n<html>
            <body>
            <h1>Code parameter found: {auth_code}</h1>
            </body>
            </html>"
            );
            token_received = self.get_tokens_with_code(auth_code.to_string()).is_some();
        }

        let _ = stream.write(response.as_bytes());
        if token_received {
            return Ok(Some(()));
        }

        Ok(None)
    }

    fn get_tokens_with_code(&self, code: String) -> Option<()> {
        let client = reqwest::blocking::Client::new();
        let params = [
            ("client_id", &self.cfg.client_id),
            ("client_secret", &self.cfg.client_secret),
            ("code", &code),
            ("grant_type", &"authorization_code".to_string()),
            ("redirect_uri", &self.cfg.redirect_uri),
        ];
        let response = client
            .post(TWITCH_TOKEN_URL)
            .header(CONTENT_TYPE, "x-www-form-urlencoded")
            .form(&params)
            .send();

        match response {
            Ok(res) => {
                let status_code = res.status().as_u16();
                if status_code != 200 {
                    return None;
                }

                let response_txt = res.text().ok()?;
                std::fs::create_dir("/tokens").ok();
                std::fs::write(&self.tokens_path, response_txt.as_bytes()).ok()?;

                return Some(());
            }
            Err(_) => return None,
        };
    }
}