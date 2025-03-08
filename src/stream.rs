use std::{
    fmt::Display, io::{Read, Write}, net::TcpStream, time::Duration
};

use anyhow::{bail, Result};

use crate::message::{self, User};
use message::Message;

// Stream is interpreted as an IRC stream
pub struct Stream {
    pub current_stream: TcpStream,
    channel: String,
}

impl Stream {
    pub async fn new(host: &str, port: &u16, channel: String) -> Result<Self> {
        Ok(Stream {
            current_stream: TcpStream::connect(format!("{}:{}", host, port))?,
            channel,
        })
    }

    pub fn connect_to_irc(&mut self, account: &str, channel: &str, access_token: &str) -> Result<()> {
        writeln!(self.current_stream, "PASS oauth:{access_token}")?;
        writeln!(self.current_stream, "NICK {account}")?;
        writeln!(self.current_stream, "JOIN #{channel}")?;
        writeln!(self.current_stream, "CAP REQ :twitch.tv/commands twitch.tv/tags")?;

        self.current_stream.set_read_timeout(Some(Duration::from_millis(10)))?;
        Ok(())
    }

    pub fn read_irc(&mut self) -> Result<Vec<Message>> {
        let mut irc_batch = String::new();
        self.current_stream.read_to_string(&mut irc_batch)?;
        
        Ok(self.handle_irc_messages(&irc_batch))
    }

    fn handle_irc_messages(&mut self, irc_batch: &String) -> Vec<Message> {
        // every message is separated by \r\n
        let mut messages: Vec<Message> = Vec::new();
        let raw_messages: Vec<&str> = irc_batch.split("\r\n").collect();

        for raw_message in raw_messages {
            let split_message: Vec<&str> = raw_message.split(' ').collect();
            let irc_message_type: IrcMessageType = (&split_message).into();
            match irc_message_type {
                IrcMessageType::PING => self.pong(split_message.get(1).map(|m| *m)),
                IrcMessageType::PRIVMSG => messages.push(parse_privmsg_to_message(raw_message)),
                IrcMessageType::OTHER => println!("{raw_message}"),
                _ => continue,
            };
        } 

        messages
    }

    pub fn send_chat_message<T: Display>(&mut self, message: T) -> Result<()> {
        if let Err(e) = writeln!(self.current_stream, "PRIVMSG #{} :{}", self.channel, message) {
            bail!("Failed to send chat message, reason: {e}")
        }

        Ok(())
    }

    fn pong(&mut self, answer_opt: Option<&str>) {
        println!("PINGED");
        if let Some(answer) = answer_opt {
            if let Err(e) = writeln!(self.current_stream, "PONG {answer}") {
                println!("WARNING: Couldn't ping back. Reason: {e}");
            }
        }
    }
}

fn invalid_message(split_message: &Vec<&str>) -> bool {
    split_message.len() < 2
}

fn is_message_ping(split_message: &Vec<&str>) -> bool {
    match split_message.first() {
        Some(first) if *first == "PING" => return true,
        _ => return false,
    }
}

fn is_message_privmsg(split_message: &Vec<&str>) -> bool {
    match split_message.get(2) {
        Some(m) if *m == "PRIVMSG" => return true,
        _ => return false,
    }
}

fn is_message_userstate(split_message: &Vec<&str>) -> bool {
    match split_message.get(2) {
        Some(m) if *m == "USERSTATE" => return true,
        _ => return false,
    }
}

fn parse_privmsg_to_message(raw_message: &str) -> Message {
    let username: String = raw_message
        .chars()
        .skip_while(|c| *c != ':')
        .skip(1)
        .take_while(|c| *c != '!')
        .collect();

    let mut user_message: String = raw_message
        .chars()
        .skip_while(|c| *c != ':')
        .skip(1)
        .skip_while(|c| *c != ':')
        .skip(1)
        .collect();

    // remove random whitespace
    user_message = user_message
        .replace(" \u{e0000}", "")
        .trim_end()
        .to_string();

    let metadata: String = raw_message
        .chars()
        .take_while(|c| *c != ' ')
        .collect();

    Message::new(
        User::from(metadata, username),
        user_message,
    )
}


#[derive(Clone, PartialEq, Debug)]
enum IrcMessageType {
    PING,
    PRIVMSG,
    EMPTY,
    USERSTATE,
    OTHER,
}

impl From<&Vec<&str>> for IrcMessageType {
    fn from(split_message: &Vec<&str>) -> Self {
        if invalid_message(split_message) {
            return IrcMessageType::EMPTY;
        }
        if is_message_ping(split_message) {
            return IrcMessageType::PING;
        }
        if is_message_privmsg(split_message) {
            return IrcMessageType::PRIVMSG;
        }
        if is_message_userstate(split_message) {
            return IrcMessageType::USERSTATE;
        }
        IrcMessageType::OTHER
    }
}