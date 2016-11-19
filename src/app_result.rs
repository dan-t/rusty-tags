use std::io;
use std::convert::From;
use std::fmt::{self, Display, Formatter};
use glob;
use toml;
use types::SourceKind;

/// The result used in the whole application.
pub type AppResult<T> = Result<T, AppErr>;

/// The generic error used in the whole application.
#[derive(Clone)]
pub enum AppErr {
    /// generic error message
    Message(String),

    /// source code couldn't be found
    MissingSource(SourceKind)
}

impl Display for AppErr {
    fn fmt(&self, f: &mut Formatter) -> Result<(), fmt::Error> {
        match self {
            &AppErr::Message(ref msg)            => writeln!(f, "{}", msg),
            &AppErr::MissingSource(ref src_kind) => writeln!(f, "{}", src_kind)
        }
    }
}

impl From<io::Error> for AppErr {
    fn from(err: io::Error) -> AppErr {
        AppErr::Message(format!("{}", err))
    }
}

impl From<glob::PatternError> for AppErr {
    fn from(err: glob::PatternError) -> AppErr {
        AppErr::Message(format!("{}", err))
    }
}

impl From<toml::DecodeError> for AppErr {
    fn from(err: toml::DecodeError) -> AppErr {
        AppErr::Message(format!("{}", err))
    }
}

impl From<String> for AppErr {
    fn from(s: String) -> AppErr {
        AppErr::Message(s)
    }
}

impl<'a> From<&'a str> for AppErr {
    fn from(s: &str) -> AppErr {
        AppErr::Message(s.to_owned())
    }
}

impl<'a> From<&'a SourceKind> for AppErr {
    fn from(s: &SourceKind) -> AppErr {
        AppErr::MissingSource(s.clone())
    }
}
