use thiserror::Error;

#[derive(Error, Debug)]
pub enum RusdisError {
    #[error("Parser Error")]
    ParserError(#[from] crate::ParserError),
    #[error("Invalid Command")]
    InvalidCommand,
    #[error("IO error")]
    IO(#[from] std::io::Error),
    #[error("Command Parser Error: {msg}")]
    CommandParserError { msg: String },
    #[error("Parse int errors")]
    ParseIntError(#[from] std::num::ParseIntError),
    #[error("Instant addition error")]
    InstantAdditionError,
}
