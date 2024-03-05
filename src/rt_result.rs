use std::io;
use std::fmt::{self, Display, Formatter};

use semver::{ReqParseError, SemVerError};

/// The result used in the whole application.
pub type RtResult<T> = Result<T, RtErr>;

/// The generic error used in the whole application.
#[derive(Clone, Debug)]
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

impl From<toml::de::Error> for RtErr {
    fn from(err: toml::de::Error) -> RtErr {
        RtErr::Message(err.to_string())
    }
}

impl From<serde_json::Error> for RtErr {
    fn from(err: serde_json::Error) -> RtErr {
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

impl From<ReqParseError> for RtErr {
    fn from(_: ReqParseError) -> RtErr {
        RtErr::Message("Invalid version requirement".to_owned())
    }
}

impl From<SemVerError> for RtErr {
    fn from(err: SemVerError) -> RtErr {
        match err {
            SemVerError::ParseError(err) => RtErr::Message(err)
        }
    }
}
