use thiserror::Error;

#[derive(Debug, Error)]
pub enum StaError {
    #[error("timing analysis produced a non-finite arrival for {key}: {value}")]
    NonFiniteArrival { key: String, value: f64 },
    #[error("timing analysis produced a non-finite critical path: {value}")]
    NonFiniteCriticalPath { value: f64 },
    #[error("timing analysis produced a non-finite Fmax: {value}")]
    NonFiniteFmax { value: f64 },
}
