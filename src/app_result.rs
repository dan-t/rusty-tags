use std::io::IoError;
use std::error::FromError;
use std::fmt::{Show, Formatter, Error};

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

impl Show for AppErr
{
   fn fmt(&self, f: &mut Formatter) -> Result<(), Error>
   {
      writeln!(f, "{}", self.error)
   }
}

impl FromError<IoError> for AppErr
{
   fn from_error(err: IoError) -> AppErr
   {
      AppErr { error: format!("{}", err) }
   }
}
