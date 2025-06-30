mod cli;

use std::{
    fs::{File, OpenOptions},
    io::{BufReader, Write},
};

use clap::Parser;
use libafl::Error;
use serde::{Deserialize, Serialize};

/// Request response pair that just handles bytes (u8) which can be serialized.
#[derive(Serialize, Deserialize, Debug)]
pub struct RequestResponsePair {
    // execution number of this request (test case)
    ek: String,
    // request
    req: Vec<u8>,
    // response
    resp: Vec<u8>,
}

fn main() -> Result<(), Error> {
    let cli = cli::Cli::parse();

    // check if input dir exists
    let in_dir = cli.in_dir;
    if !in_dir.exists() {
        return Err(Error::illegal_argument(format!(
            "IN_DIR [{}] does not exist",
            in_dir.display()
        )));
    }

    // make output dir, exist if it already exists
    let out_dir = cli.out_dir;
    if out_dir.exists() {
        if out_dir.read_dir()?.next().is_some() {
            return Err(Error::illegal_argument(format!(
                "OUT_DIR [{}] must be empty or not exist.",
                out_dir.display()
            )));
        }
    } else {
        std::fs::create_dir(&out_dir)?;
    }

    // iterate over the files
    for file in in_dir.read_dir()? {
        let file = file?;
        if file.path().is_dir() || file.file_name() == "trace_0.cbor" {
            continue;
        }
        let file_name = file.file_name().to_owned();
        let mut out_file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(out_dir.join(file_name))?;

        let mut reader = BufReader::new(File::open(file.path())?);
        //      parse cbor file
        loop {
            let pair: RequestResponsePair = match ciborium::from_reader(&mut reader) {
                Ok(a) => a,
                Err(_) => {
                    break;
                }
            };
            let request = pair.req;

            //      write <len><bytes> to file
            let len = request.len() as u32;
            out_file.write(len.to_le_bytes().as_slice())?;
            out_file.write(request.as_slice())?;
        }
    }

    Ok(())
}
