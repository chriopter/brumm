//! One error type for brumm: what Apple Music says about the session, and
//! everything else as the message a player shows.

use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// Apple rejected the session; the user must sign in again.
    Unauthorized,
    /// A listing stopped after some pages: what came is usable, but it is
    /// not the whole list.
    Partial,
    Msg(String),
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Unauthorized => f.write_str("apple music: not signed in"),
            Error::Partial => f.write_str("apple music: listing incomplete"),
            Error::Msg(s) => f.write_str(s),
        }
    }
}

impl std::error::Error for Error {}

/// An error with this message.
pub fn err(s: impl Into<String>) -> Error {
    Error::Msg(s.into())
}

impl From<String> for Error {
    fn from(s: String) -> Self {
        Error::Msg(s)
    }
}

impl From<&str> for Error {
    fn from(s: &str) -> Self {
        Error::Msg(s.to_string())
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Msg(e.to_string())
    }
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Error::Msg(e.to_string())
    }
}

impl From<reqwest::Error> for Error {
    fn from(e: reqwest::Error) -> Self {
        // Without the request's address: it can carry a query.
        Error::Msg(e.without_url().to_string())
    }
}
