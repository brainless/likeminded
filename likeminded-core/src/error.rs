use std::fmt;

#[derive(Debug)]
pub enum CoreError {
    Configuration(String),
    InvalidInput(String),
}

impl fmt::Display for CoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CoreError::Configuration(msg) => write!(f, "Configuration error: {}", msg),
            CoreError::InvalidInput(msg) => write!(f, "Invalid input: {}", msg),
        }
    }
}

impl std::error::Error for CoreError {}
