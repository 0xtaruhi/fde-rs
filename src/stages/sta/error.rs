use thiserror::Error;

#[derive(Debug, Error)]
pub enum StaError {
    #[error("constrained STA currently supports one synchronous clock domain, found {count}")]
    MultipleClockDomains { count: usize },
    #[error("clock '{clock}' references unknown design port '{port}'")]
    UnknownClockPort { clock: String, port: String },
    #[error("clock '{clock}' must target an input port, but '{port}' is not input-like")]
    InvalidClockPort { clock: String, port: String },
    #[error("clock '{clock}' on port '{port}' does not drive any sequential cell")]
    UnusedClock { clock: String, port: String },
    #[error("sequential cell '{cell}' is not driven by the constrained clock domain")]
    UnconstrainedSequentialCell { cell: String },
    #[error("constrained STA does not yet support {kind} cell '{cell}'")]
    UnsupportedSequentialCell { cell: String, kind: String },
    #[error("timing analysis produced a non-finite arrival for {key}: {value}")]
    NonFiniteArrival { key: String, value: f64 },
    #[error("timing analysis produced a non-finite critical path: {value}")]
    NonFiniteCriticalPath { value: f64 },
    #[error("timing analysis produced a non-finite Fmax: {value}")]
    NonFiniteFmax { value: f64 },
}
