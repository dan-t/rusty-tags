use std::io;
use std::old_io;
use std::error::FromError;
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

impl FromError<io::Error> for AppErr
{
   fn from_error(err: io::Error) -> AppErr
   {
      AppErr { error: format!("{}", err) }
   }
}

impl FromError<old_io::IoError> for AppErr
{
   fn from_error(err: old_io::IoError) -> AppErr
   {
      AppErr { error: format!("{}", err) }
   }
}

impl FromError<glob::PatternError> for AppErr
{
   fn from_error(err: glob::PatternError) -> AppErr
   {
      AppErr { error: format!("{}", err) }
   }
}
