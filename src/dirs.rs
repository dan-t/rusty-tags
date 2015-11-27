use std::fs;
use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;
use glob::{glob, Paths};

use app_result::{AppResult, app_err_msg};
use path_ext::PathExt;

lazy_static! {
   static ref HOME_DIR               : AppResult<PathBuf> = home_dir_internal();
   static ref RUSTY_TAGS_DIR         : AppResult<PathBuf> = rusty_tags_dir_internal();
   static ref RUSTY_TAGS_CACHE_DIR   : AppResult<PathBuf> = rusty_tags_cache_dir_internal();
   static ref CARGO_DIR              : AppResult<PathBuf> = cargo_dir_internal();
   static ref CARGO_GIT_SRC_DIR      : AppResult<PathBuf> = cargo_git_src_dir_internal();
   static ref CARGO_CRATES_IO_SRC_DIR: AppResult<PathBuf> = cargo_crates_io_src_dir_internal();
}

/// where rusty-tags puts all of its stuff
pub fn rusty_tags_dir() -> AppResult<&'static Path>
{
   RUSTY_TAGS_DIR
      .as_ref()
      .map(|pb| pb.as_path())
      .map_err(|err| err.clone())
}

/// where `rusty-tags` caches its tag files
pub fn rusty_tags_cache_dir() -> AppResult<&'static Path>
{
   RUSTY_TAGS_CACHE_DIR
      .as_ref()
      .map(|pb| pb.as_path())
      .map_err(|err| err.clone())
}

/// where cargo puts its git checkouts
pub fn cargo_git_src_dir() -> AppResult<&'static Path>
{
   CARGO_GIT_SRC_DIR
      .as_ref()
      .map(|pb| pb.as_path())
      .map_err(|err| err.clone())
}

/// where cargo puts the source code of crates.io
pub fn cargo_crates_io_src_dir() -> AppResult<&'static Path>
{
   CARGO_CRATES_IO_SRC_DIR
      .as_ref()
      .map(|pb| pb.as_path())
      .map_err(|err| err.clone())
}

pub fn glob_path(pattern: &String) -> AppResult<Paths>
{
   Ok(try!(glob(&pattern)))
}

fn home_dir() -> AppResult<PathBuf>
{
   HOME_DIR.clone()
}

fn cargo_dir() -> AppResult<PathBuf>
{
   CARGO_DIR.clone()
}

fn home_dir_internal() -> AppResult<PathBuf>
{
   if let Some(path) = env::home_dir() {
      Ok(path)
   }
   else {
      Err(app_err_msg("Couldn't read home directory!".to_string()))
   }
}

fn rusty_tags_cache_dir_internal() -> AppResult<PathBuf>
{
   let dir = try!(
      rusty_tags_dir()
         .map(Path::to_path_buf)
         .map(|mut d| {
            d.push("cache");
            d
         })
   );

   if ! dir.is_dir() {
      try!(fs::create_dir_all(&dir));
   }

   Ok(dir)
}

fn rusty_tags_dir_internal() -> AppResult<PathBuf>
{
   let dir = try!(
      home_dir().map(|mut d| {
         d.push(".rusty-tags");
         d 
      })
   );

   if ! dir.is_dir() {
      try!(fs::create_dir_all(&dir));
   }

   Ok(dir)
}

fn cargo_git_src_dir_internal() -> AppResult<PathBuf>
{
   cargo_dir().map(|mut d| {
      d.push("git");
      d.push("checkouts");
      d
   })
}

fn cargo_crates_io_src_dir_internal() -> AppResult<PathBuf>
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

fn cargo_dir_internal() -> AppResult<PathBuf>
{
   if let Ok(out) = Command::new("multirust").arg("show-override").output() {
      let output = try!(
         String::from_utf8(out.stdout)
            .map_err(|_| app_err_msg("Couldn't convert 'multirust show-override' output to utf8!".to_string()))
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

   home_dir().map(|mut d| { d.push(".cargo"); d })
}
