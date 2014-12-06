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
   TagsRoot,
   SourceKind,
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
   let cwd = try!(os::getcwd());
   let cargo_dir = try!(find_cargo_toml_dir(&cwd));
   let tags_roots = try!(read_dependencies(&cargo_dir));

   for tags_root in tags_roots.iter() {
      let mut tag_files: Vec<Path> = Vec::new();
      let mut tag_dir: Option<Path> = None;

      match *tags_root {
         TagsRoot::Src { ref src_dir, ref dependencies } => {
            let mut src_tags = src_dir.clone();
            src_tags.push("rusty.tags");
            try!(create_tags(src_dir, &src_tags));
            tag_files.push(src_tags);

            for dep in dependencies.iter() {
               tag_files.push(try!(update_tags_of(dep)).tags_file);
            }

            tag_dir = Some(src_dir.clone());
         },

         TagsRoot::Lib { ref src_kind, ref dependencies } => {
            let lib_tags = try!(update_tags_of(src_kind));
            if lib_tags.cached {
               continue;
            }

            tag_files.push(lib_tags.tags_file);
            for dep in dependencies.iter() {
               tag_files.push(try!(update_tags_of(dep)).tags_file);
            }

            tag_dir = Some(lib_tags.src_dir.clone());
         }
      }

      if tag_files.is_empty() || tag_dir.is_none() {
         continue;
      }

      let mut rust_tags = try!(tags_dir());
      rust_tags.push("rust");
      if rust_tags.is_file() {
         tag_files.push(rust_tags);
      }

      let mut tags_file = tag_dir.unwrap();
      tags_file.push("rusty.tags");

      try!(merge_tags(&tag_files, &tags_file));
   }

   Ok(())
}

fn update_tags_of(src_kind: &SourceKind) -> AppResult<Tags>
{
   match *src_kind {
      SourceKind::Git { ref lib_name, ref commit_hash } => {
         update_git_tags(lib_name, commit_hash)
      },

      SourceKind::CratesIo { ref lib_name, ref version } => {
         update_crates_io_tags(lib_name, version)
      }
   }
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
