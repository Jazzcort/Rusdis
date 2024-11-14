use crate::error::RusdisError;
use std::fs::File;
use std::io::prelude::*;
use std::io::BufReader;
use std::time::Instant;

pub enum Phase {
    MagicString,
    RDBVersion,
    AuxiliaryField,
    DBSelector,
    ResizedbField,
    KeyValue,
    CheckSum,
}

pub struct RDBFile {
    pub rdb_version: String,
    pub redis_bits: usize,
    pub redis_ver: usize,
    pub datasets: Vec<Dataset>,
}

pub struct Dataset {
    pairs: Vec<(String, ValueType)>,
    expirations: Vec<Instant>,
}

pub enum ValueType {
    String(String),
}

pub fn read_rdb(f_path: String) -> Result<(), RusdisError> {
    let f = File::open(f_path)?;
    let mut reader = BufReader::new(f);

    let mut buf = vec![];

    let length = reader.read_to_end(&mut buf)?;
    //let mut line = String::new();
    dbg!(&buf);
    dbg!(String::from_utf8_lossy(&buf));

    //while reader.read_line(&mut line)? > 0 {
    //    dbg!(&line);
    //}

    Ok(())
}
