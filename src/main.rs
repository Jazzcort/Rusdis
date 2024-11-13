mod cli_parser;
mod command_parser;
mod data;
mod error;
mod parser;
mod rdb_file_reader;

use crate::cli_parser::Args;
use crate::command_parser::{parse_command, Command};
use crate::data::Data;
use crate::error::RusdisError;
use crate::parser::{parse, ParserError, Value};
use crate::rdb_file_reader::read_rdb;
use clap::Parser;
use command_parser::{ConfigGetOption, ConfigSubcommand};
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
        (Some(dir), Some(dbfilename)) => read_rdb(dir + "/" + &dbfilename)?,
        _ => {}
    }

    //let mut dir_handle = DIR.lock().await;
    //*dir_handle = args.dir.clone();
    //drop(dir_handle);
    //
    //let mut dbfilename_handle = DBFILENAME.lock().await;
    //*dbfilename_handle = args.dbfilename.clone();
    //drop(dbfilename_handle);

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

    Ok(())
}
