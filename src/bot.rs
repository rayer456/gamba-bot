use std::fmt::Display;

use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self};

use crate::command::{self, AppError, Command};
use crate::config::Config;
use crate::message::User;
use crate::token::Token;
use crate::{message::Message, stream::Stream};

use anyhow::{bail, Result};

use rand::Rng;
use reqwest::header::AUTHORIZATION;
use serde_json::Value;
use futures::join;

const USERS_URL: &'static str = "https://api.twitch.tv/helix/users";

pub struct Bot {
    pub irc_stream: Stream,
    pub cfg: Config,
    pub bot_token: Token,
    pub stream_token: Token,
    pub active_commands: Vec<Command>,
}

impl Bot {
    pub async fn initialize() -> Result<Self> {
        let cfg = Config::build("settings.toml")?;

        // async
        let (bot_token, stream_token, active_commands, irc_stream) = join!(
            Token::from_file(cfg.twitch_cfg.bot_token_path.clone(), cfg.clone()),
            Token::from_file(cfg.twitch_cfg.stream_token_path.clone(), cfg.clone()),
            command::get_commands(),
            Stream::new(
                &cfg.twitch_cfg.irc_host,
                &cfg.twitch_cfg.irc_port,
                cfg.twitch_cfg.channel.clone(),
            )
        );

        let mut bot = Bot {
            irc_stream: irc_stream?,
            cfg: cfg.clone(),
            bot_token: bot_token?,
            stream_token: stream_token?,
            active_commands: active_commands?,
        };

        if bot.cfg.twitch_cfg.broadcaster_id.is_empty() {
            bot.cfg.twitch_cfg.broadcaster_id = bot.get_broadcaster_id(&bot.cfg.twitch_cfg.channel.clone()).await?;
            bot.cfg.update_file()?;
        }

        match bot.irc_stream.connect_to_irc(
            &bot.cfg.twitch_cfg.account,
            &bot.cfg.twitch_cfg.channel,
            &bot.bot_token.access_token,
        ) {
            Ok(_) => return Ok(bot),
            Err(e) => bail!(e),
        }
    }

    pub async fn run(&mut self) -> Result<()> {
        // main loop
        let (sender, receiver) = mpsc::channel();
        loop {
            // read irc stream
            match self.irc_stream.read_irc() {
                Ok(messages) => {
                    for message in messages {
                        if let Some(command) = self.get_command_instance(message.clone()) {
                            self.run_command(command, sender.clone()).await;
                        }
                    }
                }
                Err(err) => {
                    // println!("{err}")
                    /* match err.kind() {
                        ErrorKind::ConnectionRefused => println!("connected refused"),
                        ErrorKind::ConnectionReset => println!("connection reset"),
                        ErrorKind::TimedOut => println!("no message"),
                        other => println!("connection error: {other}"),
                    }; */
                    /* if err.kind() == ErrorKind::ConnectionReset {
                        if let Ok(new_stream) = Stream::new(
                            &self.cfg.twitch_cfg.irc_host,
                            &self.cfg.twitch_cfg.irc_port,
                            self.cfg.twitch_cfg.channel.clone(),
                        ) {
                            self.irc_stream = new_stream;
                            println!("replaced irc stream");
                            match self.irc_stream.connect_to_irc(
                                &self.cfg.twitch_cfg.account,
                                &self.cfg.twitch_cfg.channel,
                                &self.bot_token.access_token,
                            ) {
                                Ok(_) => (),
                                Err(e) => bail!(e),
                            }
                        } else {
                            println!("couldn't create new IRC stream");
                        } // TODO: ideally reconnect, but if you can't, create a new one instead
                    } */
                }
            }

            // read the channel
            self.read_channel(sender.clone(), &receiver).await;

            // hourly token validation
            self.stream_token.validate_if_invalid().await;
            self.bot_token.validate_if_invalid().await;
        }
    }

    async fn read_channel(
        &mut self,
        sender: Sender<Result<String, AppError>>,
        receiver: &Receiver<Result<String, AppError>>,
    ) {
        if let Ok(res) = receiver.try_recv() {
            match res {
                Ok(reply) => self.chat(reply),
                Err(err) => match err {
                    AppError::InvalidTokenError {
                        cmd,
                        arguments,
                        requested_by,
                    } => self.respond_to_invalid_token(sender, cmd, arguments, requested_by).await,
                    AppError::OtherError(err) => println!("{err}"),
                },
            }
        }
    }

    async fn respond_to_invalid_token(
        &mut self,
        sender: Sender<Result<String, AppError>>,
        cmd: String,
        arguments: Vec<String>,
        requested_by: Option<User>,
    ) {
        if self.stream_token.refresh().await.is_err() {
            eprintln!("ERROR: Failed to refresh token. Won't attempt again.");
            return;
        }
        // No need to call get_command_instance() since the command failed the first time, thus was never executed in case of an InvalidTokenError
        let mut command = match self.find_command_by_cmd(cmd) {
            Some(cmd) => cmd,
            None => return, // Shouldn't be possible since the given command was already activated once. 
        };

        command.arguments = arguments;
        command.requested_by = requested_by;
        self.run_command(command, sender.clone()).await;
    }

    pub fn get_command_instance(&mut self, mut message: Message) -> Option<Command> {
        let (option, arguments) = self.find_command_by_message(&message);

        // Create an instance of the given command
        // An instance of a command might be different for each instance
        // E.g. The user who called the command or the arguments to the command might differ per use
        match command::validate_and_return_command(option, &mut message) {
            Some(c) => {
                let mut instance = c.clone();
                instance.arguments = arguments;
                instance.requested_by = Some(message.user);
                return Some(instance);
            }
            None => return None,
        }
    }

    pub async fn run_command(&mut self, command: Command, sender: Sender<Result<String, AppError>>) {
        if let Some(response) = command.response.as_ref() {
            self.chat(response);
        }

        // Add command specific functionality here
        match command.cmd.as_str() {
            
            _ => (), // Don't do any additional work for these commands, a response defined in commands.yaml was most likely already sent.
        }
    }

    pub fn find_command_by_message(&mut self, msg: &Message) -> (Option<&mut Command>, Vec<String>) {
        let split_message: Vec<String> = msg.message.split(' ').map(|m| m.to_string()).collect();

        if let Some((command, arguments)) = split_message.split_first() {
            for active_command in &mut self.active_commands {
                if *command == active_command.cmd || active_command.alternative_cmds.contains(command) {
                    return (Some(active_command), arguments.to_vec());
                }
            }
        }
        (None, Vec::new())
    }

    pub fn find_command_by_cmd(&mut self, cmd: String) -> Option<Command> {
        for active_command in self.active_commands.iter() {
            if cmd == active_command.cmd || active_command.alternative_cmds.contains(&cmd) {
                return Some(active_command.clone());
            }
        }
        None
    }

    pub fn chat<T: Display>(&mut self, message: T) {
        if let Err(e) = self.irc_stream.send_chat_message(message) {
            eprintln!("{e}");
        }
    }

    pub async fn get_broadcaster_id(&mut self, channel: &String) -> Result<String> {
        // https://dev.twitch.tv/docs/api/reference/#get-users

        let params = [("login", channel)];
        let client = reqwest::Client::new();
        let response = client
            .get(USERS_URL)
            .header(AUTHORIZATION, format!("Bearer {}", self.stream_token.access_token))
            .header("Client-Id", &self.cfg.twitch_cfg.client_id)
            .query(&params)
            .send()
            .await?;

        match response.status().as_u16() {
            200 => {
                let text = response.text().await?;
                match serde_json::from_str::<Value>(&text)?["data"][0]["id"].as_str() {
                    Some(id) => return Ok(id.to_string()),
                    None => {
                        bail!("Field 'id' was not found in the response or was not of type str.")
                    }
                }
            }
            401 => {
                self.stream_token.refresh().await?;
                return Box::pin(self.get_broadcaster_id(channel)).await;
            }
            other => {
                bail!("ERROR: Status code was {other} when trying to get the broadcaster ID, expected 200 or 401.")
            }
        }
    }
}
