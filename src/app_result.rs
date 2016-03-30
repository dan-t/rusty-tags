use std::io;
use std::convert::From;
use std::fmt::{self, Display, Formatter};
use glob;
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

pub fn app_err_msg(msg: String) -> AppErr {
    AppErr::Message(msg)
}

pub fn app_err_missing_src(src_kind: &SourceKind) -> AppErr {
    AppErr::MissingSource(src_kind.clone())
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
