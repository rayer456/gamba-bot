use chrono::Utc;
use date_component::date_component;
use reqwest::header::AUTHORIZATION;
use serde::{de, Deserialize};
use std::fmt::{self, Display};

use anyhow::{bail, Result};
use serde_yaml::Value;
use std::time::{Duration, SystemTime};

use crate::message::{Group, Message, RecentUser, User};

const CLIPS_URL: &'static str = "https://api.twitch.tv/helix/clips";
const FOLLOWERS_URL: &'static str = "https://api.twitch.tv/helix/channels/followers";

#[derive(Debug)]
pub enum AppError {
    InvalidTokenError {
        cmd: String,
        arguments: Vec<String>,
        requested_by: Option<User>,
    },
    OtherError(String),
}

impl Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AppError::InvalidTokenError { .. } => write!(f, "invalid token"),
            AppError::OtherError(e) => write!(f, "other error: {e}"),
        }
    }
}

impl From<String> for AppError {
    fn from(cmd: String) -> Self {
        AppError::InvalidTokenError {
            cmd,
            arguments: vec![],
            requested_by: None,
        }
    }
}

impl From<reqwest::Error> for AppError {
    fn from(value: reqwest::Error) -> Self {
        AppError::OtherError(value.to_string())
    }
}

impl From<serde_json::Error> for AppError {
    fn from(value: serde_json::Error) -> Self {
        AppError::OtherError(value.to_string())
    }
}

#[derive(Clone, PartialEq, Deserialize, Debug)]
pub struct Command {
    pub cmd: String, // Mandatory

    // Disallow these properties from being defined in commands.yaml
    #[serde(skip)]
    pub recent_users: Vec<RecentUser>,
    #[serde(skip)]
    pub arguments: Vec<String>,
    #[serde(skip)]
    pub requested_by: Option<User>,
    #[serde(skip, default = "SystemTime::now")]
    pub can_be_used_at: SystemTime,

    #[serde(default, deserialize_with = "Command::build_alt_cmds")]
    pub alternative_cmds: Vec<String>,

    #[serde(default, deserialize_with = "Command::build_response")]
    pub response: Option<String>,

    #[serde(default, deserialize_with = "Command::cast_cooldown")]
    pub global_cooldown: Duration,

    #[serde(default, deserialize_with = "Command::cast_cooldown")]
    pub user_cooldown: Duration,

    #[serde(default = "Command::get_default_permitted_by")]
    #[serde(deserialize_with = "Command::build_permitted_by")]
    pub permitted_by: Vec<Group>,

    #[serde(default, deserialize_with = "Command::build_allowed_bypass")]
    pub allowed_to_bypass: Vec<Group>,
}

impl Command {
    pub fn is_global_cooldown_active(&self) -> bool {
        SystemTime::now() < self.can_be_used_at
    }

    pub fn add_global_cooldown(&mut self) {
        self.can_be_used_at = SystemTime::now() + self.global_cooldown;
    }

    pub fn is_user_permitted(&self, user: &User) -> bool {
        user.groups
            .iter()
            .any(|user_group| self.permitted_by.contains(user_group))
    }

    pub fn can_user_bypass(&self, user: &User) -> bool {
        user.groups
            .iter()
            .any(|user_group| self.allowed_to_bypass.contains(user_group))
    }

    pub fn is_user_cooldown_active(&mut self, user: &User) -> bool {
        for recent_user in &mut self.recent_users {
            if recent_user.user.username != user.username {
                continue;
            }

            if recent_user.is_cooldown_active() {
                println!(
                    "existing user {} can't use the command {} again",
                    user.username, self.cmd
                );
                return true;
            } else {
                println!("existing user {} can use command again", user.username);
                recent_user.add_cooldown(self.user_cooldown);
                return false;
            }
        }

        let mut recent_user = RecentUser::new(user.clone());
        recent_user.add_cooldown(self.user_cooldown);
        self.recent_users.push(recent_user);
        println!("new user {} created", user.username);

        false
    }

    fn cast_cooldown<'de, D>(input: D) -> Result<Duration, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        let n = u64::deserialize(input)?;
        Ok(Duration::from_secs(n))
    }

    fn build_response<'de, D>(input: D) -> Result<Option<String>, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        let r = String::deserialize(input)?;
        Ok(Some(r))
    }

    fn build_alt_cmds<'de, D>(input: D) -> Result<Vec<String>, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        let alt_cmds = Vec::<String>::deserialize(input)?;
        Ok(alt_cmds)
    }

    fn get_default_permitted_by() -> Vec<Group> {
        vec![Group::EVERYONE]
    }

    fn build_permitted_by<'de, D>(input: D) -> Result<Vec<Group>, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        let permitted_by_de = Vec::<String>::deserialize(input)?;

        let mut permitted_by: Vec<Group> = permitted_by_de.iter().map(|group| group.as_str().into()).collect();

        if permitted_by.is_empty() {
            permitted_by.push(Group::EVERYONE);
        }

        Ok(permitted_by)
    }

    fn build_allowed_bypass<'de, D>(input: D) -> Result<Vec<Group>, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        let allowed_to_bypass_de = Vec::<String>::deserialize(input)?;

        let allowed_to_bypass = allowed_to_bypass_de.iter().map(|group| group.as_str().into()).collect();

        Ok(allowed_to_bypass)
    }
}

pub async fn get_commands() -> Result<Vec<Command>> {
    let commands_yaml = match std::fs::read_to_string("commands.yaml") {
        Ok(cmds) => cmds,
        Err(e) => bail!("{e}:\nFile commands.yaml not found, can't continue."),
    };

    let yaml_commands: Vec<Command> = match serde_yaml::from_str(&commands_yaml) {
        Ok(cmds) => cmds,
        Err(e) => bail!("Syntax of defined commands in commands.yaml is wrong.\nIn specific: {e}"),
    };

    Ok(yaml_commands)
}

pub fn create_clip(broadcaster_id: String, access_token: String, client_id: String) -> Result<String, AppError> {
    let params = [("broadcaster_id", broadcaster_id)];
    let client = reqwest::blocking::Client::new();
    let response = client
        .post(CLIPS_URL)
        .header(AUTHORIZATION, format!("Bearer {}", access_token))
        .header("Client-Id", &client_id)
        .form(&params)
        .send();

    let res = response?;
    match res.status().as_u16() {
        202 => {
            let data: Value = serde_json::from_str(&res.text()?)?;
            match data["data"][0]["id"].as_str() {
                Some(id) => id.to_string(),
                None => return Err(AppError::OtherError(format!("clip_id not found"))),
            };
            let edit_url = match data["data"][0]["edit_url"].as_str() {
                Some(url) => url.to_string(),
                None => return Err(AppError::OtherError(format!("edit_url not found"))),
            };

            return Ok(edit_url.strip_suffix("/edit").unwrap().to_string());
        }
        401 => {
            return Err(AppError::InvalidTokenError {
                cmd: "!clip".to_string(),
                arguments: vec![],
                requested_by: None,
            })
        }
        other => return Err(AppError::OtherError(format!("{}: {}", other, res.text()?))),
    };
}

pub fn get_followage(
    user_id: String,
    broadcaster_id: String,
    access_token: String,
    client_id: String,
    user: User,
) -> Result<String, AppError> {
    let params = [("broadcaster_id", broadcaster_id), ("user_id", user_id)];
    let client = reqwest::blocking::Client::new();
    let response = client
        .get(FOLLOWERS_URL)
        .header(AUTHORIZATION, format!("Bearer {}", access_token))
        .header("Client-Id", &client_id)
        .query(&params)
        .send()?;

    match response.status().as_u16() {
        200 => {
            let data: Value = serde_json::from_str(&response.text()?)?;
            match data["data"][0]["followed_at"].as_str() {
                Some(date) => {
                    match chrono::DateTime::parse_from_str(format!("{date} +0000").as_str(), "%Y-%m-%dT%H:%M:%SZ %z") {
                        Ok(datetime) => {
                            let date_comp = date_component::calculate(&datetime.to_utc(), &Utc::now());
                            let mut result_string = format!("{} has been following for ", user.username);

                            // if following for more than 1 day total
                            match date_comp.year {
                                0 => (),
                                1 => result_string += format!("{} year ", date_comp.year).as_str(),
                                _ => result_string += format!("{} years ", date_comp.year).as_str(),
                            }
                            match date_comp.month {
                                0 => (),
                                1 => result_string += format!("{} month ", date_comp.month).as_str(),
                                _ => result_string += format!("{} months ", date_comp.month).as_str(),
                            }
                            match date_comp.day {
                                0 => (),
                                1 => result_string += format!("{} day ", date_comp.day).as_str(),
                                _ => result_string += format!("{} days ", date_comp.day).as_str(),
                            }

                            // if following for less than 1 day total
                            if date_comp.interval_days == 0 {
                                match date_comp.hour {
                                    0 => (),
                                    1 => result_string += format!("{} hour ", date_comp.hour).as_str(),
                                    _ => result_string += format!("{} hours ", date_comp.hour).as_str(),
                                }
                                match date_comp.minute {
                                    0 => (),
                                    1 => result_string += format!("{} minute ", date_comp.minute).as_str(),
                                    _ => result_string += format!("{} minutes ", date_comp.minute).as_str(),
                                }
                                match date_comp.second {
                                    0 => (),
                                    1 => result_string += format!("{} second ", date_comp.second).as_str(),
                                    _ => result_string += format!("{} seconds ", date_comp.second).as_str(),
                                }
                            }

                            return Ok(result_string);
                        }
                        Err(e) => return Err(AppError::OtherError(format!("Couldn't convert date: {e}"))),
                    }
                }
                None => return Ok(format!("{} isn't following", user.username)),
            };
        }
        401 => {
            return Err(AppError::InvalidTokenError {
                cmd: "!followage".to_string(),
                arguments: vec![],
                requested_by: Some(user),
            })
        }
        other => return Err(AppError::OtherError(format!("{}: {}", other, response.text()?))),
    };
}

pub fn validate_and_return_command<'a>(
    option: Option<&'a mut Command>,
    message: &mut Message,
) -> Option<&'a mut Command> {
    // make checks to see if the command is allowed to be used

    let cmd = match option {
        Some(c) => c,
        None => return None,
    };

    if !cmd.is_user_permitted(&message.user) {
        return None;
    }
    if cmd.can_user_bypass(&message.user) {
        if !cmd.is_global_cooldown_active() {
            cmd.add_global_cooldown();
        }
        return Some(cmd);
    }
    if cmd.is_global_cooldown_active() {
        return None;
    }
    if cmd.is_user_cooldown_active(&message.user) {
        return None;
    }

    cmd.add_global_cooldown();

    Some(cmd)
}
