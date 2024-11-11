use crate::{RusdisError, Value};
#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    Set {
        key: String,
        value: String,
        px: Option<usize>,
    },
    Get(String),
    Ping,
    Echo(String),
    ConfigGet(ConfigGetOption),
}

#[derive(Debug, Clone, PartialEq)]
pub enum ConfigGetOption {
    Dir,
    DbFilename,
}

pub fn parse_command(value_vec: Vec<Value>) -> Result<Command, RusdisError> {
    let mut value_iter = value_vec.into_iter();
    //let mut string_vec = vec![];
    //
    //while let Some(s) = value_iter

    let command = value_iter.next();
    if command.is_none() {
        return Err(RusdisError::CommandParserError {
            msg: "No leading command".to_string(),
        });
    }
    let command = command.unwrap();

    if let Value::BulkString(cmd) = command {
        let cmd = cmd.to_uppercase();
        match cmd.as_str() {
            "SET" => parse_set_command(value_iter),
            "GET" => parse_get_command(value_iter),
            "PING" => Ok(Command::Ping),
            "ECHO" => parse_echo_command(value_iter),
            _ => Err(RusdisError::CommandParserError {
                msg: "Unrecognized command".to_string(),
            }),
        }
    } else {
        Err(RusdisError::CommandParserError {
            msg: "Invalid command format".to_string(),
        })
    }
}

fn parse_get_command(mut iter: impl Iterator<Item = Value>) -> Result<Command, RusdisError> {
    match iter.next() {
        Some(key_v) => {
            if let Value::BulkString(key) = key_v {
                Ok(Command::Get(key))
            } else {
                Err(RusdisError::CommandParserError {
                    msg: "Not Bulk String in command".to_string(),
                })
            }
        }
        None => Err(RusdisError::CommandParserError {
            msg: "No key in get command".to_string(),
        }),
    }
}

fn parse_set_command(mut iter: impl Iterator<Item = Value>) -> Result<Command, RusdisError> {
    match (iter.next(), iter.next()) {
        (Some(key_v), Some(value_v)) => match (key_v, value_v) {
            (Value::BulkString(key), Value::BulkString(value)) => {
                let mut px: Option<usize> = None;

                while let Some(v) = iter.next() {
                    if let Value::BulkString(s) = v {
                        let s = s.to_uppercase();

                        match s.as_str() {
                            "PX" => {
                                let value_px = iter.next();

                                match value_px {
                                    Some(mil_sec_bulk_str) => {
                                        if let Value::BulkString(mil_sec_str) = mil_sec_bulk_str {
                                            let mil_sec = mil_sec_str.parse::<usize>()?;
                                            px = Some(mil_sec);
                                        } else {
                                            return Err(RusdisError::CommandParserError {
                                                msg: "Not Bulk String in command".to_string(),
                                            });
                                        }
                                    }
                                    None => {
                                        return Err(RusdisError::CommandParserError {
                                            msg: "No millisecond value after PX".to_string(),
                                        })
                                    }
                                }
                            }
                            _ => {}
                        }
                    } else {
                        return Err(RusdisError::CommandParserError {
                            msg: "Not Bulk String in command".to_string(),
                        });
                    }
                }

                Ok(Command::Set { key, value, px })
            }
            _ => Err(RusdisError::CommandParserError {
                msg: "Not Bulk String in command".to_string(),
            }),
        },
        _ => Err(RusdisError::CommandParserError {
            msg: "No key or value in set command".to_string(),
        }),
    }
}

fn parse_echo_command(mut iter: impl Iterator<Item = Value>) -> Result<Command, RusdisError> {
    match iter.next() {
        Some(value) => {
            if let Value::BulkString(words) = value {
                Ok(Command::Echo(words))
            } else {
                Err(RusdisError::CommandParserError {
                    msg: "Not Bulk String in command".to_string(),
                })
            }
        }
        None => Err(RusdisError::CommandParserError {
            msg: "Echo without words".to_string(),
        }),
    }
}

#[cfg(test)]

mod test {
    use super::*;

    #[test]
    fn test_command_parser_set_command_without_px() {
        let test_vec = vec![
            Value::BulkString("seT".to_string()),
            Value::BulkString("a".to_string()),
            Value::BulkString("30".to_string()),
        ];

        let res = parse_command(test_vec);
        assert!(res.is_ok());

        let res = res.unwrap();
        assert_eq!(
            res,
            Command::Set {
                key: "a".to_string(),
                value: "30".to_string(),
                px: None
            }
        );
    }

    #[test]
    fn test_command_parser_set_command_with_px() {
        let test_vec = vec![
            Value::BulkString("seT".to_string()),
            Value::BulkString("a".to_string()),
            Value::BulkString("30".to_string()),
            Value::BulkString("pX".to_string()),
            Value::BulkString("5000".to_string()),
        ];

        let res = parse_command(test_vec);
        assert!(res.is_ok());

        let res = res.unwrap();
        assert_eq!(
            res,
            Command::Set {
                key: "a".to_string(),
                value: "30".to_string(),
                px: Some(5000)
            }
        );
    }

    #[test]
    fn test_command_parser_set_command_with_not_bulk_string() {
        let test_vec = vec![
            Value::BulkString("seT".to_string()),
            Value::BulkString("a".to_string()),
            Value::BulkString("30".to_string()),
            Value::SimpleString("pX".to_string()),
            Value::BulkString("5000".to_string()),
        ];

        let res = parse_command(test_vec);
        let is_err_correct = res.is_err_and(|e| {
            e.to_string() == "Command Parser Error: Not Bulk String in command".to_string()
        });

        assert!(is_err_correct);
    }

    #[test]
    fn test_command_parser_set_command_px_without_value() {
        let test_vec = vec![
            Value::BulkString("seT".to_string()),
            Value::BulkString("a".to_string()),
            Value::BulkString("30".to_string()),
            Value::BulkString("Px".to_string()),
        ];

        let res = parse_command(test_vec);
        let is_err_correct = res.is_err_and(|e| {
            e.to_string() == "Command Parser Error: No millisecond value after PX".to_string()
        });

        assert!(is_err_correct);
    }

    #[test]
    fn test_command_parser_set_command_px_with_invalid_value() {
        let test_vec = vec![
            Value::BulkString("seT".to_string()),
            Value::BulkString("a".to_string()),
            Value::BulkString("30".to_string()),
            Value::BulkString("Px".to_string()),
            Value::BulkString("-1000".to_string()),
        ];

        let res = parse_command(test_vec);
        let is_err_correct = res.is_err_and(|e| e.to_string() == "Parse int errors".to_string());

        assert!(is_err_correct);
    }

    #[test]
    fn test_command_parser_set_command_with_useless_flag() {
        let test_vec = vec![
            Value::BulkString("seT".to_string()),
            Value::BulkString("a".to_string()),
            Value::BulkString("30".to_string()),
            Value::BulkString("i love rust".to_string()),
            Value::BulkString("pX".to_string()),
            Value::BulkString("5000".to_string()),
            Value::BulkString("13452".to_string()),
            Value::BulkString("ppp".to_string()),
        ];

        let res = parse_command(test_vec);
        assert!(res.is_ok());

        let res = res.unwrap();
        assert_eq!(
            res,
            Command::Set {
                key: "a".to_string(),
                value: "30".to_string(),
                px: Some(5000)
            }
        );
    }

    #[test]
    fn test_command_parser_set_command_missing_key_or_value() {
        let test_vec = vec![
            Value::BulkString("seT".to_string()),
            Value::BulkString("a".to_string()),
        ];

        let res = parse_command(test_vec);
        let is_err_correct = res.is_err_and(|e| {
            e.to_string() == "Command Parser Error: No key or value in set command".to_string()
        });

        assert!(is_err_correct);
    }

    #[test]
    fn test_command_parser_get_command() {
        let test_vec = vec![
            Value::BulkString("gEt".to_string()),
            Value::BulkString("mypassword".to_string()),
        ];

        let res = parse_command(test_vec);

        assert!(res.is_ok());
    }

    #[test]
    fn test_command_parser_get_command_without_key() {
        let test_vec = vec![Value::BulkString("gEt".to_string())];

        let res = parse_command(test_vec);

        assert!(res.is_err_and(|e| {
            e.to_string() == "Command Parser Error: No key in get command".to_string()
        }));
    }
}
