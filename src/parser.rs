use thiserror::Error;

#[derive(Error, Debug)]
pub enum ParserError {
    #[error("Invalid protocol: {msg:?}")]
    InvalidProtocol { msg: String },
    #[error("IO errors")]
    IO(#[from] std::io::Error),
    #[error("Parse int errors")]
    ParseIntError(#[from] std::num::ParseIntError),
}

#[derive(Debug, PartialEq, Clone)]
pub enum Value {
    SimpleString(String),
    SimpleError(String),
    Integer(i32),
    BulkString(String),
    Array(Vec<Value>),
    Null,
}

pub fn parse_multi_array(protocol: String) -> Result<Vec<Value>, ParserError> {
    let mut first = true;
    let mut res_vec = vec![];
    let mut pre = 0;
    let char_vec = protocol.chars().collect::<Vec<char>>();

    for idx in 0..char_vec.len() {
        if idx == char_vec.len() - 1 {
            let slice = &char_vec[pre..];
            res_vec.push(parse(slice.into_iter().collect::<String>())?)
        }

        if char_vec[idx] == '*' && char_vec[idx + 1].is_numeric() {
            if first {
                first = false;
            } else {
                let slice = &char_vec[pre..idx];
                res_vec.push(parse(slice.into_iter().collect::<String>())?);
                pre = idx;
            }
        }
    }

    Ok(res_vec)
}

pub fn parse(protocol: String) -> Result<Value, ParserError> {
    let symbol = protocol.chars().nth(0);

    if symbol.is_none() {
        return Err(ParserError::InvalidProtocol {
            msg: "No leading symbol".to_string(),
        });
    }

    let symbol = symbol.unwrap();

    match symbol {
        '+' => Ok(parse_simple_string(protocol)?),
        ':' => Ok(parse_integer(protocol)?),
        '$' => Ok(parse_bulk_string(protocol)?),
        '*' => {
            //Ok(Value::Integer(3))
            Ok(parse_array(protocol)?)
        }
        _ => Err(ParserError::InvalidProtocol {
            msg: "Invalid symbol".to_string(),
        }),
    }
}

fn parse_simple_string(s: String) -> Result<Value, ParserError> {
    let crlf = s.find("\r\n");
    if crlf.is_none() {
        return Err(ParserError::InvalidProtocol {
            msg: "No CRLF".to_string(),
        });
    }
    Ok(Value::SimpleString(s[1..crlf.unwrap()].to_string()))
}

fn parse_integer(s: String) -> Result<Value, ParserError> {
    let crlf = s.find("\r\n");
    if crlf.is_none() {
        return Err(ParserError::InvalidProtocol {
            msg: "No CRLF".to_string(),
        });
    }

    let num_str = s[1..crlf.unwrap()].to_string();
    let num = num_str.parse::<i32>()?;

    Ok(Value::Integer(num))
}

fn parse_bulk_string(s: String) -> Result<Value, ParserError> {
    let mut protocol_iter = s.split("\r\n").into_iter();

    let (first, second) = (protocol_iter.next(), protocol_iter.next());

    match (first, second) {
        (Some(length_str), Some(data)) => {
            let length_str = length_str[1..].to_string();
            let length = length_str.parse::<i32>()?;

            if length == -1 {
                return Ok(Value::Null);
            } else if length < -1 {
                return Err(ParserError::InvalidProtocol {
                    msg: "Invalid Bulk String length".to_string(),
                });
            }

            Ok(Value::BulkString(data[..length as usize].to_string()))
        }
        _ => Err(ParserError::InvalidProtocol {
            msg: "Invalid Bulk String format".to_string(),
        }),
    }
}

fn parse_array(protocol: String) -> Result<Value, ParserError> {
    let x = protocol.clone();
    let x_v = x.split("\r\n").collect::<Vec<&str>>();
    dbg!(x_v);
    let mut protocol_iter = protocol.split("\r\n").into_iter();

    let first = protocol_iter.next();
    if first.is_none() {
        return Err(ParserError::InvalidProtocol {
            msg: "Invalid Array format".to_string(),
        });
    }

    let length_str = first.unwrap();
    let length_str = length_str[1..].to_string();
    let mut left = length_str.parse::<i32>()?;

    // handle -1 case
    if left == -1 {
        return Ok(Value::Null);
    } else if left < -1 {
        return Err(ParserError::InvalidProtocol {
            msg: "Invalid Array length".to_string(),
        });
    }

    let mut protocol_vec = vec![];
    let mut tmp = String::new();
    let mut qoda = 1;

    while let Some(p) = protocol_iter.next() {
        if left == 0 {
            break;
        }

        qoda += count_qoda(p);
        tmp += p;
        tmp += "\r\n";
        qoda -= 1;

        if qoda == 0 {
            if !tmp.is_empty() {
                protocol_vec.push(tmp.clone());
                tmp.clear();
                left -= 1;
            }
            qoda = 1;
        }
    }

    let mut res_vec = vec![];

    for p in protocol_vec.into_iter() {
        res_vec.push(parse(p)?);
    }

    Ok(Value::Array(res_vec))
}

fn count_qoda(s: &str) -> i32 {
    if s.len() < 1 {
        return 0;
    }
    let symbol = &s[..1];

    match symbol {
        "$" => {
            if s.len() < 2 {
                return 0;
            }

            let num_str = &s[1..];
            if let Ok(num) = num_str.parse::<i32>() {
                if num < 0 {
                    0
                } else {
                    1
                }
            } else {
                0
            }
        }
        "*" => {
            if s.len() < 2 {
                return 0;
            }

            let num_str = &s[1..];
            if let Ok(num) = num_str.parse::<i32>() {
                if num < 0 {
                    0
                } else {
                    num
                }
            } else {
                0
            }
        }
        _ => 0,
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn parser_test_parse_simple_string() {
        let test_string = "+OK\r\n".to_string();
        let res = parse(test_string);
        if res.is_err() {
            assert!(false);
        }

        let res = res.unwrap();

        assert_eq!(res, Value::SimpleString("OK".to_string()))
    }

    #[test]
    fn parser_test_parse_integer() {
        let test_string = ":2\r\n".to_string();
        let res = parse(test_string);
        if res.is_err() {
            assert!(false);
        }

        let res = res.unwrap();

        assert_eq!(res, Value::Integer(2))
    }

    #[test]
    fn parser_test_parse_array() {
        let test_string =
            "*4\r\n*3\r\n+PING\r\n*1\r\n:2\r\n$3\r\nhey\r\n:1\r\n*2\r\n:3\r\n$2\r\nhi\r\n:2\r\n"
                .to_string();
        let res = parse(test_string);
        if res.is_err() {
            assert!(false);
        }

        let res = res.unwrap();

        assert_eq!(
            res,
            Value::Array(vec![
                Value::Array(vec![
                    Value::SimpleString("PING".to_string()),
                    Value::Array(vec![Value::Integer(2)]),
                    Value::BulkString("hey".to_string())
                ]),
                Value::Integer(1),
                Value::Array(vec![Value::Integer(3), Value::BulkString("hi".to_string())]),
                Value::Integer(2)
            ])
        )
    }

    #[test]
    fn test_parser_array_with_null_bulk_string() {
        let test_string = "*3\r\n$5\r\nhello\r\n$-1\r\n$5\r\nworld\r\n".to_string();
        let res = parse(test_string);
        assert!(res.is_ok());

        let res = res.unwrap();
        assert_eq!(
            res,
            Value::Array(vec![
                Value::BulkString("hello".to_string()),
                Value::Null,
                Value::BulkString("world".to_string())
            ])
        );
    }

    #[test]
    fn test_parser_null_array() {
        let test_string = "*-1\r\n".to_string();
        let res = parse(test_string);
        assert!(res.is_ok());

        let res = res.unwrap();

        assert_eq!(res, Value::Null);
    }

    #[test]
    fn test_parser_empty_array() {
        let test_string = "*0\r\n".to_string();
        let res = parse(test_string);
        assert!(res.is_ok());

        let res = res.unwrap();
        assert_eq!(res, Value::Array(vec![]));
    }

    #[test]
    fn test_parser_null_bulk_string() {
        let test_string = "$-1\r\n".to_string();
        let res = parse(test_string);
        assert!(res.is_ok());

        let res = res.unwrap();
        assert_eq!(res, Value::Null);
    }

    #[test]
    fn test_parser_empty_bulk_string() {
        let test_string = "$0\r\n\r\n".to_string();
        let res = parse(test_string);
        assert!(res.is_ok());

        let res = res.unwrap();
        assert_eq!(res, Value::BulkString("".to_string()));
    }

    #[test]
    fn test_parser_multi_array() {
        let test_string = "*3\r\n$3\r\nSET\r\n$3\r\nfoo\r\n$3\r\n123\r\n*3\r\n$3\r\nSET\r\n$3\r\nbar\r\n$3\r\n456\r\n*3\r\n$3\r\nSET\r\n$3\r\nbaz\r\n$3\r\n789\r\n".to_string();
        let res = parse_multi_array(test_string);
        assert!(res.is_ok());

        let res = res.unwrap();
        assert_eq!(
            res,
            vec![
                Value::Array(vec![
                    Value::BulkString("SET".to_string()),
                    Value::BulkString("foo".to_string()),
                    Value::BulkString("123".to_string())
                ]),
                Value::Array(vec![
                    Value::BulkString("SET".to_string()),
                    Value::BulkString("bar".to_string()),
                    Value::BulkString("456".to_string())
                ]),
                Value::Array(vec![
                    Value::BulkString("SET".to_string()),
                    Value::BulkString("baz".to_string()),
                    Value::BulkString("789".to_string())
                ])
            ]
        )
    }
}
