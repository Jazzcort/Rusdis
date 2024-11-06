mod parser;

use crate::parser::{parse, ParserError, Value};
use lazy_static::lazy_static;
use regex::Regex;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::tcp::WriteHalf;
use tokio::net::{TcpListener, TcpStream};
use tokio::task;

//lazy_static! {
//    static ref START_WITH_SPECIAL: Regex = Regex::new(r#"^([\+-:$\*_#,=\(!%`>~])"#).unwrap();
//    static ref ARRAY_STRUCT: Regex = Regex::new(r#"^*"#).unwrap();
//    static ref BULK_STRING_STRUCT: Regex = Regex::new(r#"^$"#).unwrap();
//}

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

async fn handle_commands(mut stream: TcpStream) -> Result<(), ParserError> {
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

        execute_commands(value, &mut writer).await?;

        //if commands == "*1\r\n$4\r\nPING\r\n" {
        //    writer.write_all(b"+PONG\r\n").await?;
        //}
    }

    //let commands = commands.split("\n");

    //for command in commands {
    //    match command {
    //        "ping" | "PING" => {
    //            writer.write_all(b"+PONG\r\n").await?;
    //        }
    //        _ => {}
    //    }
    //}

    Ok(())
}

async fn execute_commands(command: Value, writer: &mut WriteHalf<'_>) -> Result<(), ParserError> {
    dbg!(&command);
    match command {
        Value::Array(values_vec) => {
            for v in values_vec.into_iter() {
                dbg!(&v);
                Box::pin(execute_commands(v, writer)).await?;
            }
        }
        Value::BulkString(s) => {
            let s_idx = s.find(" ");
            let mut c = s.to_uppercase();
            let mut data = String::new();
            if s_idx.is_some() {
                let (lhs, rhs) = s.split_at(s_idx.unwrap());
                c = lhs.to_uppercase();
                data = rhs[1..].to_string();
            }
            dbg!(&c);

            match c.as_str() {
                "ECHO" => {
                    let length = data.len();
                    dbg!(&data);
                    writer
                        .write_all(format!("${}\r\n{}\r\n", length, data).as_bytes())
                        .await?;
                }
                "PING" => {
                    writer.write_all(b"+PONG\r\n").await?;
                }
                _ => {}
            }
        }
        Value::SimpleString(s) => {
            let upper_s = s.to_uppercase();
            dbg!(&upper_s);

            match upper_s.as_str() {
                "PING" => {
                    writer.write_all(b"+PONG\r\n").await?;
                }
                _ => {}
            }
        }
        _ => {}
    }

    Ok(())
}
