//! Functionality that helps store and retrace what happened. Which messages were sent, in what order.
//! 
//! But, there is nothing smart about it.

use std::{
    fs::{File, OpenOptions},
    io::BufWriter,
    path::{Path, PathBuf},
};

use libafl::{executors::ExitKind, Error};
use serde::{Deserialize, Serialize};

/// Request response pair that just handles bytes (u8) which can be serialized.
#[derive(Serialize, Deserialize, Debug)]
pub struct RequestResponsePair<'a> {
    // exit kind
    ek: String,
    // request
    req: &'a [u8],
    // response
    resp: &'a [u8],
}

impl<'a> RequestResponsePair<'a> {
    pub fn new(exit_kind: ExitKind, request: &'a [u8], response: &'a [u8]) -> Self {
        let ek = match exit_kind {
            ExitKind::Ok => "Ok",
            ExitKind::Crash => "Cr",
            ExitKind::Oom => "Oo",
            ExitKind::Timeout => "Tm",
            ExitKind::Diff {
                primary: _,
                secondary: _,
            } => "Diff",
        };
        Self {
            ek: ek.to_string(),
            req: request,
            resp: response,
        }
    }
}

/// Struct that helps to write request-response pairs from the target to file, collecting them per trace.
/// This way, all pairs that belong to a single trace are stored together, in order.
#[derive(Debug)]
pub struct RequestResponseCollector {
    /// Directory to save the traces to
    traces_dir: PathBuf,
    // current open trace file
    writer: BufWriter<File>,
    /// the number of the trace we are currently collecting
    trace_no: usize,
}

impl RequestResponseCollector {
    /// Creates a new [`RequestResponseCollector`].
    ///
    /// If any of the trace files that will be created already exists, they will be overwritten.
    ///
    /// # Parameters
    ///
    /// - `path`: Path to the directory where the traces will be stored.
    ///           
    /// # Errors:
    ///
    /// - Any IO errors
    /// - Path exists but is not a directory
    ///
    pub fn new(path: &Path) -> Result<Self, Error>
where {
        // make sure the directory exists.
        match path.exists() {
            true if path.is_dir() => {}
            true if !path.is_dir() => {
                return Err(Error::illegal_argument(
                    "Path to replay dir is a file that already exists.",
                ))
            }
            false => std::fs::create_dir(path)?,
            _ => unreachable!("All match arms are covered"),
        }

        let trace_no = 0;
        let file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(path.join(Self::get_filename(trace_no)))?;

        let writer = BufWriter::new(file);

        Ok(Self {
            traces_dir: path.to_path_buf(),
            writer,
            trace_no,
        })
    }

    /// Write the request response pair to the current trace, i.e., the open file, serializing it to CBOR.
    pub fn write_pair(&mut self, pair: &RequestResponsePair) -> Result<(), Error> {
        // todo remove unwrap and instead bubble up error
        ciborium::into_writer(pair, &mut self.writer).unwrap();

        Ok(())
    }

    /// Save the trace.
    /// In actuality, the next time a new trace is started, the current file isn't overwritten
    /// and therefore saved
    pub fn save_this_trace(&mut self) {
        self.trace_no = self.trace_no + 1;
    }

    /// Start a new trace. If the trace number has no been changed,
    /// the trace file is overwritten
    pub fn start_new_trace(&mut self) -> Result<(), Error> {
        let new_file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(self.traces_dir.join(Self::get_filename(self.trace_no)))?;
        self.writer = BufWriter::new(new_file);
        Ok(())
    }

    fn get_filename(trace_no: usize) -> String {
        format!("trace_{trace_no}.cbor")
    }
}
