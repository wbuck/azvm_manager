use std::fmt::{self, Formatter, Display};

#[derive(Debug, Clone)]
pub enum AppError {
    NoSub,
    NoRg,
    NoVault,
    MissingLocationHeader
}

impl std::error::Error for AppError {}

impl Display for AppError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            AppError::NoSub => write!(f, "No subscription specified"),
            AppError::NoRg => write!(f, "No resource group specified"),
            AppError::NoVault => write!(f, "No vault name specified"),
            AppError::MissingLocationHeader => write!(f, "The response is missing a location header")
        }
    }
}