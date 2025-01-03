use clap::Parser;

/// Rusdis
#[derive(Parser, Debug)]
pub struct Args {
    /// Path to the directory where the RDB file is stored
    #[arg(long)]
    pub dir: Option<String>,

    /// Name of the RDB file
    #[arg(long)]
    pub dbfilename: Option<String>,

    /// Port number to listen to
    #[arg(long)]
    pub port: Option<String>,

    /// Master to listen to
    #[arg(long)]
    pub replicaof: Option<String>,
}

impl Args {
    pub fn new() -> Self {
        Args {
            dir: None,
            dbfilename: None,
            port: None,
            replicaof: None,
        }
    }
}
