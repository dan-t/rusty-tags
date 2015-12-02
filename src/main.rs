// to silence a bogus warning about `tag_dir` being unused
#![allow(unused_assignments)]

extern crate toml;
extern crate glob;

#[macro_use]
extern crate clap;

#[macro_use]
extern crate lazy_static;

use std::fs;
use std::env;
use std::path::{PathBuf, Path};
use std::io::{self, Write};
use std::process::Command;

use app_result::{AppResult, AppErr, app_err_msg};
use dependencies::read_dependencies;
use types::{TagsRoot, TagsKind};
use path_ext::PathExt;

use tags::{
   update_tags,
   update_tags_and_check_for_reexports,
   create_tags,
   merge_tags
};

use dirs::rusty_tags_dir;
use config::{get_config_from_args, Config};

mod app_result;
mod dependencies;
mod dirs;
mod tags;
mod types;
mod path_ext;
mod config;

fn main() 
{
   let config = get_config_from_args();

   update_all_tags(&config).unwrap_or_else(|err| {
      writeln!(&mut io::stderr(), "{}", err).unwrap();
      std::process::exit(1);
   });
}

fn update_all_tags(config: &Config) -> AppResult<()>
{
   try!(fetch_source_of_dependencies());

   let cwd = try!(env::current_dir());
   let cargo_dir = try!(find_cargo_toml_dir(&cwd));
   let tags_roots = try!(read_dependencies(&cargo_dir));

   let rust_std_lib_tags_file = try!(rust_std_lib_tags_file(&config.tags_kind));
   let mut missing_sources = Vec::new();

   for tags_root in tags_roots.iter() {
      let mut tag_files: Vec<PathBuf> = Vec::new();
      let mut tag_dir: Option<PathBuf> = None;

      match *tags_root {
         TagsRoot::Src { ref src_dir, ref dependencies } => {
            let mut src_tags = src_dir.clone();
            src_tags.push(config.tags_kind.tags_file_name());
            try!(create_tags(config, src_dir, &src_tags));
            tag_files.push(src_tags);

            for dep in dependencies.iter() {
               match update_tags(config, dep) {
                  Ok(tags) => tag_files.push(tags.tags_file),
                  Err(app_err) => {
                     match app_err {
                        AppErr::MissingSource(src_kind) => missing_sources.push(src_kind),
                        _ => return Err(app_err)
                     }
                  }
               }
            }

            tag_dir = Some(src_dir.clone());
         },

         TagsRoot::Lib { ref src_kind, ref dependencies } => {
            let lib_tags = match update_tags_and_check_for_reexports(config, src_kind, dependencies) {
               Ok(tags) => {
                 if tags.is_up_to_date(&config.tags_kind) && ! config.force_recreate {
                    continue;
                 }
                 else {
                    tags
                 }
               }

               Err(app_err) => {
                  match app_err {
                     AppErr::MissingSource(src_kind) => {
                        missing_sources.push(src_kind);
                        continue;
                     }

                     _ => return Err(app_err)
                  }
               }
            };

            tag_files.push(lib_tags.tags_file);

            for dep in dependencies.iter() {
               match update_tags(config, dep) {
                  Ok(tags) => tag_files.push(tags.tags_file),
                  Err(app_err) => {
                     match app_err {
                        AppErr::MissingSource(src_kind) => missing_sources.push(src_kind),
                        _ => return Err(app_err)
                     }
                  }
               }
            }

            tag_dir = Some(lib_tags.src_dir.clone());
         }
      }

      if tag_files.is_empty() || tag_dir.is_none() {
         continue;
      }

      if rust_std_lib_tags_file.as_path().is_file() {
         tag_files.push(rust_std_lib_tags_file.clone());
      }

      let mut tags_file = tag_dir.unwrap();
      tags_file.push(config.tags_kind.tags_file_name());

      try!(merge_tags(config, &tag_files, &tags_file));
   }

   if ! missing_sources.is_empty() {
      println!("Couldn't find source code of dependencies:");
      for src in missing_sources.iter() {
         println!("   {}", src);
      }

      println!("
The dependencies might be platform specific and not needed on your current platform.
You might try calling 'cargo fetch' by hand again.
");
   }

   Ok(())
}

fn fetch_source_of_dependencies() -> AppResult<()>
{
   println!("Fetching source of dependencies ...");

   let mut cmd = Command::new("cargo");
   cmd.arg("fetch");

   let _ = try!(cmd.output());
   Ok(())
}

/// Searches for a directory containing a `Cargo.toml` file starting at
/// `start_dir` and continuing the search upwards the directory tree
/// until a directory is found.
fn find_cargo_toml_dir(start_dir: &Path) -> AppResult<PathBuf>
{
   let mut dir = start_dir.to_path_buf();
   loop {
      if let Ok(files) = fs::read_dir(&dir) {
         for file in files {
            if let Ok(file) = file {
               if file.path().is_file() {
                  if let Some("Cargo.toml") = file.path().file_name().and_then(|s| s.to_str()) {
                     return Ok(dir);
                  }
               }
            }
         }
      }

      if ! dir.pop() {
         return Err(app_err_msg(format!("Couldn't find 'Cargo.toml' starting at directory '{}'!", start_dir.display())));
      }
   }
}

/// the tags file containing the tags for the rust standard library
fn rust_std_lib_tags_file(tags_kind: &TagsKind) -> AppResult<PathBuf>
{
   let mut tags_file = try!(rusty_tags_dir().map(Path::to_path_buf));
   tags_file.push(&format!("rust-std-lib.{}", tags_kind.tags_file_extension()));

   Ok(tags_file)
}
