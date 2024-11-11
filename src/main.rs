mod command_parser;
mod data;
mod error;
mod parser;

use crate::command_parser::{parse_command, Command};
use crate::data::Data;
use crate::error::RusdisError;
use crate::parser::{parse, ParserError, Value};
use lazy_static::lazy_static;
use regex::Regex;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::tcp::WriteHalf;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;
use tokio::task;

lazy_static! {
    //static ref START_WITH_SPECIAL: Regex = Regex::new(r#"^([\+-:$\*_#,=\(!%`>~])"#).unwrap();
    //static ref ARRAY_STRUCT: Regex = Regex::new(r#"^*"#).unwrap();
    //static ref BULK_STRING_STRUCT: Regex = Regex::new(r#"^$"#).unwrap();
    static ref DATABASE: Arc<Mutex<HashMap<String, Data>>> = Arc::new(Mutex::new(HashMap::new()));
}

#[tokio::main]
async fn main() {
    // You can use print statements as follows for debugging, they'll be visible when running tests.
    println!("Logs from your program will appear here!");

    // Uncomment this block to pass the first stage
    //
    let listener = TcpListener::bind("127.0.0.1:6379").await.unwrap();
    //

    loop {
        let res = listener.accept().await;

        match res {
            Ok((stream, _)) => {
                println!("accepted new connection");
                task::spawn(async move {
                    handle_commands(stream).await;
                });
            }
            Err(e) => {
                println!("error: {}", e);
            }
        }
    }
}

async fn handle_commands(mut stream: TcpStream) -> Result<(), RusdisError> {
    let (reader, mut writer) = stream.split();
    let mut reader = BufReader::new(reader);

    loop {
        let mut buf = Vec::from(reader.fill_buf().await?);
        if buf.len() == 0 {
            break;
        }
        reader.consume(buf.len());
        let commands = String::from_utf8_lossy(&buf).to_string();
        println!("{}", commands);
        let value = parse(commands)?;

        match value {
            Value::Array(cmds) => {
                execute_commands(cmds, &mut writer).await?;
            }
            _ => {}
        }
    }
    Ok(())
}

async fn execute_commands(
    command: Vec<Value>,
    writer: &mut WriteHalf<'_>,
) -> Result<(), RusdisError> {
    dbg!(&command);
    let command = parse_command(command)?;

    match command {
        Command::Ping => {
            writer.write_all(b"+PONG\r\n").await?;
        }
        Command::Echo(words) => {
            writer
                .write_all(format!("+{}\r\n", words).as_bytes())
                .await?;
        }
        Command::ConfigGet(option) => {
            todo!();
        }
        Command::Set { key, value, px } => {
            // Todo: implement "active" or "passive" way to delete data
            let mut expiration = None;

            if let Some(mills) = px {
                let now = Instant::now();
                let fu = now.checked_add(Duration::from_millis(mills as u64));
                if fu.is_none() {
                    return Err(RusdisError::InstantAdditionError);
                }

                expiration = fu;
            }

            let mut data_handle = DATABASE.lock().await;
            let _ = data_handle.insert(key, Data::new(value, expiration));
            writer.write_all(b"+OK\r\n").await?;
        }
        Command::Get(key) => {
            let mut data_handle = DATABASE.lock().await;
            match data_handle.get(&key) {
                Some(data) => {
                    if data.is_expired() {
                        let _ = data_handle.remove(&key);
                        writer.write_all(b"$-1\r\n").await?;
                    } else {
                        writer
                            .write_all(
                                format!("${}\r\n{}\r\n", data.get_data().len(), data.get_data())
                                    .as_bytes(),
                            )
                            .await?;
                    }
                }
                None => {
                    writer.write_all(b"$-1\r\n").await?;
                }
            }
        }
    }

    //match &command[0] {
    //    Value::BulkString(cmd) => {
    //        let cmd = cmd.to_uppercase();
    //        match cmd.as_str() {
    //            "ECHO" => {
    //                if command.len() < 2 {
    //                    return Err(RusdisError::InvalidCommand);
    //                }
    //
    //                if let Value::BulkString(word) = &command[1] {
    //                    writer
    //                        .write_all(format!("+{}\r\n", word).as_bytes())
    //                        .await?;
    //                } else {
    //                    return Err(RusdisError::InvalidCommand);
    //                }
    //            }
    //            "PING" => {
    //                writer.write_all(b"+PONG\r\n").await?;
    //            }
    //            "SET" => {
    //                if command.len() < 3 {
    //                    return Err(RusdisError::InvalidCommand);
    //                }
    //
    //                let (key, value) = (&command[1], &command[2]);
    //
    //                match (key, value) {
    //                    (Value::BulkString(k), Value::BulkString(v)) => {
    //                        let mut data_handle = DATABASE.lock().await;
    //                        let _ = data_handle.insert(k.to_string(), v.to_string());
    //                        writer.write_all(b"+OK\r\n").await?;
    //                    }
    //                    _ => return Err(RusdisError::InvalidCommand),
    //                }
    //            }
    //            "GET" => {
    //                if command.len() < 2 {
    //                    return Err(RusdisError::InvalidCommand);
    //                }
    //
    //                if let Value::BulkString(key) = &command[1] {
    //                    let data_handle = DATABASE.lock().await;
    //                    match data_handle.get(key) {
    //                        Some(val) => {
    //                            writer
    //                                .write_all(format!("${}\r\n{}\r\n", val.len(), val).as_bytes())
    //                                .await?;
    //                        }
    //                        None => {
    //                            writer.write_all(b"$-1\r\n").await?;
    //                        }
    //                    }
    //                } else {
    //                    return Err(RusdisError::InvalidCommand);
    //                }
    //            }
    //            _ => {}
    //        }
    //    }
    //    _ => {}
    //}
    Ok(())
}
