mod cli_parser;
mod command_parser;
mod data;
mod error;
mod parser;
mod rdb_file_reader;
mod utils;

use crate::cli_parser::Args;
use crate::command_parser::{parse_command, Command, ReplconfSubcommand};
use crate::data::{Admin, Database, ReplicaRole, ReplicationInfo, StringData};
use crate::error::RusdisError;
use crate::parser::{parse, parse_multi_array, ParserError, Value};
use crate::rdb_file_reader::read_rdb;
use crate::utils::generate_resp;
use clap::Parser;
use command_parser::{ConfigGetOption, ConfigSubcommand, InfoSection};
use lazy_static::lazy_static;
use regex::Regex;
use std::collections::{HashMap, VecDeque};
use std::fs::File;
use std::io::prelude::*;
use std::iter::Peekable;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::tcp::{ReadHalf, WriteHalf};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::broadcast::{channel, Sender};
use tokio::sync::{Mutex, RwLock};
use tokio::task;

lazy_static! {
    //static ref START_WITH_SPECIAL: Regex = Regex::new(r#"^([\+-:$\*_#,=\(!%`>~])"#).unwrap();
    //static ref ARRAY_STRUCT: Regex = Regex::new(r#"^*"#).unwrap();
    //static ref BULK_STRING_STRUCT: Regex = Regex::new(r#"^$"#).unwrap();
    static ref SLAVES_COUNT: RwLock<usize> = RwLock::new(0);
    static ref BROADCAST_CHANNEL: Sender<String> = channel(100).0;
    static ref ARGS: RwLock<Args> = RwLock::new(Args::new());
    static ref ADMIN: Arc<Mutex<Admin>> = Arc::new(Mutex::new(Admin::new(vec![])));
    static ref DIR: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    static ref DBFILENAME: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    static ref REPLICATION_INFO: Arc<RwLock<ReplicationInfo>> = Arc::new(RwLock::new(ReplicationInfo::new()));
}

#[tokio::main]
async fn main() -> Result<(), RusdisError> {
    let mut args = Args::parse();
    //if args.port.is_none() {
    //    args.port = Some("6379".to_string())
    //}
    let mut args_writer = ARGS.write().await;
    *args_writer = args;
    drop(args_writer);
    //tokio::join!(
    //    async {
    //        let mut dir_handle = DIR.lock().await;
    //        *dir_handle = args.dir.clone();
    //    },
    //    async {
    //        let mut dbfilename_handle = DBFILENAME.lock().await;
    //        *dbfilename_handle = args.dbfilename.clone();
    //    },
    //);

    match ARGS.read().await.replicaof.clone() {
        Some(s) => {
            let mut replication_info_handle = REPLICATION_INFO.write().await;
            replication_info_handle.change_role(ReplicaRole::Slave);
            drop(replication_info_handle);

            let split_idx = s.find(" ");
            if split_idx.is_none() {
                eprintln!("Invalid --replicaif parameters");
                return Ok(());
            }

            let (host, port) = s.split_at(split_idx.unwrap());
            let master_stream = TcpStream::connect(format!("{}:{}", host, port.trim())).await;

            if master_stream.is_err() {
                eprintln!("Master is offline");
                return Ok(());
            }

            let master_stream = master_stream.unwrap();
            connect_master(master_stream).await?;
        }
        None => {}
    }

    let (dir_option, dbfilename_option) = tokio::join!(
        async {
            let args_read = ARGS.read().await;
            args_read.dir.clone()
        },
        async {
            let args_read = ARGS.read().await;
            args_read.dbfilename.clone()
        },
    );

    match (dir_option, dbfilename_option) {
        (Some(dir), Some(dbfilename)) => {
            let f = File::open(dir + "/" + &dbfilename);
            if let Ok(f) = f {
                let mut reader = std::io::BufReader::new(f);
                let mut buf = vec![];
                let _length = reader.read_to_end(&mut buf)?;
                let iter = buf.into_iter().peekable();

                let res = read_rdb(iter);
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
        }
        _ => {}
    }

    let admin_handle = ADMIN.lock().await;
    let string_data_arc = admin_handle.get_string_data_map();
    drop(admin_handle);

    // You can use print statements as follows for debugging, they'll be visible when running tests.
    println!("Logs from your program will appear here!");

    let port = match ARGS.read().await.port.clone() {
        Some(p) => p,
        None => "6379".to_string(),
    };
    let listener = TcpListener::bind(format!("127.0.0.1:{}", port))
        .await
        .unwrap();

    loop {
        let res = listener.accept().await;

        match res {
            Ok((stream, addr)) => {
                println!("accepted new connection: {}", addr.to_string());
                task::spawn(async move {
                    handle_commands(stream, addr.to_string()).await;
                });
            }
            Err(e) => {
                println!("error: {}", e);
            }
        }
    }
}

async fn connect_master(mut stream: TcpStream) -> Result<(), RusdisError> {
    let (reader, mut writer) = stream.split();
    let mut reader = BufReader::new(reader);

    // PING Master
    writer.write_all(b"*1\r\n$4\r\nPING\r\n").await?;
    let buf = Vec::from(reader.fill_buf().await?);
    reader.consume(buf.len());
    let response = parse(String::from_utf8_lossy(&buf).to_string())?;
    if let Value::SimpleString(r) = response {
        let r = r.to_uppercase();
        if r.as_str() != "PONG" {
            return Err(RusdisError::MasterConnectionError {
                msg: "Wrong response to ping".to_string(),
            });
        }
    }

    let port = match ARGS.read().await.port.clone() {
        Some(p) => p,
        None => "6379".to_string(),
    };

    // REPLCONF listening-port
    writer
        .write_all(
            format!(
                "*3\r\n$8\r\nREPLCONF\r\n$14\r\nlistening-port\r\n${}\r\n{}\r\n",
                port.len(),
                port
            )
            .as_bytes(),
        )
        .await?;
    let buf = Vec::from(reader.fill_buf().await?);
    reader.consume(buf.len());
    let response = parse(String::from_utf8_lossy(&buf).to_string())?;
    if let Value::SimpleString(r) = response {
        let r = r.to_uppercase();
        if r.as_str() != "OK" {
            return Err(RusdisError::MasterConnectionError {
                msg: "Wrong response to replconf".to_string(),
            });
        }
    }

    // REPLCONF capa
    writer
        .write_all(b"*3\r\n$8\r\nREPLCONF\r\n$4\r\ncapa\r\n$6\r\npsync2\r\n")
        .await?;

    let buf = Vec::from(reader.fill_buf().await?);
    reader.consume(buf.len());
    let response = parse(String::from_utf8_lossy(&buf).to_string())?;
    if let Value::SimpleString(r) = response {
        let r = r.to_uppercase();
        if r.as_str() != "OK" {
            return Err(RusdisError::MasterConnectionError {
                msg: "Wrong response to replconf".to_string(),
            });
        }
    }

    writer
        .write_all(b"*3\r\n$5\r\nPSYNC\r\n$1\r\n?\r\n$2\r\n-1\r\n")
        .await?;
    let mut buf = vec![];
    reader.read_until('\n' as u8, &mut buf).await;

    let psync_response = parse(String::from_utf8_lossy(&buf).to_string())?;
    dbg!(&psync_response);

    let buf = Vec::from(reader.fill_buf().await?);
    reader.consume(buf.len());
    let idx = buf.iter().position(|&x| x == '\n' as u8);
    if idx.is_some() {
        let idx = idx.unwrap();
        let file_buf = Vec::from(&buf[idx + 1..]);

        let rdb_file = read_rdb(file_buf.into_iter().peekable());
        dbg!(rdb_file);
    }

    tokio::spawn(async move {
        let (reader, mut writer) = stream.split();
        let mut reader = BufReader::new(reader);
        loop {
            if let Ok(buf) = reader.fill_buf().await {
                let buf = Vec::from(buf);
                let buf_length = buf.len();
                if buf.len() == 0 {
                    break;
                }
                reader.consume(buf.len());
                let commands = String::from_utf8_lossy(&buf).to_string();
                dbg!(&commands);
                let parse_res = parse_multi_array(commands);

                if let Ok(array_vec) = parse_res {
                    for value in array_vec.into_iter() {
                        if let Value::Array(bulk_string_vec) = value {
                            let parse_res = parse_command(bulk_string_vec);
                            dbg!(&parse_res);
                            if let Ok(cmd) = parse_res {
                                let reply_msg =
                                    execute_multi_commands(vec![cmd.clone()], false).await;

                                dbg!(&reply_msg);

                                if let Command::Replconf(ReplconfSubcommand::Getack(_)) = cmd {
                                    writer.write_all(reply_msg.as_bytes()).await;
                                }
                            }
                        }
                    }
                }

                let mut replication_info_write = REPLICATION_INFO.write().await;
                replication_info_write.increment_offset(buf_length as u64);
            }
        }
    });

    Ok(())
}

async fn handle_commands(mut stream: TcpStream, addr: String) -> Result<(), RusdisError> {
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
                    Command::Psync { id, offset } => match id {
                        Some(master_id) => {}
                        None => {
                            let replica_info_read = REPLICATION_INFO.read().await;
                            let (master_id, cur_offset) = (
                                replica_info_read.get_master_replid(),
                                replica_info_read.get_master_repl_offset(),
                            );

                            writer
                                .write_all(
                                    format!("+FULLRESYNC {} {}\r\n", master_id, cur_offset)
                                        .as_bytes(),
                                )
                                .await;

                            drop(replica_info_read);

                            // Transfer RDB file
                            let file = "524544495330303131fa0972656469732d76657205372e322e30fa0a72656469732d62697473c040fa056374696d65c26d08bc65fa08757365642d6d656dc2b0c41000fa08616f662d62617365c000fff06e3bfec0ff5aa2";

                            let data_slice = file.chars().collect::<Vec<char>>();
                            let data_vec = data_slice
                                .chunks(2)
                                .into_iter()
                                .map(|a| {
                                    let x = a.into_iter().collect::<String>();
                                    u8::from_str_radix(&x, 16).unwrap()
                                })
                                .collect::<Vec<u8>>();
                            let prefix = format!("${}\r\n", data_vec.len());
                            writer
                                .write_all(
                                    [prefix.as_bytes(), data_vec.as_slice()].concat().as_slice(),
                                )
                                .await;
                            build_replica_pipe(stream);
                            return Ok(());
                        }
                    },
                    // Maybe can move this part to execuate_multi_commands
                    //Command::Replconf(subcommand) => match subcommand {
                    //    ReplconfSubcommand::ListeningPort(port) => {
                    //        // Store the replica's port
                    //        writer.write_all(b"+OK\r\n").await;
                    //    }
                    //    ReplconfSubcommand::Capa(options) => {
                    //        // Configure capa?
                    //        writer.write_all(b"+OK\r\n").await;
                    //    }
                    //    _ => {}
                    //},
                    Command::Multi => {
                        is_multi = true;
                        writer.write_all(b"+OK\r\n").await;
                    }
                    Command::Exec => {
                        if !is_multi {
                            writer.write_all(b"-ERR EXEC without MULTI\r\n").await;
                            continue;
                        }

                        let reply_string = execute_multi_commands(queue, true).await;
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
                            //execute_commands(other, &mut writer).await;
                            let reply_string = execute_multi_commands(vec![other], false).await;
                            writer.write_all(reply_string.as_bytes()).await;
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

fn build_replica_pipe(mut stream: TcpStream) {
    let mut rx = BROADCAST_CHANNEL.subscribe();
    tokio::spawn(async move {
        let (reader, mut writer) = stream.split();
        let mut reader = BufReader::new(reader);
        let mut slaves_count_write = SLAVES_COUNT.write().await;
        *slaves_count_write += 1;
        drop(slaves_count_write);
        loop {
            tokio::select! {
                result = rx.recv() => {
                    if let Ok(cmd) = result {
                        dbg!(&cmd);

                        writer.write_all(cmd.as_bytes()).await;
                    }
                }
            }
        }

        let mut slaves_count_write = SLAVES_COUNT.write().await;
        *slaves_count_write -= 1;
    });
}

async fn execute_multi_commands(commands: Vec<Command>, is_multi: bool) -> String {
    let mut res = String::new();
    if is_multi {
        res = format!("*{}\r\n", commands.len());
    }

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
                        let dir_ref = &ARGS.read().await.dir;
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
                        let dbfilename_ref = &ARGS.read().await.dbfilename;
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
                let _ = string_data_handle
                    .insert(key.clone(), StringData::new(value.clone(), expiration));
                res += "+OK\r\n";

                if REPLICATION_INFO.read().await.get_role() == ReplicaRole::Master
                    && *SLAVES_COUNT.read().await != 0
                {
                    let _ = BROADCAST_CHANNEL.send(generate_resp(Command::Set { key, value, px }));
                }
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

                let mut reply_string = format!("*{}\r\n", res_vec.len());
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
            Command::Info(sections) => {
                let mut string = String::new();
                let mut cnt = 0;
                for section in sections.into_iter() {
                    match section {
                        InfoSection::Replication => {
                            let replication_info_handle = REPLICATION_INFO.read().await;
                            let role = format!(
                                "role:{}\n",
                                match replication_info_handle.get_role() {
                                    ReplicaRole::Master => "master",
                                    ReplicaRole::Slave => "slave",
                                }
                            );
                            string += role.as_str();

                            cnt += role.len();

                            let master_replid = format!(
                                "master_replid:{}\n",
                                replication_info_handle.get_master_replid()
                            );
                            string += master_replid.as_str();
                            cnt += master_replid.len();

                            let master_repl_offset = format!(
                                "master_repl_offset:{}\n",
                                replication_info_handle.get_master_repl_offset()
                            );

                            string += master_repl_offset.as_str();
                            cnt += master_repl_offset.len();
                        }
                    }
                }
                string.pop();
                cnt -= 1;
                string += "\r\n";
                string = format!("${}\r\n", cnt) + string.as_str();
                res += string.as_str();
            }
            Command::Replconf(subcommand) => match subcommand {
                ReplconfSubcommand::ListeningPort(port) => {
                    // Store the replica's port
                    res += "+OK\r\n";
                }
                ReplconfSubcommand::Capa(options) => {
                    // Configure capa?
                    res += "+OK\r\n";
                }
                ReplconfSubcommand::Getack(_) => {
                    let offset = REPLICATION_INFO
                        .read()
                        .await
                        .get_master_repl_offset()
                        .to_string();
                    dbg!(&offset);
                    let ack_msg = format!(
                        "*3\r\n$8\r\nREPLCONF\r\n$3\r\nACK\r\n${}\r\n{}\r\n",
                        offset.len(),
                        offset
                    );

                    res += ack_msg.as_str();
                }
                _ => {}
            },
            _ => {
                res += "-ERR not supported command";
            }
        }
    }

    res
}

// Review it and delete
//async fn execute_commands(command: Command, writer: &mut WriteHalf<'_>) -> Result<(), RusdisError> {
//    dbg!(&command);
//
//    match command {
//        Command::Ping => {
//            writer.write_all(b"+PONG\r\n").await?;
//        }
//        Command::Echo(words) => {
//            writer
//                .write_all(format!("+{}\r\n", words).as_bytes())
//                .await?;
//        }
//        Command::Config(subcommand) => match subcommand {
//            ConfigSubcommand::Get(option) => match option {
//                ConfigGetOption::Dir => {
//                    let dir_handle = DIR.lock().await;
//                    let dir_ref = dir_handle.as_ref();
//                    match dir_ref {
//                        Some(dir) => {
//                            writer
//                                .write_all(
//                                    format!("*2\r\n$3\r\ndir\r\n${}\r\n{}\r\n", dir.len(), dir)
//                                        .as_bytes(),
//                                )
//                                .await?;
//                        }
//                        None => {
//                            writer.write_all(b"*2\r\n$3\r\ndir\r\n$-1\r\n").await?;
//                        }
//                    }
//                }
//                ConfigGetOption::DbFilename => {
//                    let dbfilename_handle = DBFILENAME.lock().await;
//                    let dbfilename_ref = dbfilename_handle.as_ref();
//                    match dbfilename_ref {
//                        Some(dbfilename) => {
//                            writer
//                                .write_all(
//                                    format!(
//                                        "*2\r\n$10\r\ndbfilename\r\n${}\r\n{}\r\n",
//                                        dbfilename.len(),
//                                        dbfilename
//                                    )
//                                    .as_bytes(),
//                                )
//                                .await?;
//                        }
//                        None => {
//                            writer
//                                .write_all(b"*2\r\n$10\r\ndbfilename\r\n$-1\r\n")
//                                .await?;
//                        }
//                    }
//                }
//            },
//        },
//        Command::Set { key, value, px } => {
//            // Todo: implement "active" or "passive" way to delete data
//            let mut expiration = None;
//
//            if let Some(mills) = px {
//                let now = SystemTime::now();
//                let fu = now.checked_add(Duration::from_millis(mills as u64));
//                if fu.is_none() {
//                    return Err(RusdisError::InstantAdditionError);
//                }
//
//                expiration = fu;
//            }
//
//            let admin_handle = ADMIN.lock().await;
//            let string_data_arc = admin_handle.get_string_data_map();
//            drop(admin_handle);
//            let mut string_data_handle = string_data_arc.lock().await;
//            let _ = string_data_handle.insert(key, StringData::new(value, expiration));
//            writer.write_all(b"+OK\r\n").await?;
//        }
//        Command::Get(key) => {
//            let admin_handle = ADMIN.lock().await;
//            let string_data_arc = admin_handle.get_string_data_map();
//            drop(admin_handle);
//            let mut string_data_handle = string_data_arc.lock().await;
//
//            match string_data_handle.get(&key) {
//                Some(data) => {
//                    if data.is_expired() {
//                        let _ = string_data_handle.remove(&key);
//                        writer.write_all(b"$-1\r\n").await?;
//                    } else {
//                        writer
//                            .write_all(
//                                format!("${}\r\n{}\r\n", data.get_data().len(), data.get_data())
//                                    .as_bytes(),
//                            )
//                            .await?;
//                    }
//                }
//                None => {
//                    writer.write_all(b"$-1\r\n").await?;
//                }
//            }
//        }
//        Command::Keys(pattern_string) => {
//            let pattern = Regex::new(&pattern_string)?;
//            let admin_handle = ADMIN.lock().await;
//            let string_data_arc = admin_handle.get_string_data_map();
//            drop(admin_handle);
//            let string_data_handle = string_data_arc.lock().await;
//            let mut res = vec![];
//
//            for key in string_data_handle.keys() {
//                if pattern.is_match(key) {
//                    res.push(key);
//                }
//            }
//
//            let mut reply_string = format!("*{}\r\n", res.len());
//            for matched_key in res.into_iter() {
//                reply_string += format!("${}\r\n{}\r\n", matched_key.len(), matched_key).as_str();
//            }
//
//            writer.write_all(reply_string.as_bytes()).await?;
//        }
//        Command::Incr(key) => {
//            let admin_handle = ADMIN.lock().await;
//            let string_data_arc = admin_handle.get_string_data_map();
//            drop(admin_handle);
//
//            let mut string_data_handle = string_data_arc.lock().await;
//            //let a = string_data_handle.get_mut(&key);
//
//            let data = string_data_handle
//                .entry(key)
//                .or_insert(StringData::new("0".to_string(), None));
//            let num_str = data.get_data();
//            match num_str.parse::<i64>() {
//                Ok(mut num) => {
//                    if num < i64::MAX {
//                        num += 1;
//                    }
//
//                    data.set_data(format!("{}", num));
//                    writer.write_all(format!(":{}\r\n", num).as_bytes()).await?;
//                }
//                Err(_) => {
//                    writer
//                        .write_all(b"-ERR value is not an integer or out of range\r\n")
//                        .await?;
//                }
//            }
//        }
//        _ => {}
//    }
//
//    Ok(())
//}
