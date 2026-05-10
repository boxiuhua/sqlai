use thiserror::Error;

#[derive(Debug, Error)]
pub enum SkillError {
    #[error("missing required argument: {0}")]
    MissingArg(&'static str),

    #[error("invalid argument {0}: {1}")]
    InvalidArg(&'static str, String),

    #[error("render error: {0}")]
    Render(String),
}
