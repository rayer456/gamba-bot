use core::panic;
use std::fmt::Display;

use std::path::PathBuf;
use std::rc::Rc;

use std::time::Duration;

use crate::command::{self, Command};
use crate::config::Config;
use crate::eventsub::{self, EventType, EventsubClientError, SubEvent, SubToEventData, WSAction};
use crate::prediction::{self, EndPredictionData, Outcome, Prediction, PredictionCommandVariant, PredictionStatus};
use crate::signal::TwitchApiSignal;
use crate::token::Token;
use crate::twitch::{self, TwitchCommonParameters};
use crate::websocket::Transport;
use crate::{message::Message, stream::Stream};
use crate::token::TokenType;

use anyhow::{bail, Result};

use chrono::{DateTime, FixedOffset, Utc};
use reqwest::header::AUTHORIZATION;
use reqwest::Client;
use serde_json::{json, Value};
use futures::join;
use tokio::spawn;
use tokio::sync::mpsc::{Receiver as TokioReceiver, Sender as TokioSender};

const USERS_URL: &'static str = "https://api.twitch.tv/helix/users";

pub struct Bot {
    pub irc_stream: Stream,
    pub cfg: Rc<Config>,
    pub bot_token: Token,
    pub stream_token: Token,
    pub active_commands: Vec<Command>,
    pub loaded_predictions: Vec<Prediction>,
    prediction_reminder_time: Option<DateTime<Utc>>,

    http_client: Client,
    tx_to_bot: TokioSender<TwitchApiSignal>,
    pub bot_rx: TokioReceiver<TwitchApiSignal>, // TODO: doesn't need to be tokioreceiver
    evs_receiver: TokioReceiver<Result<WSAction, EventsubClientError>>,
}

impl Bot {
    pub async fn initialize() -> Result<Self> {
        let cfg = Config::from_path(PathBuf::from("settings.toml"))?;
        

        println!("Running {} version {}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));

        let cfg_rc: Rc<Config> = Rc::new(cfg.clone());
        
        // async
        let (bot_token, stream_token, active_commands, predictions, irc_stream) = join!(
            Token::new(
                Rc::clone(&cfg_rc),
                TokenType::Bot,
            ),
            Token::new(
                Rc::clone(&cfg_rc),
                TokenType::Streamer,
            ),
            command::get_commands(),
            prediction::get_predictions(),
            Stream::new(
                &cfg.twitch_cfg.irc_host,
                &cfg.twitch_cfg.irc_port,
                cfg.twitch_cfg.channel.clone(),
            )
        );

        // API channels
        let (evs_sender, evs_receiver) = tokio::sync::mpsc::channel(64);
        eventsub::run_eventsub_client(evs_sender); // TODO: should probably have some kind of handler to this thread

        let (tx_to_bot, bot_rx) = tokio::sync::mpsc::channel(32);

        // let twitch_client = TwitchApiClient::new(rx_from_bot, tx_to_bot);
        

        let mut bot = Bot {
            irc_stream: irc_stream?,
            cfg: cfg_rc,
            bot_token: bot_token?,
            stream_token: stream_token?,
            active_commands: active_commands?,
            loaded_predictions: predictions?,
            prediction_reminder_time: None,

            http_client: Client::new(),
            tx_to_bot,
            bot_rx,
            evs_receiver,
        };

        bot.update_broadcaster_id().await?;

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
                        if let Some(command) = self.get_command_instance(message) {
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

            // read the channels
            self.read_twitch_channel().await;
            self.read_eventsub_channel().await;

            self.check_prediction_reminder_time();

            // hourly token validation
            self.stream_token.validate_if_invalid().await;
            self.bot_token.validate_if_invalid().await;
        }
    }

    fn check_prediction_reminder_time(&mut self) {
        let Some(reminder_time) = self.prediction_reminder_time else {
            return;
        };

        if Utc::now() >= reminder_time {
            self.chat("30 seconds left to make your prediction!");
            self.prediction_reminder_time = None;
        }
    }

    async fn read_twitch_channel(&mut self) {
        if let Ok(signal) = self.bot_rx.try_recv() {
            match signal {
                TwitchApiSignal::Unauthorized { command, reason } => self.respond_to_invalid_token(command, reason).await,
                TwitchApiSignal::BadRequest(reason) => println!("ERROR: 400 Bad Request: {reason}"),
                TwitchApiSignal::TooManyRequests => println!("ERROR: Too many requests lol"),
                TwitchApiSignal::Unknown { status, text }=> println!("ERROR: unknown response: {status}: {text}"),

                TwitchApiSignal::PredictionCreated => println!("INFO: created prediction via API"),
                TwitchApiSignal::PredictionStillActive => {
                    self.chat("Prediction still active, use arguments [outcome, cancel] to end.");
                    // log
                },
                _ => ()
            }
        }
    }

    async fn read_eventsub_channel(&mut self) {
        if let Ok(action_or_error) = self.evs_receiver.try_recv() { // TODO: Use let else here to avoid shit getting too nested
            // Do things based on action and log errors for now
            // Probably create new function in bot to handle actions? Or at least define said actions
            if let Ok(action) = action_or_error {
                match action {
                    WSAction::SessionWelcome { session_id } => {
                        let condition: Value = json!({ "broadcaster_user_id": self.cfg.twitch_cfg.broadcaster_id }); // same for all events in this case
                        self.sub_twitch_events(&session_id, SubEvent::ChannelPredictionBegin, &condition);
                        self.sub_twitch_events(&session_id, SubEvent::ChannelPredictionLock, &condition);
                        self.sub_twitch_events(&session_id, SubEvent::ChannelPredictionEnd, &condition);
                    }
                    WSAction::SessionKeepAlive => println!("keep alive message"),

                    // Check electrobot
                    WSAction::Notification { event_type } => self.handle_twitch_events(event_type),
                    WSAction::SessionReconnect => (),
                    WSAction::Revocation => (),
                };

            };
        }
    }

    fn sub_twitch_events(&self, session_id: &str, sub_event: SubEvent, condition_object: &Value) {
        let http_client_c = self.http_client.clone();
        let tx_to_bot_c = self.tx_to_bot.clone();
        let common_paras = self.get_common_twitch_parameters();
        let data = SubToEventData {
            _type: sub_event.as_str().to_string(),
            version: "1".to_string(),
            condition: condition_object.clone(),
            transport: Transport {
                method: "websocket".to_string(),
                session_id: session_id.to_string(),
            }
        };

        spawn(async move {
            let res = twitch::sub_to_event(
                http_client_c,
                common_paras,
                tx_to_bot_c,
                data
            ).await;

            match res {
                Ok(_) => println!("Finished sub_to_event call"),
                Err(e) => (),
            }

        });
    }

    fn handle_twitch_events(&mut self, event_type: EventType) {
        // TODO: Why not do this in the eventsub thread and send the result to chat immediately?
        match event_type {
            EventType::ChannelPredictionBegin { locks_at } => self.handle_event_prediction_begin(locks_at),
            EventType::ChannelPredictionLock { outcomes } => self.handle_event_prediction_lock(outcomes),
            EventType::ChannelPredictionEnd { winning_id, status, outcomes } => {
                println!("Status is {status} with winning ID: {winning_id}"); // possible values: resolved, canceled
                self.handle_event_prediction_end(winning_id, status, outcomes);
            }
        }
    }

    fn handle_event_prediction_begin(&mut self, locks_at: String) {
        self.chat("Prediction has started!");

        match eventsub::get_prediction_reminder_time(locks_at) {
            Ok(datetime) => self.prediction_reminder_time = Some(datetime.to_utc()),
            Err(e) => {
                println!("Failed to get datetime, reason: {e}");
                return;
            }
        };
    }

    fn handle_event_prediction_lock(&mut self, outcomes: Vec<Outcome>) {
        // bets are closed 40/60 split (2 betters, pool: 224) pausefish

        self.prediction_reminder_time = None; // Don't remind after locking manually

        let (split_str, total_points, total_users) = prediction::get_prediction_lock_vars_from_outcomes(outcomes);

        self.chat(format!("Bets are closed, {split_str} split ({total_users} betters, pool: {total_points}) PauseFish"));
    }
    
    fn handle_event_prediction_end(&mut self, winning_id: String, status: String, outcomes: Vec<Outcome>) {
        self.prediction_reminder_time = None;
        match status.to_uppercase().as_str() {
            "RESOLVED" => {
                let (winner_string, loser_string) = prediction::get_prediction_end_vars(winning_id, outcomes).unwrap_or_else(|_| (String::from("test"), String::from("test")));
                self.chat(format!("{winner_string}"));
                self.chat(format!("{loser_string}"));

            },
            "CANCELED" => {
                self.chat("Prediction canceled");
            },
            _ => return,
        };
    }

    async fn respond_to_invalid_token(&mut self, command: Command, reason: String) {
        // TODO: Might be used for non command API calls too, will need to support other options than just a command
        if let Some(elapsed) = self.stream_token.last_refresh_elapsed() {
            if elapsed < Duration::from_secs(10) {
                // Really bad
                println!("ERROR: Stream token is being refreshed way too soon, 401's are being returned for a different reason.");
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
        let (option, arguments) = command::find_command_by_message(&mut self.active_commands, &message);

        // Create an instance of the given command
        // An instance of a command might be different for each instance
        // E.g. The user who called the command or the arguments to the command might differ per use
        match command::validate_and_return_command(option, &mut message) {
            Some(c) => {
                let mut instance = c.clone(); // turn command definition into an instance
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

    // TODO: probably remove
    // pub fn find_command_by_cmd(&mut self, cmd: String) -> Option<Command> {
    //     for active_command in self.active_commands.iter() {
    //         if cmd == active_command.cmd || active_command.alternative_cmds.contains(&cmd) {
    //             return Some(active_command.clone());
    //         }
    //     }
    //     None
    // }

    pub fn chat<T: Display>(&mut self, message: T) {
        if let Err(e) = self.irc_stream.send_chat_message(message) {
            eprintln!("{e}");
        }
    }

    // TODO: put this somewhere else
    pub async fn update_broadcaster_id(&mut self) -> Result<()> {
        // https://dev.twitch.tv/docs/api/reference/#get-users

        let params = [("login", &self.cfg.twitch_cfg.channel)];
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
                    Some(id) => {
                        *self.cfg.twitch_cfg.broadcaster_id.borrow_mut() = id.to_string();  

                        self.cfg.update_file()?;

                        Ok(())
                    },
                    None => {
                        bail!("Field 'id' was not found in the response or was not of type str.")
                    }
                }
            }
            401 => {
                self.stream_token.refresh().await?;
                return Box::pin(self.update_broadcaster_id()).await; // Dangerous!
            }
            other => {
                bail!("ERROR: Status code was {other} when trying to get the broadcaster ID, expected 200 or 401.")
            }
        }
    }


    //// TODO: Think of moving the prediction functions outside of bot? client_id is static, access_token can be send via channel...
    
    fn get_common_twitch_parameters(&self) -> TwitchCommonParameters {
        TwitchCommonParameters::new(
            self.cfg.twitch_cfg.client_id.clone(),
            self.stream_token.access_token.clone(),
            self.cfg.twitch_cfg.broadcaster_id.borrow().clone(),
        )
    }

    async fn prediction_router(&mut self, command: Command) {
        let first_arg = command.arguments.first().map_or("", |arg| arg);
        let Ok(pred_variant) = TryInto::<PredictionCommandVariant>::try_into(first_arg) else {
            self.chat("Possible arguments: start lock outcome cancel");
            return;
        };

        match pred_variant {
            PredictionCommandVariant::Start => self.cmd_create_prediction(command).await,

            // cancel, outcome, lock
            _ => self.send_end_prediction_signal(command, pred_variant).await,
        }
    }

    async fn cmd_create_prediction(&mut self, command: Command) {
        let Some(prediction_name) = command.get_nth_argument(1) else {
            self.chat(format!(
                "Missing argument: <prediction name>. Available predictions: {}", 
                prediction::get_defined_predictions_as_str(&self.loaded_predictions)),
            );
            return;
        };

        let Some(prediction) = prediction::find_prediction_by_name(&self.loaded_predictions, &prediction_name) else { 
            self.chat(format!(
                "Prediction {prediction_name} not found. Available predictions: {}",
                prediction::get_defined_predictions_as_str(&self.loaded_predictions)),
            );
            return;
        };

        spawn(twitch::create_prediction(
            self.http_client.clone(),
            self.get_common_twitch_parameters(), 
            self.tx_to_bot.clone(),
            command,
            prediction.clone(),
        ));
    }

    async fn send_end_prediction_signal(&mut self, command: Command, subcommand: PredictionCommandVariant) {
        use PredictionStatus as Status;

        let latest_pred = match twitch::get_latest_prediction(
                self.http_client.clone(),
                self.get_common_twitch_parameters(),
                self.tx_to_bot.clone(),
                command.clone(),
            ).await {
            Ok(latest_pred) => latest_pred,
            Err(e) => {
                println!("{e}");
                return;
            }
        };

        match latest_pred.status {
            Status::Locked => {
                if subcommand == PredictionCommandVariant::Lock {
                    self.chat("Prediction is already locked!");
                    return;
                }
            },
            Status::Canceled|Status::Resolved {..} => {
                self.chat("No active predictions! Use pred start <name> to start a prediction!");
                return;
            },
            Status::Active => (), // Irrelevant here
        }

        let desired_status = match subcommand {
            PredictionCommandVariant::Lock => Status::Locked,
            PredictionCommandVariant::Cancel => Status::Canceled,
            PredictionCommandVariant::Outcome => {
                let num_outcomes = latest_pred.outcomes.len();
                let Some(outcome_str) = command.get_nth_argument(1) else {
                    self.chat(format!("Missing argument: <outcome>. Expected a value of 1-{num_outcomes}"));
                    return;
                };

                let outcome_int = match outcome_str.trim().parse::<usize>() {
                    Ok(int) if 0 < int && int <= num_outcomes => int,
                    _ => {
                        self.chat(format!("Unexpected value for argument <outcome>. Expected a value of 1-{num_outcomes}"));
                        return;
                    },
                };

                let winning_id = latest_pred.outcomes[outcome_int-1].id.clone();
                Status::Resolved { winning_outcome_id: Some(winning_id) }
            },
            _ => panic!("shouldn't fucking happen"),
        };
        
        let common_paras = self.get_common_twitch_parameters();
        let data = EndPredictionData {
            broadcaster_id: common_paras.broadcaster_id.clone(),
            id: latest_pred.id,
            status: desired_status.clone().into(),
            winning_outcome_id: match desired_status {
                PredictionStatus::Resolved { winning_outcome_id } => winning_outcome_id.clone(),
                _ => None,
            },
        };
        
        // TODO: get result of function in the spawn function, then call tx_to_bot
        let http_client_c = self.http_client.clone();
        let tx_to_bot_c = self.tx_to_bot.clone();
        spawn(async move {
            let res = twitch::end_prediction(
                http_client_c,
                common_paras,
                tx_to_bot_c,
                command,
                data,
            ).await;

            match res {
                Ok(_) => println!("ended prediction succesfully"),
                Err(e) => (),
            }

        });

    }
}
