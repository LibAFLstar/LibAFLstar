//! LibAFLstar, an extension of LibAFL to fuzz stateful targets, primarily via sockets.

pub mod event_manager;
pub mod executor;
pub mod fuzzer;
pub mod mutator;
pub mod http_mutator;
pub mod rtsp_mutator;
pub mod replay;
pub mod state;
pub mod state_scheduler;

pub mod perf;

mod libaflstar_bolts;
