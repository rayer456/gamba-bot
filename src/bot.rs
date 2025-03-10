use std::fmt::Display;

use std::sync::mpsc::{self, Receiver, Sender};

use std::thread::{self};
use std::time::Duration;

use crate::command::{self, Command};
use crate::config::Config;
use crate::message::User;
use crate::prediction::{self, Prediction, PredictionVariant};
use crate::signal::{BotSignal, TwitchApiSignal};
use crate::token::Token;
use crate::twitch::{self, TwitchApiClient};
use crate::{message::Message, stream::Stream};

use anyhow::{bail, Result};

use rand::Rng;
use reqwest::header::AUTHORIZATION;
use serde_json::Value;
use futures::join;
use tokio::{signal, spawn};
use tokio::sync::mpsc::{Receiver as TokioReceiver, Sender as TokioSender};

const USERS_URL: &'static str = "https://api.twitch.tv/helix/users";

pub struct Bot {
    pub irc_stream: Stream,
    pub cfg: Config,
    pub bot_token: Token,
    pub stream_token: Token,
    pub active_commands: Vec<Command>,
    pub loaded_predictions: Vec<Prediction>,

    pub tx_to_api_client: TokioSender<BotSignal>,
    pub rx_from_api_client: TokioReceiver<TwitchApiSignal>,
}

impl Bot {
    pub async fn initialize() -> Result<Self> {
        let cfg = Config::build("settings.toml")?;

        // async
        let (bot_token, stream_token, active_commands, predictions, irc_stream) = join!(
            Token::from_file(cfg.twitch_cfg.bot_token_path.clone(), cfg.clone()),
            Token::from_file(cfg.twitch_cfg.stream_token_path.clone(), cfg.clone()),
            command::get_commands(),
            prediction::get_predictions(),
            Stream::new(
                &cfg.twitch_cfg.irc_host,
                &cfg.twitch_cfg.irc_port,
                cfg.twitch_cfg.channel.clone(),
            )
        );

        // API channels
        let (tx_to_api_client, rx_from_bot) = tokio::sync::mpsc::channel(32);
        let (tx_to_bot, rx_from_api_client) = tokio::sync::mpsc::channel(32);

        // Run the Twitch API client in a separate thread
        let twitch_client = TwitchApiClient::new(rx_from_bot, tx_to_bot);
        spawn(async move {
            let _ = twitch::main_loop(twitch_client).await;
        });

        let mut bot = Bot {
            irc_stream: irc_stream?,
            cfg: cfg.clone(),
            bot_token: bot_token?,
            stream_token: stream_token?,
            active_commands: active_commands?,
            loaded_predictions: predictions?,

            tx_to_api_client,
            rx_from_api_client,
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

    pub async fn run(&mut self) {
        // main loop
        // let (sender, mut receiver) = mpsc::channel();
        loop {
            // read irc stream
            match self.irc_stream.read_irc() {
                Ok(messages) => {
                    for message in messages {
                        if let Some(command) = self.get_command_instance(message.clone()) {
                            self.run_command(command).await;
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
            self.read_channels().await;

            // hourly token validation
            self.stream_token.validate_if_invalid().await;
            self.bot_token.validate_if_invalid().await;
        }
    }

    async fn read_channels(&mut self) {
        if let Ok(signal) = self.rx_from_api_client.try_recv() {
            match signal {
                TwitchApiSignal::Unauthorized { command, reason } => self.respond_to_invalid_token(command, reason).await,
                TwitchApiSignal::BadRequest(reason) => println!("ERROR: 400 Bad Request: {reason}"),
                TwitchApiSignal::TooManyRequests => println!("ERROR: Too many requests lol"),
                TwitchApiSignal::Unknown { status, text }=> println!("ERROR: unknown response: {status}: {text}"),

                TwitchApiSignal::PredictionCreated => println!("INFO: created prediction via API"),

            }
        }
    }

    async fn respond_to_invalid_token(&mut self, command: Command, reason: String) {
        // TODO: Might be used for non command API calls too, will need to support other options than just a command
        if let Some(elapsed) = self.stream_token.last_refresh_elapsed() {
            if elapsed < Duration::from_secs(2) {
                // Really bad
                println!("ERROR: Stream token is being refreshed way too soon, 401's are being returned for a different reason.")
                return;
            }
        }

        println!("INFO: Token was likely invalid, refreshing. Reason: {reason}");
        if self.stream_token.refresh().await.is_err() {
            eprintln!("ERROR: Failed to refresh token. Won't attempt again.");
            return;
        }

        self.run_command(command).await;
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

    pub async fn run_command(&mut self, command: Command) {
        if let Some(response) = command.response.as_ref() {
            self.chat(response);
        }

        // Add command specific functionality here
        match command.cmd.as_str() {
            "pred" => self.prediction_router(command).await,
            _ => (), // Don't do any additional work for these commands, a response defined in commands.yaml was most likely already sent.
        };
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

    // TODO: put this somewhere else
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

    //// TODO: Think of moving the prediction functions outside of bot? client_id is static, access_token can be send via channel...

    async fn prediction_router(&mut self, command: Command) {
        let Some(pred_variant) = command.arguments.first() else { return };
        let pred_variant: PredictionVariant = pred_variant.as_str().into();
        let sub_argument = command.arguments.get(1).map_or("", |sa| sa.as_str()).to_owned();
        match pred_variant {
            PredictionVariant::Start => self.send_create_prediction_signal(command, sub_argument).await,
            PredictionVariant::Lock => self.chat("locking pred"),
            PredictionVariant::Outcome => self.chat("choosing outcome"),
            PredictionVariant::Cancel => self.chat("cancelling pred"),
            PredictionVariant::Invalid => self.chat("Possible arguments: start lock outcome cancel")
        }
    }

    async fn send_create_prediction_signal(&mut self, command: Command, prediction_name: String) {
        if !prediction::prediction_name_exists(&self.loaded_predictions, &prediction_name) {
            let preds_str = prediction::get_defined_predictions_as_str(&self.loaded_predictions);
            self.chat(format!("Prediction {prediction_name} not found. Available predictions: {preds_str}"));
            return;
        }

        let Some(prediction) = prediction::find_prediction_by_name(&self.loaded_predictions, &prediction_name) else { return };
        let _ = self.tx_to_api_client.send(BotSignal::CreatePrediction {
            client_id: self.cfg.twitch_cfg.client_id.clone(),
            access_token: self.stream_token.access_token.clone(),
            command,
            prediction: prediction.clone(),
        }).await;
    }
}
