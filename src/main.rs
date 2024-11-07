mod error;
mod parser;

use crate::error::RusdisError;
use crate::parser::{parse, ParserError, Value};
use lazy_static::lazy_static;
use regex::Regex;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::tcp::WriteHalf;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;
use tokio::task;

lazy_static! {
    //static ref START_WITH_SPECIAL: Regex = Regex::new(r#"^([\+-:$\*_#,=\(!%`>~])"#).unwrap();
    //static ref ARRAY_STRUCT: Regex = Regex::new(r#"^*"#).unwrap();
    //static ref BULK_STRING_STRUCT: Regex = Regex::new(r#"^$"#).unwrap();
    static ref DATABASE: Arc<Mutex<HashMap<String, String>>> = Arc::new(Mutex::new(HashMap::new()));
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
    //for res in listener.accept().await {
    //    match res {
    //        Ok((mut stream, _)) => {
    //            println!("accepted new connection");
    //
    //            let mut buf = [0; 512];
    //            loop {
    //                let read_count = stream.read(&mut buf).unwrap();
    //                if read_count == 0 {
    //                    break;
    //                }
    //
    //                stream.write(b"+PONG\r\n").unwrap();
    //            }
    //        }
    //        Err(e) => {
    //            println!("error: {}", e);
    //        }
    //    }
    //}
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

    match &command[0] {
        Value::BulkString(cmd) => {
            let cmd = cmd.to_uppercase();
            match cmd.as_str() {
                "ECHO" => {
                    if command.len() < 2 {
                        return Err(RusdisError::InvalidCommand);
                    }

                    if let Value::BulkString(word) = &command[1] {
                        writer
                            .write_all(format!("+{}\r\n", word).as_bytes())
                            .await?;
                    } else {
                        return Err(RusdisError::InvalidCommand);
                    }
                }
                "PING" => {
                    writer.write_all(b"+PONG\r\n").await?;
                }
                "SET" => {
                    if command.len() < 3 {
                        return Err(RusdisError::InvalidCommand);
                    }

                    let (key, value) = (&command[1], &command[2]);

                    match (key, value) {
                        (Value::BulkString(k), Value::BulkString(v)) => {
                            let mut data_handle = DATABASE.lock().await;
                            let _ = data_handle.insert(k.to_string(), v.to_string());
                            writer.write_all(b"+OK\r\n").await?;
                        }
                        _ => return Err(RusdisError::InvalidCommand),
                    }
                }
                "GET" => {
                    if command.len() < 2 {
                        return Err(RusdisError::InvalidCommand);
                    }

                    if let Value::BulkString(key) = &command[1] {
                        let data_handle = DATABASE.lock().await;
                        match data_handle.get(key) {
                            Some(val) => {
                                writer
                                    .write_all(format!("${}\r\n{}\r\n", val.len(), val).as_bytes())
                                    .await?;
                            }
                            None => {
                                writer.write_all(b"$-1\r\n").await?;
                            }
                        }
                    } else {
                        return Err(RusdisError::InvalidCommand);
                    }
                }
                _ => {}
            }
        }
        _ => {}
    }
    Ok(())
}
