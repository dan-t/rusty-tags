// to silence a bogus warning about `tag_dir` being unused
#![allow(unused_assignments)]
#![allow(dead_code)]

extern crate toml;
extern crate glob;
extern crate rustc_serialize;
extern crate tempdir;

#[macro_use]
extern crate clap;

#[macro_use]
extern crate lazy_static;

use std::fs;
use std::path::{PathBuf, Path};
use std::io::{self, Write};
use std::process::Command;
use std::env;

use app_result::AppResult;
use dependencies::read_dependencies;
use tags::{update_tags, create_tags, move_tags};
use config::Config;

mod app_result;
mod dependencies;
mod dirs;
mod tags;
mod types;
mod config;

fn main() {
    execute().unwrap_or_else(|err| {
        writeln!(&mut io::stderr(), "{}", err).unwrap();
        std::process::exit(1);
    });
}

fn execute() -> AppResult<()> {
    let config = try!(Config::from_command_args());
    try!(update_all_tags(&config));
    let _ = try!(config.temp_dir.close());
    Ok(())
}

fn update_all_tags(config: &Config) -> AppResult<()> {
    try!(fetch_source_of_dependencies(config));
    try!(update_std_lib_tags(&config));

    let cargo_dir = try!(find_cargo_toml_dir(&config.start_dir));
    let dep_tree = try!(read_dependencies(&cargo_dir));
    update_tags(&config, &dep_tree)
}

fn fetch_source_of_dependencies(config: &Config) -> AppResult<()> {
    if ! config.quiet {
        println!("Fetching source of dependencies ...");
    }

    let mut cmd = Command::new("cargo");
    cmd.arg("fetch");

    let _ = try!(cmd.output());
    Ok(())
}

/// Searches for a directory containing a `Cargo.toml` file starting at
/// `start_dir` and continuing the search upwards the directory tree
/// until a directory is found.
fn find_cargo_toml_dir(start_dir: &Path) -> AppResult<PathBuf> {
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
            return Err(format!("Couldn't find 'Cargo.toml' starting at directory '{}'!", start_dir.display()).into());
        }
    }
}

fn update_std_lib_tags(config: &Config) -> AppResult<()> {
    let src_path_str = env::var("RUST_SRC_PATH");
    if ! src_path_str.is_ok() {
        return Ok(());
    }

    let src_path_str = src_path_str.unwrap();
    let src_path = Path::new(&src_path_str);
    if ! src_path.is_dir() {
        return Err(format!("Missing rust source code at '{}'!", src_path.display()).into());
    }

    let std_lib_tags = src_path.join(config.tags_spec.file_name());
    if std_lib_tags.is_file() && ! config.force_recreate {
        return Ok(());
    }

    let possible_src_dirs = [
        "liballoc",
        "libarena",
        "libbacktrace",
        "libcollections",
        "libcore",
        "libflate",
        "libfmt_macros",
        "libgetopts",
        "libgraphviz",
        "liblog",
        "librand",
        "librbml",
        "libserialize",
        "libstd",
        "libsyntax",
        "libterm"
    ];

    let mut src_dirs = Vec::new();
    for dir in &possible_src_dirs {
        let src_dir = src_path.join(&dir);
        if src_dir.is_dir() {
            src_dirs.push(src_dir);
        }
    }

    let tmp_std_lib_tags = config.temp_file("std_lib_tags");
    try!(create_tags(config, &src_dirs, &tmp_std_lib_tags));
    try!(move_tags(config, &tmp_std_lib_tags, &std_lib_tags));
    Ok(())
}
