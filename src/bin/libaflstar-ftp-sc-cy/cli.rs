//! The command line interface of the fuzzer

use std::{error::Error, path::PathBuf};

use clap::Parser;
use nix::sys::signal::Signal;

#[derive(Debug, Parser)]
#[command(
    about = "Single corpus and single metadata. State scheduler = Cycler"
)]
pub struct Cli {
    #[arg(
        help = "The instrumented binary we want to fuzz",
        name = "EXEC",
        required = true
    )]
    pub executable: String,

    #[arg(
        help = "Arguments passed to the target",
        name = "arguments",
        num_args(1..),
        allow_hyphen_values = true,
    )]
    pub arguments: Vec<String>,

    #[arg(
        help = "The directory to read initial inputs from ('seeds')",
        short = 'i',
        long = "in-dir",
        required = true
    )]
    pub in_dir: PathBuf,

    #[arg(
        help = "The directory to store all outputs in",
        short = 'o',
        long = "out-dir",
        required = true
    )]
    pub out_dir: PathBuf,

    #[arg(
        help = "Timeout for each individual execution, in milliseconds",
        short = 't',
        long = "timeout",
        default_value = "1200"
    )]
    pub timeout: u64,

    #[arg(
        help = "Number of test cases that are tried before a new target state is again selected",
        short = 'l',
        long = "loops",
        default_value = "100"
    )]
    pub loops: usize,

    #[arg(
        help = "If not set, the child's stdout and stderror will be redirected to /dev/null",
        short = 'd',
        long = "debug-child",
        default_value = "false"
    )]
    pub debug_child: bool,

    #[arg(
        help = "Environment variables passed to the target",
        short = 'e',
        long = "target-env",
        value_parser = parse_key_val_pairs::<String, String>,
    )]
    pub environment_variables: Option<std::vec::Vec<(String, String)>>,

    #[arg(
        help = "Port the target uses",
        short = 'p',
        long = "target-port",
        required = true
    )]
    pub target_port: u16,

    #[arg(
        help = "Signal used to stop child",
        short = 's',
        long = "signal",
        value_parser = str::parse::<Signal>,
        default_value = "SIGKILL"
    )]
    pub signal: Signal,
}

/// Parse a list of key-value pairs
fn parse_key_val_pairs<T, U>(
    strs: &str,
) -> Result<Vec<(T, U)>, Box<dyn Error + Send + Sync + 'static>>
where
    T: std::str::FromStr,
    T::Err: Error + Send + Sync + 'static,
    U: std::str::FromStr,
    U::Err: Error + Send + Sync + 'static,
{
    let mut v = Vec::new();
    for s in strs.split(',') {
        v.push(parse_key_val(s)?);
    }
    Ok(v)
}
/// Parse a single key-value pair
fn parse_key_val<T, U>(s: &str) -> Result<(T, U), Box<dyn Error + Send + Sync + 'static>>
where
    T: std::str::FromStr,
    T::Err: Error + Send + Sync + 'static,
    U: std::str::FromStr,
    U::Err: Error + Send + Sync + 'static,
{
    let pos = s
        .find('=')
        .ok_or_else(|| format!("invalid KEY=value: no `=` found in `{s}`"))?;
    Ok((s[..pos].parse()?, s[pos + 1..].parse()?))
}
