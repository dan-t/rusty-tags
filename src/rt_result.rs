use std::io;
use std::convert::From;
use std::fmt::{self, Display, Formatter};
use glob;
use toml;
use types::SourceKind;

/// The result used in the whole application.
pub type RtResult<T> = Result<T, RtErr>;

/// The generic error used in the whole application.
#[derive(Clone)]
pub enum RtErr {
    /// generic error message
    Message(String),

    /// source code couldn't be found
    MissingSource(SourceKind)
}

impl Display for RtErr {
    fn fmt(&self, f: &mut Formatter) -> Result<(), fmt::Error> {
        match self {
            &RtErr::Message(ref msg)            => writeln!(f, "{}", msg),
            &RtErr::MissingSource(ref src_kind) => writeln!(f, "Missing source of '{}'", src_kind)
        }
    }
}

impl From<io::Error> for RtErr {
    fn from(err: io::Error) -> RtErr {
        RtErr::Message(format!("{}", err))
    }
}

impl From<glob::PatternError> for RtErr {
    fn from(err: glob::PatternError) -> RtErr {
        RtErr::Message(format!("{}", err))
    }
}

impl From<toml::DecodeError> for RtErr {
    fn from(err: toml::DecodeError) -> RtErr {
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

impl<'a> From<&'a SourceKind> for RtErr {
    fn from(s: &SourceKind) -> RtErr {
        RtErr::MissingSource(s.clone())
    }
}
