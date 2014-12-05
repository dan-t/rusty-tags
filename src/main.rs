#![feature(if_let)]

#![allow(dead_code)]
#![allow(unused_variables)]
#![allow(unused_mut)]
#![allow(unused_imports)]

extern crate toml;
extern crate glob;

use std::io::fs::PathExtensions;
use std::io::fs;
use std::io;
use std::os;

use app_result::{AppResult, app_err};

use dependencies::{
   Dependency,
   read_dependencies
};

use tags::{
   Tags,
   update_git_tags,
   update_crates_io_tags,
   create_tags,
   merge_tags
};

use dirs::tags_dir;

mod app_result;
mod dependencies;
mod dirs;
mod tags;

fn main() 
{
   update_tags().unwrap_or_else(|err| {
      let stderr = &mut io::stderr();
      let _ = writeln!(stderr, "rusty-tags: {}", err);
      os::set_exit_status(1);
   });
}

fn update_tags() -> AppResult<()>
{
   let cargo_dir = try!(find_cargo_toml_dir(&Path::new("/home/dan/projekte/rusty-tags/src")));
   let deps = try!(read_dependencies(&cargo_dir));

   let mut tag_files: Vec<Path> = Vec::new();
   for dep in deps.iter() {
      match *dep {
         Dependency::Git { ref lib_name, ref commit_hash, .. } => {
            tag_files.push(try!(update_git_tags(lib_name, commit_hash)).tags_file);
         },

         Dependency::CratesIo { ref lib_name, ref version, .. } => {
            tag_files.push(try!(update_crates_io_tags(lib_name, version)).tags_file);
         }
      }
   }

   let mut src_tags = cargo_dir.clone();
   src_tags.push("rusty.tags");
   try!(create_tags(&cargo_dir, &src_tags));
   tag_files.push(src_tags.clone());

   let mut rust_tags = try!(tags_dir());
   rust_tags.push("rust");
   if rust_tags.is_file() {
      tag_files.push(rust_tags);
   }

   try!(merge_tags(&tag_files, &src_tags));

   Ok(())
}

/// Searches for a directory containing a `Cargo.toml` file starting at
/// `start_dir` and continuing the search upwards the directory tree
/// until a directory is found.
fn find_cargo_toml_dir(start_dir: &Path) -> AppResult<Path>
{
   let mut dir = start_dir.clone();
   loop {
      if let Ok(files) = fs::readdir(&dir) {
         for file in files.iter() {
            if file.is_file() {
               if let Some("Cargo.toml") = file.filename_str() {
                  return Ok(dir);
               }
            }
         }
      }

      if ! dir.pop() {
         return Err(app_err(format!("Couldn't find 'Cargo.toml' starting at directory '{}'!", start_dir.display())));
      }
   }
}
