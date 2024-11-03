// Uncomment this block to pass the first stage
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::task;

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

async fn handle_commands(mut stream: TcpStream) -> Result<(), std::io::Error> {
    let (reader, mut writer) = stream.split();
    let mut reader = BufReader::new(reader);
    let mut buf = Vec::from(reader.fill_buf().await?);
    reader.consume(buf.len());

    let commands = String::from_utf8_lossy(&buf);
    if commands == "*1\r\n$4\r\nPING\r\n" {
        writer.write_all(b"+PONG\r\n").await?;
    }
    println!("{}", commands);
    let commands = commands.split("\n");

    for command in commands {
        match command {
            "ping" | "PING" => {
                writer.write_all(b"+PONG\r\n").await?;
            }
            _ => {}
        }
    }

    Ok(())
}
