use thiserror::Error;

#[derive(Error, Debug)]
pub enum TokenError {
    #[error("api returns unexpected format: {0}")] UnexpectedFormat(String),
}
