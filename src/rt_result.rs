use std::io;
use std::convert::From;
use std::fmt::{self, Display, Formatter};
use rustc_serialize::json;
use toml;

/// The result used in the whole application.
pub type RtResult<T> = Result<T, RtErr>;

/// The generic error used in the whole application.
#[derive(Clone)]
pub enum RtErr {
    /// generic error message
    Message(String),
}

impl Display for RtErr {
    fn fmt(&self, f: &mut Formatter) -> Result<(), fmt::Error> {
        match self {
            &RtErr::Message(ref msg) => writeln!(f, "{}", msg),
        }
    }
}

impl From<io::Error> for RtErr {
    fn from(err: io::Error) -> RtErr {
        RtErr::Message(format!("{}", err))
    }
}

impl From<toml::DecodeError> for RtErr {
    fn from(err: toml::DecodeError) -> RtErr {
        RtErr::Message(format!("{}", err))
    }
}

impl From<json::ParserError> for RtErr {
    fn from(err: json::ParserError) -> RtErr {
        RtErr::Message(format!("{}", err))
    }
}

impl From<String> for RtErr {
    fn from(s: String) -> RtErr {
        RtErr::Message(s)
    }
}

impl<'a> From<&'a str> for RtErr {
    fn from(s: &str) -> RtErr {
        RtErr::Message(s.to_owned())
    }
}
