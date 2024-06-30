// Uncomment this block to pass the first stage
use std::{
    io::{Read, Write},
    net::TcpListener,
};

fn main() {
    // You can use print statements as follows for debugging, they'll be visible when running tests.
    println!("Logs from your program will appear here!");

    // Uncomment this block to pass the first stage
    //
    let listener = TcpListener::bind("127.0.0.1:6379").unwrap();
    //
    for stream in listener.incoming() {
        match stream {
            Ok(mut stream) => {
                let mut req = String::new();
                if let Ok(_) = stream.read_to_string(&mut req) {
                    let a: Vec<&str> = req.split("\r\n").collect();
                    match a[2].to_lowercase().as_str() {
                        // "ping" => {
                        //     let response = String::from("+PONG\r\n");
                        //     stream.write(response.as_bytes()).unwrap();

                        // }
                        _ => {
                            let response = String::from("+PONG\r\n");
                            stream.write(response.as_bytes()).unwrap();
                        }
                    }
                }
                println!("accepted new connection");
            }
            Err(e) => {
                println!("error: {}", e);
            }
        }
    }
}
