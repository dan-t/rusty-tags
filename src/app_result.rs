use std::io;
use std::convert::From;
use std::fmt::{self, Display, Formatter};
use glob;

/// The result used in the whole application.
pub type AppResult<T> = Result<T, AppErr>;

/// The generic error used in the whole application.
pub struct AppErr
{
   error: String
}

pub fn app_err(string: String) -> AppErr
{
   AppErr::from_string(string)
}

impl AppErr
{
   pub fn from_string(string: String) -> AppErr
   {
      AppErr { error: string }
   }
}

impl Display for AppErr
{
   fn fmt(&self, f: &mut Formatter) -> Result<(), fmt::Error>
   {
      writeln!(f, "{}", self.error)
   }
}

impl From<io::Error> for AppErr
{
   fn from(err: io::Error) -> AppErr
   {
      AppErr { error: format!("{}", err) }
   }
}

impl From<glob::PatternError> for AppErr
{
   fn from(err: glob::PatternError) -> AppErr
   {
      AppErr { error: format!("{}", err) }
   }
}
