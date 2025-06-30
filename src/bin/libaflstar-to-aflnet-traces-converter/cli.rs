//! The command line interface of the fuzzer

use std::path::PathBuf;

use clap::Parser;

#[derive(Debug, Parser)]
#[command(about = "AFLnet Replayer")]
pub struct Cli {
    #[arg(
        help = "The directory holding the replay_traces inputs that need to be converted.",
        short = 'i',
        long = "in-dir",
        required = true
    )]
    pub in_dir: PathBuf,

    #[arg(
        help = "The directory to store the new traces in.",
        short = 'o',
        long = "out-dir",
        required = true
    )]
    pub out_dir: PathBuf,
}
