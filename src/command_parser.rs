use crate::{RusdisError, Value};
use regex::Regex;
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
    Config(ConfigSubcommand),
    Keys(String),
    Incr(String),
    Multi,
    Exec,
    Discard,
    Info(Vec<InfoSection>),
    Replconf(ReplconfSubcommand),
}

#[derive(Debug, Clone, PartialEq)]
pub enum ReplconfSubcommand {
    ListeningPort(u16),
    Capa(Vec<CapaOption>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum CapaOption {
    Eof,
    Psync2,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ConfigSubcommand {
    Get(ConfigGetOption),
}

#[derive(Debug, Clone, PartialEq)]
pub enum InfoSection {
    Replication,
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
            "CONFIG" => parse_config_command(value_iter),
            "KEYS" => parse_keys_command(value_iter),
            "INCR" => parse_incr_command(value_iter),
            "MULTI" => Ok(Command::Multi),
            "EXEC" => Ok(Command::Exec),
            "DISCARD" => Ok(Command::Discard),
            "INFO" => parse_info_command(value_iter),
            "REPLCONF" => parse_replconf_command(value_iter),
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

fn parse_replconf_command(mut iter: impl Iterator<Item = Value>) -> Result<Command, RusdisError> {
    let subcommand_bulk_string = iter.next();
    if subcommand_bulk_string.is_none() {
        return Err(RusdisError::CommandParserError {
            msg: "No subcommand after REPLCONF".to_string(),
        });
    }
    let subcommand_bulk_string = subcommand_bulk_string.unwrap();

    match subcommand_bulk_string {
        Value::BulkString(subcommand) => {
            let subcommand = subcommand.to_uppercase();
            match subcommand.as_str() {
                "LISTENING-PORT" => Ok(Command::Replconf(parse_replconf_listening_port_command(
                    iter,
                )?)),
                "CAPA" => Ok(Command::Replconf(parse_replconf_capa_command(iter)?)),
                _ => Err(RusdisError::CommandParserError {
                    msg: "Invalid REPLCONF subcommand".to_string(),
                }),
            }
        }
        _ => Err(RusdisError::CommandParserError {
            msg: "Not Bulk String in command".to_string(),
        }),
    }
}

fn parse_replconf_capa_command(
    mut iter: impl Iterator<Item = Value>,
) -> Result<ReplconfSubcommand, RusdisError> {
    let mut options = vec![];
    while let Some(value) = iter.next() {
        if let Value::BulkString(s) = value {
            let s = s.to_uppercase();
            match s.as_str() {
                "EOF" => options.push(CapaOption::Eof),
                "PSYNC2" => options.push(CapaOption::Psync2),
                _ => {}
            }
        } else {
            return Err(RusdisError::CommandParserError {
                msg: "Not Bulk String in command".to_string(),
            });
        }
    }

    if options.len() == 0 {
        return Err(RusdisError::CommandParserError {
            msg: "No options after capa".to_string(),
        });
    }

    Ok(ReplconfSubcommand::Capa(options))
}

fn parse_replconf_listening_port_command(
    mut iter: impl Iterator<Item = Value>,
) -> Result<ReplconfSubcommand, RusdisError> {
    match iter.next() {
        Some(p_bulk_string) => {
            if let Value::BulkString(p) = p_bulk_string {
                let port = p.parse::<u16>()?;
                Ok(ReplconfSubcommand::ListeningPort(port))
            } else {
                Err(RusdisError::CommandParserError {
                    msg: "Not Bulk String in command".to_string(),
                })
            }
        }
        None => Err(RusdisError::CommandParserError {
            msg: "REPLCONF listening-port without port parameter".to_string(),
        }),
    }
}

fn parse_info_command(mut iter: impl Iterator<Item = Value>) -> Result<Command, RusdisError> {
    let mut sections = vec![];

    while let Some(value) = iter.next() {
        if let Value::BulkString(s) = value {
            let s = s.to_uppercase();
            match s.as_str() {
                "REPLICATION" => {
                    sections.push(InfoSection::Replication);
                }
                _ => {}
            }
        } else {
            return Err(RusdisError::CommandParserError {
                msg: "Not Bulk String in command".to_string(),
            });
        }
    }

    Ok(Command::Info(sections))
}

fn parse_incr_command(mut iter: impl Iterator<Item = Value>) -> Result<Command, RusdisError> {
    let key = iter.next();
    if key.is_none() {
        return Err(RusdisError::CommandParserError {
            msg: "No key after incr command".to_string(),
        });
    }
    let key = key.unwrap();

    if let Value::BulkString(key) = key {
        Ok(Command::Incr(key))
    } else {
        Err(RusdisError::CommandParserError {
            msg: "Not Bulk String in command".to_string(),
        })
    }
}

fn parse_keys_command(mut iter: impl Iterator<Item = Value>) -> Result<Command, RusdisError> {
    let pattern = iter.next();
    if pattern.is_none() {
        return Err(RusdisError::CommandParserError {
            msg: "No pattern after keys command".to_string(),
        });
    }
    let pattern = pattern.unwrap();

    if let Value::BulkString(pattern) = pattern {
        let mut new_pattern = String::new();

        for b in pattern.into_bytes() {
            if b == '*' as u8 {
                new_pattern += ".*";
            } else {
                new_pattern.push(b as char);
            }
        }

        Ok(Command::Keys(new_pattern))
    } else {
        Err(RusdisError::CommandParserError {
            msg: "Not Bulk String in command".to_string(),
        })
    }
}

fn parse_config_command(mut iter: impl Iterator<Item = Value>) -> Result<Command, RusdisError> {
    match iter.next() {
        Some(subcommand_bulk_string) => {
            if let Value::BulkString(subcommand) = subcommand_bulk_string {
                let subcommand = subcommand.to_uppercase();
                match subcommand.as_str() {
                    "GET" => Ok(Command::Config(parse_config_get_command(iter)?)),
                    _ => Err(RusdisError::CommandParserError {
                        msg: "Unrecognizable subcommand in config command".to_string(),
                    }),
                }
            } else {
                Err(RusdisError::CommandParserError {
                    msg: "Not Bulk String in command".to_string(),
                })
            }
        }
        None => Err(RusdisError::CommandParserError {
            msg: "No subcommand in config command".to_string(),
        }),
    }
}

fn parse_config_get_command(
    mut iter: impl Iterator<Item = Value>,
) -> Result<ConfigSubcommand, RusdisError> {
    match iter.next() {
        Some(value) => {
            if let Value::BulkString(parameter) = value {
                let parameter = parameter.to_uppercase();
                match parameter.as_str() {
                    "DIR" => Ok(ConfigSubcommand::Get(ConfigGetOption::Dir)),
                    "DBFILENAME" => Ok(ConfigSubcommand::Get(ConfigGetOption::DbFilename)),
                    _ => Err(RusdisError::CommandParserError {
                        msg: "Unrecognizable config get option".to_string(),
                    }),
                }
            } else {
                Err(RusdisError::CommandParserError {
                    msg: "Not Bulk String in command".to_string(),
                })
            }
        }
        None => Err(RusdisError::CommandParserError {
            msg: "No parameter in config get command".to_string(),
        }),
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
