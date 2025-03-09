use reqwest::header::AUTHORIZATION;
use serde::{de, Deserialize};
use std::fmt::{self, Display};

use anyhow::{bail, Result};
use serde_yaml::Value;
use std::time::{Duration, SystemTime};

use crate::message::{Group, Message, RecentUser, User};

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

