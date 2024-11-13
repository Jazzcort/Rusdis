use crate::error::RusdisError;
use std::fs::File;
use std::io::prelude::*;
use std::io::BufReader;

pub fn read_rdb(f_path: String) -> Result<(), RusdisError> {
    let f = File::open(f_path)?;
    let mut reader = BufReader::new(f);

    //let mut line = String::new();
    let mut buf: [u8] = [0; 512];
    reader.read(&mut buf);
    dbg!(buf);

    //while reader.read_line(&mut line)? > 0 {
    //    dbg!(&line);
    //}

    Ok(())
}
