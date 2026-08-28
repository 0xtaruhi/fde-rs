mod api;
mod arrival;
mod constraints;
mod delay;
mod error;
mod graph;
mod keys;
mod names;
mod report;
#[cfg(test)]
mod tests;

pub use api::{
    StaArtifact, StaOptions, StaTimingContext, run, run_with_reporter, run_with_timing,
    run_with_timing_and_reporter,
};
pub use error::StaError;
