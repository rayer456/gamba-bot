use std::time::{Duration, SystemTime};

use serde::Deserialize;

#[derive(Clone, PartialEq, Deserialize, Debug)]
pub enum Group {
    STREAMER,
    MODERATOR,
    VIP,
    EVERYONE,
}

impl From<&str> for Group {
    fn from(group: &str) -> Self {
        match group.to_uppercase().as_str() {
            "STREAMER" => Group::STREAMER,
            "MODERATOR" => Group::MODERATOR,
            "VIP" => Group::VIP,
            "EVERYONE" => Group::EVERYONE,
            _ => panic!("No matching group was found for the Group {group}"),
        }
    }
}

#[derive(Clone, PartialEq, Debug)]
pub struct Message {
    pub user: User,
    pub message: String,
}

impl Message {
    pub fn new(user: User, message: String) -> Self {
        Message { user, message }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct User {
    pub username: String,
    pub groups: Vec<Group>,
}

impl PartialEq for User {
    fn eq(&self, other: &Self) -> bool {
        self.username == other.username
    }
}

impl User {
    pub fn new(username: String, mut groups: Vec<Group>) -> Self {
        groups.push(Group::EVERYONE); // the default group is EVERYONE
        User { username, groups }
    }

    pub fn from(metadata: String, username: String) -> Self {
        let mut groups = vec![];

        // add any groups depending on the metadata
        if metadata.contains("broadcaster/1") {
            groups.push(Group::STREAMER);
        } else if metadata.contains("mod=1") {
            groups.push(Group::MODERATOR);
        } else if metadata.contains("vip=1") {
            groups.push(Group::VIP);
        }

        Self::new(username, groups)
    }
}

#[derive(Clone, PartialEq, Deserialize, Debug)]
pub struct RecentUser {
    pub user: User,
    pub can_use_at: SystemTime,
}

impl RecentUser {
    pub fn new(user: User) -> Self {
        RecentUser {
            user,
            can_use_at: SystemTime::now(),
        }
    }

    pub fn is_cooldown_active(&self) -> bool {
        SystemTime::now() < self.can_use_at
    }

    pub fn add_cooldown(&mut self, cooldown: Duration) {
        self.can_use_at = SystemTime::now() + cooldown;
    }
}
