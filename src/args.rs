use clap::{Parser, Subcommand};
use std::path::PathBuf;

use crate::config;

#[derive(Debug, Parser)]
#[command(name = "jmapsyncd", version, about = "JMAP to Maildir sync daemon")]
pub struct Args {
    #[arg(short = 'c', long = "config", env = "JMAPSYNCD_CONFIG")]
    pub config_file: Option<PathBuf>,

    #[arg(long, env = "JMAPSYNCD_LOG_LEVEL", value_enum)]
    pub log_level: Option<config::LogLevel>,

    #[command(flatten)]
    pub overrides: Overrides,

    #[arg(short = 'n', long)]
    pub dry_run: bool,

    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Debug, Default, Subcommand)]
pub enum Command {
    /// Run a one-shot sync and exit
    Sync {
        /// Account name to sync (omit to sync all)
        account: Option<String>,
    },
    /// Start the long-running daemon (default)
    #[default]
    Daemon,
}

#[derive(Debug, clap::Args)]
pub struct Overrides {
    #[arg(long, env = "JMAPSYNCD_DB_DIR")]
    db_dir: Option<PathBuf>,
}

impl From<Overrides> for config::Overrides {
    fn from(args: Overrides) -> Self {
        config::Overrides {
            db_dir: args.db_dir,
        }
    }
}
