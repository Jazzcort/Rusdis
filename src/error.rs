use thiserror::Error;

#[derive(Error, Debug)]
pub enum RusdisError {
    #[error("Parser Error")]
    ParserError(#[from] crate::ParserError),
    #[error("Invalid Command")]
    InvalidCommand,
    #[error("IO errors")]
    IO(#[from] std::io::Error),
}
