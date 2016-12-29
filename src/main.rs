extern crate libc;
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
use tempdir::TempDir;

use rt_result::RtResult;
use dependencies::read_dependencies;
use tags::{update_tags, create_tags, move_tags};
use config::Config;

mod rt_result;
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

fn execute() -> RtResult<()> {
    let config = try!(Config::from_command_args());
    try!(update_all_tags(&config));
    let _ = try!(config.close_temp_dirs());
    Ok(())
}

fn update_all_tags(config: &Config) -> RtResult<()> {
    try!(fetch_source_of_dependencies(config));
    try!(update_std_lib_tags(&config));

    let cargo_toml = try!(find_file_upwards("Cargo.toml", &config.start_dir));
    let cargo_lock = try!(find_file_upwards("Cargo.lock", &config.start_dir));
    let dep_tree = try!(read_dependencies(&cargo_toml, &cargo_lock));
    update_tags(&config, &dep_tree)
}

fn fetch_source_of_dependencies(config: &Config) -> RtResult<()> {
    if ! config.quiet {
        println!("Fetching source of dependencies ...");
    }

    let mut cmd = Command::new("cargo");
    cmd.arg("fetch");

    let output = try!(cmd.output()
        .map_err(|err| format!("'cargo' execution failed: {}\nIs 'cargo' correctly installed?", err)));

    if ! output.status.success() {
        let mut msg = String::from_utf8_lossy(&output.stderr).into_owned();
        if msg.is_empty() {
            msg = String::from_utf8_lossy(&output.stdout).into_owned();
        }

        return Err(msg.into());
    }

    Ok(())
}

/// Searches for a file named `file_name` starting at `start_dir` and continuing the search upwards
/// the directory tree until the file is found.
fn find_file_upwards(file_name: &str, start_dir: &Path) -> RtResult<PathBuf> {
    let mut dir = start_dir.to_path_buf();
    loop {
        if let Ok(files) = fs::read_dir(&dir) {
            for path in files.map(|r| r.map(|d| d.path())) {
                match path {
                    Ok(ref path) if path.is_file() =>
                        match path.file_name() {
                            Some(name) if name.to_str() == Some(file_name) => return Ok(path.to_path_buf()),
                            _ => continue
                        },
                    _ => continue
                }
            }
        }

        if ! dir.pop() {
            return Err(format!("Couldn't find '{}' starting at directory '{}'!", file_name, start_dir.display()).into());
        }
    }
}

fn update_std_lib_tags(config: &Config) -> RtResult<()> {
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

    let temp_dir = try!(TempDir::new_in(&src_path, "std-lib-temp-dir"));
    let tmp_std_lib_tags = temp_dir.path().join("std_lib_tags");

    try!(create_tags(config, &src_dirs, &tmp_std_lib_tags));
    try!(move_tags(config, &tmp_std_lib_tags, &std_lib_tags));

    try!(temp_dir.close());

    Ok(())
}
