use std::fmt::{self, Formatter, Display};

#[derive(Debug, Clone)]
pub enum AppError {
    NoSub,
    NoRg,
}

impl std::error::Error for AppError {}

impl Display for AppError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            AppError::NoSub => write!(f, "No subscription specified"),
            AppError::NoRg => write!(f, "No resource group specified"),
        }
    }
}