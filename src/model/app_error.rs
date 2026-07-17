use crate::prelude::*;

#[derive(Debug)]
pub(crate) enum AppError {
    Io(io::Error),
    Tool(String),
    Usage(String),
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AppError::Io(err) => write!(f, "{err}"),
            AppError::Tool(message) | AppError::Usage(message) => write!(f, "{message}"),
        }
    }
}

impl From<io::Error> for AppError {
    fn from(value: io::Error) -> Self {
        AppError::Io(value)
    }
}
