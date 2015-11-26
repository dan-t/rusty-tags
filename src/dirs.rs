use std::fs;
use std::env;
use std::path::PathBuf;
use std::process::Command;
use glob::{glob, Paths};

use app_result::{AppResult, app_err_msg};
use path_ext::PathExt;

/// where `rusty-tags` caches its tag files
pub fn rusty_tags_cache_dir() -> AppResult<PathBuf>
{
   let dir = try!(
      rusty_tags_dir().map(|mut d| {
         d.push("cache");
         d
      })
   );

   if ! dir.is_dir() {
      try!(fs::create_dir_all(&dir));
   }

   Ok(dir)
}

/// where rusty-tags puts all of its stuff
pub fn rusty_tags_dir() -> AppResult<PathBuf>
{
   let dir = try!(
      homedir().map(|mut d| {
         d.push(".rusty-tags");
         d 
      })
   );

   if ! dir.is_dir() {
      try!(fs::create_dir_all(&dir));
   }

   Ok(dir)
}

/// where cargo puts its git checkouts
pub fn cargo_git_src_dir() -> AppResult<PathBuf>
{
   cargo_dir().map(|mut d| {
      d.push("git");
      d.push("checkouts");
      d
   })
}

/// where cargo puts the source code of crates.io
pub fn cargo_crates_io_src_dir() -> AppResult<PathBuf>
{
   let src_dir = try!(
      cargo_dir().map(|mut d| {
         d.push("registry");
         d.push("src");
         d.push("github.com-*");
         d
      })
   );

   let src_str = format!("{}", src_dir.display());
   let mut paths = try!(glob_path(&src_str));
   if let Some(Ok(path)) = paths.nth(0) {
      Ok(path)
   }
   else {
      Err(app_err_msg(format!("Expected one matching path for '{}'!", src_str)))
   }
}

/// where cargo puts all of its stuff
fn cargo_dir() -> AppResult<PathBuf>
{
   if let Ok(out) = Command::new("multirust").arg("show-override").output() {
      let output = try!(
         String::from_utf8(out.stdout)
            .map_err(|_| app_err_msg("Couldn't convert git output to utf8!".to_string()))
      );

      for line in output.lines() {
         let strs: Vec<&str> = line.split(" location: ").collect();
         if strs.len() == 2 {
            let mut path = PathBuf::new();
            path.push(strs[1]);
            path.push("cargo");
            if path.is_dir() {
               return Ok(path);
            }
         }
      }

      return Err(app_err_msg(format!("Couldn't get multirust cargo location from output:\n{}", output)));
   }

   homedir().map(|mut d| { d.push(".cargo"); d })
}

pub fn glob_path(pattern: &String) -> AppResult<Paths>
{
   Ok(try!(glob(&pattern)))
}

fn homedir() -> AppResult<PathBuf>
{
   if let Some(path) = env::home_dir() {
      Ok(path)
   }
   else {
      Err(app_err_msg("Couldn't read home directory!".to_string()))
   }
}
