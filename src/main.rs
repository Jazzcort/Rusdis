mod cli_parser;
mod command_parser;
mod data;
mod error;
mod parser;
mod rdb_file_reader;

use crate::cli_parser::Args;
use crate::command_parser::{parse_command, Command};
use crate::data::{Admin, Database, StringData};
use crate::error::RusdisError;
use crate::parser::{parse, ParserError, Value};
use crate::rdb_file_reader::read_rdb;
use clap::Parser;
use command_parser::{ConfigGetOption, ConfigSubcommand};
use lazy_static::lazy_static;
use regex::Regex;
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::tcp::WriteHalf;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;
use tokio::task;

lazy_static! {
    //static ref START_WITH_SPECIAL: Regex = Regex::new(r#"^([\+-:$\*_#,=\(!%`>~])"#).unwrap();
    //static ref ARRAY_STRUCT: Regex = Regex::new(r#"^*"#).unwrap();
    //static ref BULK_STRING_STRUCT: Regex = Regex::new(r#"^$"#).unwrap();
    static ref ADMIN: Arc<Mutex<Admin>> = Arc::new(Mutex::new(Admin::new(vec![])));
    static ref DIR: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    static ref DBFILENAME: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
}

#[tokio::main]
async fn main() -> Result<(), RusdisError> {
    let args = Args::parse();
    tokio::join!(
        async {
            let mut dir_handle = DIR.lock().await;
            *dir_handle = args.dir.clone();
        },
        async {
            let mut dbfilename_handle = DBFILENAME.lock().await;
            *dbfilename_handle = args.dbfilename.clone();
        }
    );

    let (dir_option, dbfilename_option) = tokio::join!(
        async {
            let dir_handle = DIR.lock().await;
            dir_handle.clone()
        },
        async {
            let dbfilename_handle = DBFILENAME.lock().await;
            dbfilename_handle.clone()
        },
    );

    match (dir_option, dbfilename_option) {
        (Some(dir), Some(dbfilename)) => {
            let res = read_rdb(dir + "/" + &dbfilename);
            match res {
                Ok(rdb_file) => {
                    dbg!(&rdb_file.datasets);
                    let new_admin = Admin::new(rdb_file.datasets);

                    let mut admin_handle = ADMIN.lock().await;
                    *admin_handle = new_admin;
                }
                Err(e) => {
                    dbg!(e);
                }
            }
        }
        _ => {}
    }

    let admin_handle = ADMIN.lock().await;
    let string_data_arc = admin_handle.get_string_data_map();
    drop(admin_handle);
    dbg!(string_data_arc);

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
    let mut is_multi = false;
    let mut queue = vec![];

    loop {
        let mut buf = Vec::from(reader.fill_buf().await?);
        if buf.len() == 0 {
            break;
        }
        reader.consume(buf.len());
        let commands = String::from_utf8_lossy(&buf).to_string();
        println!("{}", commands);
        // wrong protocol: need to disconnect
        let value = parse(commands)?;

        match value {
            Value::Array(cmds) => {
                let cmd = parse_command(cmds);
                if cmd.is_err() {
                    continue;
                }

                let cmd = cmd.unwrap();

                match cmd {
                    Command::Multi => {
                        is_multi = true;
                        writer.write_all(b"+OK\r\n").await;
                    }
                    Command::Exec => {
                        if !is_multi {
                            writer.write_all(b"-ERR EXEC without MULTI\r\n").await;
                            continue;
                        }

                        let reply_string = execute_multi_commands(queue).await;
                        queue = vec![];
                        is_multi = false;
                        writer.write_all(reply_string.as_bytes()).await;
                    }
                    Command::Discard => {
                        if !is_multi {
                            writer.write_all(b"-ERR DISCARD without MULTI\r\n").await;
                            continue;
                        }

                        queue.clear();
                        is_multi = false;
                        writer.write_all(b"+OK\r\n").await;
                    }
                    other => {
                        if !is_multi {
                            execute_commands(other, &mut writer).await;
                        } else {
                            queue.push(other);
                            writer.write_all(b"+QUEUED\r\n").await;
                        }
                    }
                }
            }
            _ => {}
        }
    }
    Ok(())
}

async fn execute_multi_commands(commands: Vec<Command>) -> String {
    let length = commands.len();
    let mut res = format!("*{}\r\n", length);

    for cmd in commands.into_iter() {
        match cmd {
            Command::Ping => {
                res += "+PONG\r\n";
            }
            Command::Echo(words) => {
                res += format!("+{}\r\n", words).as_str();
            }
            Command::Config(subcommand) => match subcommand {
                ConfigSubcommand::Get(option) => match option {
                    ConfigGetOption::Dir => {
                        let dir_handle = DIR.lock().await;
                        let dir_ref = dir_handle.as_ref();
                        match dir_ref {
                            Some(dir) => {
                                res += format!("*2\r\n$3\r\ndir\r\n${}\r\n{}\r\n", dir.len(), dir)
                                    .as_str();
                            }
                            None => {
                                res += "*2\r\n$3\r\ndir\r\n$-1\r\n";
                            }
                        }
                    }
                    ConfigGetOption::DbFilename => {
                        let dbfilename_handle = DBFILENAME.lock().await;
                        let dbfilename_ref = dbfilename_handle.as_ref();
                        match dbfilename_ref {
                            Some(dbfilename) => {
                                res += format!(
                                    "*2\r\n$10\r\ndbfilename\r\n${}\r\n{}\r\n",
                                    dbfilename.len(),
                                    dbfilename
                                )
                                .as_str();
                            }
                            None => {
                                res += "*2\r\n$10\r\ndbfilename\r\n$-1\r\n";
                            }
                        }
                    }
                },
            },
            Command::Set { key, value, px } => {
                // Todo: implement "active" or "passive" way to delete data
                let mut expiration = None;

                if let Some(mills) = px {
                    let now = SystemTime::now();
                    let fu = now.checked_add(Duration::from_millis(mills as u64));
                    if fu.is_none() {
                        res += "-ERR Instant Addtion Error";
                        continue;
                    }

                    expiration = fu;
                }

                let admin_handle = ADMIN.lock().await;
                let string_data_arc = admin_handle.get_string_data_map();
                drop(admin_handle);
                let mut string_data_handle = string_data_arc.lock().await;
                let _ = string_data_handle.insert(key, StringData::new(value, expiration));
                res += "+OK\r\n";
            }
            Command::Get(key) => {
                let admin_handle = ADMIN.lock().await;
                let string_data_arc = admin_handle.get_string_data_map();
                drop(admin_handle);
                let mut string_data_handle = string_data_arc.lock().await;

                match string_data_handle.get(&key) {
                    Some(data) => {
                        if data.is_expired() {
                            let _ = string_data_handle.remove(&key);
                            res += "$-1\r\n";
                        } else {
                            res += format!("${}\r\n{}\r\n", data.get_data().len(), data.get_data())
                                .as_str();
                        }
                    }
                    None => {
                        res += "$-1\r\n";
                    }
                }
            }
            Command::Keys(pattern_string) => {
                let pattern = Regex::new(&pattern_string);
                if pattern.is_err() {
                    res += "-ERR Invalid Regex Format";
                    continue;
                }
                let pattern = pattern.unwrap();

                let admin_handle = ADMIN.lock().await;
                let string_data_arc = admin_handle.get_string_data_map();
                drop(admin_handle);
                let string_data_handle = string_data_arc.lock().await;
                let mut res_vec = vec![];

                for key in string_data_handle.keys() {
                    if pattern.is_match(key) {
                        res_vec.push(key);
                    }
                }

                let mut reply_string = format!("*{}\r\n", res.len());
                for matched_key in res_vec.into_iter() {
                    reply_string +=
                        format!("${}\r\n{}\r\n", matched_key.len(), matched_key).as_str();
                }

                res += reply_string.as_str();
            }
            Command::Incr(key) => {
                let admin_handle = ADMIN.lock().await;
                let string_data_arc = admin_handle.get_string_data_map();
                drop(admin_handle);

                let mut string_data_handle = string_data_arc.lock().await;

                let data = string_data_handle
                    .entry(key)
                    .or_insert(StringData::new("0".to_string(), None));
                let num_str = data.get_data();
                match num_str.parse::<i64>() {
                    Ok(mut num) => {
                        if num < i64::MAX {
                            num += 1;
                        }

                        data.set_data(format!("{}", num));
                        res += format!(":{}\r\n", num).as_str();
                    }
                    Err(_) => {
                        res += "-ERR value is not an integer or out of range\r\n";
                    }
                }
            }
            _ => {
                res += "-ERR not supported command";
            }
        }
    }

    res
}

async fn execute_commands(command: Command, writer: &mut WriteHalf<'_>) -> Result<(), RusdisError> {
    dbg!(&command);

    match command {
        Command::Ping => {
            writer.write_all(b"+PONG\r\n").await?;
        }
        Command::Echo(words) => {
            writer
                .write_all(format!("+{}\r\n", words).as_bytes())
                .await?;
        }
        Command::Config(subcommand) => match subcommand {
            ConfigSubcommand::Get(option) => match option {
                ConfigGetOption::Dir => {
                    let dir_handle = DIR.lock().await;
                    let dir_ref = dir_handle.as_ref();
                    match dir_ref {
                        Some(dir) => {
                            writer
                                .write_all(
                                    format!("*2\r\n$3\r\ndir\r\n${}\r\n{}\r\n", dir.len(), dir)
                                        .as_bytes(),
                                )
                                .await?;
                        }
                        None => {
                            writer.write_all(b"*2\r\n$3\r\ndir\r\n$-1\r\n").await?;
                        }
                    }
                }
                ConfigGetOption::DbFilename => {
                    let dbfilename_handle = DBFILENAME.lock().await;
                    let dbfilename_ref = dbfilename_handle.as_ref();
                    match dbfilename_ref {
                        Some(dbfilename) => {
                            writer
                                .write_all(
                                    format!(
                                        "*2\r\n$10\r\ndbfilename\r\n${}\r\n{}\r\n",
                                        dbfilename.len(),
                                        dbfilename
                                    )
                                    .as_bytes(),
                                )
                                .await?;
                        }
                        None => {
                            writer
                                .write_all(b"*2\r\n$10\r\ndbfilename\r\n$-1\r\n")
                                .await?;
                        }
                    }
                }
            },
        },
        Command::Set { key, value, px } => {
            // Todo: implement "active" or "passive" way to delete data
            let mut expiration = None;

            if let Some(mills) = px {
                let now = SystemTime::now();
                let fu = now.checked_add(Duration::from_millis(mills as u64));
                if fu.is_none() {
                    return Err(RusdisError::InstantAdditionError);
                }

                expiration = fu;
            }

            let admin_handle = ADMIN.lock().await;
            let string_data_arc = admin_handle.get_string_data_map();
            drop(admin_handle);
            let mut string_data_handle = string_data_arc.lock().await;
            let _ = string_data_handle.insert(key, StringData::new(value, expiration));
            writer.write_all(b"+OK\r\n").await?;
        }
        Command::Get(key) => {
            let admin_handle = ADMIN.lock().await;
            let string_data_arc = admin_handle.get_string_data_map();
            drop(admin_handle);
            let mut string_data_handle = string_data_arc.lock().await;

            match string_data_handle.get(&key) {
                Some(data) => {
                    if data.is_expired() {
                        let _ = string_data_handle.remove(&key);
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
        Command::Keys(pattern_string) => {
            let pattern = Regex::new(&pattern_string)?;
            let admin_handle = ADMIN.lock().await;
            let string_data_arc = admin_handle.get_string_data_map();
            drop(admin_handle);
            let string_data_handle = string_data_arc.lock().await;
            let mut res = vec![];

            for key in string_data_handle.keys() {
                if pattern.is_match(key) {
                    res.push(key);
                }
            }

            let mut reply_string = format!("*{}\r\n", res.len());
            for matched_key in res.into_iter() {
                reply_string += format!("${}\r\n{}\r\n", matched_key.len(), matched_key).as_str();
            }

            writer.write_all(reply_string.as_bytes()).await?;
        }
        Command::Incr(key) => {
            let admin_handle = ADMIN.lock().await;
            let string_data_arc = admin_handle.get_string_data_map();
            drop(admin_handle);

            let mut string_data_handle = string_data_arc.lock().await;
            //let a = string_data_handle.get_mut(&key);

            let data = string_data_handle
                .entry(key)
                .or_insert(StringData::new("0".to_string(), None));
            let num_str = data.get_data();
            match num_str.parse::<i64>() {
                Ok(mut num) => {
                    if num < i64::MAX {
                        num += 1;
                    }

                    data.set_data(format!("{}", num));
                    writer.write_all(format!(":{}\r\n", num).as_bytes()).await?;
                }
                Err(_) => {
                    writer
                        .write_all(b"-ERR value is not an integer or out of range\r\n")
                        .await?;
                }
            }
        }
        _ => {}
    }

    Ok(())
}
