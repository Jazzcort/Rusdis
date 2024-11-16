use crate::error::RusdisError;
use std::fs::File;
use std::io::prelude::*;
use std::io::BufReader;
use std::iter::Peekable;
use std::time::Instant;

pub enum Phase {
    Header,
    Metadata,
    Database,
    CheckSum,
}

pub struct RDBFile {
    pub rdb_version: String,
    pub aux_fields: Vec<(String, String)>,
    pub datasets: Vec<Dataset>,
}

pub struct Dataset {
    pub pairs: Vec<(String, ValueType, Option<Instant>)>,
}

impl Dataset {
    pub fn get_pairs(self) -> Vec<(String, ValueType, Option<Instant>)> {
        self.pairs
    }
}

pub enum ValueType {
    String(String),
}

pub fn read_rdb(f_path: String) -> Result<RDBFile, RusdisError> {
    let f = File::open(f_path)?;
    let mut reader = BufReader::new(f);

    let mut buf = vec![];
    let length = reader.read_to_end(&mut buf)?;
    dbg!(&buf);
    dbg!(String::from_utf8_lossy(&buf));

    let mut iter = buf.into_iter().peekable();
    let mut phase = Phase::Header;
    let mut rdb_version = String::new();
    let mut aux_fields = vec![];

    loop {
        match phase {
            Phase::Header => {
                // Magic String
                let mut slice = [0; 5];
                for i in 0..5 {
                    let byte = iter.next();
                    if byte.is_none() {
                        return Err(RusdisError::RDBFileParserError {
                            msg: "Wrong Magic String".to_string(),
                        });
                    }
                    slice[i] = byte.unwrap();
                }
                let magic_string = String::from_utf8_lossy(&slice);
                if magic_string != "REDIS" {
                    return Err(RusdisError::RDBFileParserError {
                        msg: "Wrong Magic String".to_string(),
                    });
                }

                // RDB Version
                let mut slice = [0; 4];
                for i in 0..4 {
                    let byte = iter.next();
                    if byte.is_none() {
                        return Err(RusdisError::RDBFileParserError {
                            msg: "Invalid RDB Version".to_string(),
                        });
                    }
                    slice[i] = byte.unwrap();
                }
                rdb_version = String::from_utf8_lossy(&slice).to_string();
                phase = Phase::Metadata;
            }
            Phase::Metadata => {
                let flag = iter.peek();
                if flag.is_none() {
                    return Err(RusdisError::RDBFileParserError {
                        msg: "Invalid RDB file format".to_string(),
                    });
                }
                let flag = *flag.unwrap();

                match flag {
                    0xfa => {
                        // skip the FA flag
                        let _ = iter.next();
                        let phanton_iter = iter;
                        let (phanton_iter, key) = decode_string(phanton_iter)?;
                        let (phanton_iter, value) = decode_string(phanton_iter)?;
                        iter = phanton_iter;
                        aux_fields.push((key, value));
                    }
                    0xfe => phase = Phase::Database,
                    0xff => phase = Phase::CheckSum,
                    _ => {
                        return Err(RusdisError::RDBFileParserError {
                            msg: "Invalid RDB file format".to_string(),
                        })
                    }
                }
            }
            Phase::Database => {
                break;
            }
            _ => {}
        }
    }
    let rdb_file = RDBFile {
        rdb_version,
        aux_fields,
        datasets: vec![],
    };
    Ok(rdb_file)
}

fn decode_string(
    mut iter: Peekable<std::vec::IntoIter<u8>>,
) -> Result<(Peekable<std::vec::IntoIter<u8>>, String), RusdisError> {
    let first_byte = iter.peek();
    if first_byte.is_none() {
        return Err(RusdisError::RDBFileParserError {
            msg: "Decode string failed".to_string(),
        });
    }

    let first_byte = *first_byte.unwrap();
    if first_byte & 0b1100_0000 == 0b1100_0000 {
        let (iter, res) = decode_length(iter)?;
        let res = format!("{}", res);
        return Ok((iter, res));
    }

    let (mut iter, length) = decode_length(iter)?;

    let mut res = String::new();
    for _ in 0..length {
        let cur_byte = iter.next();
        if cur_byte.is_none() {
            return Err(RusdisError::RDBFileParserError {
                msg: "Decode string failed".to_string(),
            });
        }

        let cur_byte = cur_byte.unwrap();
        res.push(cur_byte as char);
    }

    Ok((iter, res))
}

fn decode_length(
    mut iter: Peekable<std::vec::IntoIter<u8>>,
) -> Result<(Peekable<std::vec::IntoIter<u8>>, usize), RusdisError> {
    let first_byte = iter.next();
    if first_byte.is_none() {
        return Err(RusdisError::RDBFileParserError {
            msg: "Decode length failed".to_string(),
        });
    }

    let first_byte = first_byte.unwrap();

    match first_byte & 0b1100_0000 {
        0b1100_0000 => {
            let next_six_bits = first_byte & 0b0011_1111;
            match next_six_bits {
                0 => {
                    let res = iter.next();
                    if res.is_none() {
                        return Err(RusdisError::RDBFileParserError {
                            msg: "Decode length failed".to_string(),
                        });
                    }

                    let res = res.unwrap() as usize;
                    Ok((iter, res))
                }
                1 => {
                    let mut res = 0;
                    for i in 0..2 {
                        let tmp = iter.next();
                        if tmp.is_none() {
                            return Err(RusdisError::RDBFileParserError {
                                msg: "Decode length failed".to_string(),
                            });
                        }
                        let mut tmp = tmp.unwrap() as usize;
                        tmp <<= i * 8;
                        res += tmp;
                    }

                    Ok((iter, res))
                }
                2 => {
                    let mut res = 0;
                    for i in 0..4 {
                        let tmp = iter.next();
                        if tmp.is_none() {
                            return Err(RusdisError::RDBFileParserError {
                                msg: "Decode length failed".to_string(),
                            });
                        }
                        let mut tmp = tmp.unwrap() as usize;
                        tmp <<= i * 8;
                        res += tmp;
                    }

                    Ok((iter, res))
                }
                _ => Err(RusdisError::RDBFileParserError {
                    msg: "LZF not covered".to_string(),
                }),
            }
        }
        0b1000_0000 => {
            let mut res = 0;
            for _ in 0..4 {
                let tmp = iter.next();
                if tmp.is_none() {
                    return Err(RusdisError::RDBFileParserError {
                        msg: "Decode length failed".to_string(),
                    });
                }
                let tmp = tmp.unwrap();

                res <<= 8;
                res += tmp as usize;
            }

            Ok((iter, res))
        }
        0b0100_0000 => {
            let next_six_bits = (first_byte & 0b0011_1111) as usize;
            let further_byte = iter.next();
            if further_byte.is_none() {
                return Err(RusdisError::RDBFileParserError {
                    msg: "Decode length failed".to_string(),
                });
            }

            let further_byte = further_byte.unwrap() as usize;
            Ok((iter, (next_six_bits << 8) + further_byte))
        }
        0b0000_0000 => {
            let next_six_bits = first_byte & 0b0011_1111;
            Ok((iter, next_six_bits as usize))
        }
        _ => Err(RusdisError::RDBFileParserError {
            msg: "Decode length failed".to_string(),
        }),
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_rdb_reader_decode_length() {
        //  start with 00
        let vec = vec![0x0a];
        let res = decode_length(vec.into_iter().peekable());
        assert!(res.is_ok());
        let (_, res) = res.unwrap();
        assert_eq!(res, 10);

        //  start with 01
        let vec = vec![0x42, 0xbc];
        let res = decode_length(vec.into_iter().peekable());
        assert!(res.is_ok());
        let (_, res) = res.unwrap();
        assert_eq!(res, 700);

        //  start with 10
        let vec = vec![0x80, 0x00, 0x00, 0x42, 0x68];
        let res = decode_length(vec.into_iter().peekable());
        assert!(res.is_ok());
        let (_, res) = res.unwrap();
        assert_eq!(res, 17000);

        //  start with 11 and remaining 6 bits are 0
        let vec = vec![0xc0, 0x7b];
        let res = decode_length(vec.into_iter().peekable());
        assert!(res.is_ok());
        let (_, res) = res.unwrap();
        assert_eq!(res, 123);

        //  start with 11 and remaining 6 bits are 1
        let vec = vec![0xc1, 0x39, 0x30];
        let res = decode_length(vec.into_iter().peekable());
        assert!(res.is_ok());
        let (_, res) = res.unwrap();
        assert_eq!(res, 12345);

        //  start with 11 and remaining 6 bits are 2
        let vec = vec![0xc2, 0x87, 0xd6, 0x12, 0x00];
        let res = decode_length(vec.into_iter().peekable());
        assert!(res.is_ok());
        let (_, res) = res.unwrap();
        assert_eq!(res, 1234567);
    }

    #[test]
    fn test_rdb_reader_decode_string() {
        let vec = vec![
            0x0d, 0x48, 0x65, 0x6c, 0x6c, 0x6f, 0x2c, 0x20, 0x57, 0x6f, 0x72, 0x6c, 0x64, 0x21,
        ];
        let res = decode_string(vec.into_iter().peekable());
        assert!(res.is_ok());
        let (_, res) = res.unwrap();
        assert_eq!(res.as_str(), "Hello, World!");

        let vec = vec![0xc2, 0x87, 0xd6, 0x12, 0x00];
        let res = decode_string(vec.into_iter().peekable());
        assert!(res.is_ok());
        let (_, res) = res.unwrap();
        assert_eq!(res.as_str(), "1234567");
    }
}
