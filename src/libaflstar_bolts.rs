//! Named after LibAFL Bolts
//!
//! Stuff and functions that are handy but not directly related to the fuzzers

use std::io::ErrorKind;

use libafl::Error;
use libafl_bolts::ErrorBacktrace;

pub fn create_timeout_error(msg: impl Into<String>) -> Error {
    Error::File(
        std::io::Error::new(ErrorKind::TimedOut, msg.into()),
        ErrorBacktrace::new(),
    )
}
