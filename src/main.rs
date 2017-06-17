extern crate toml;
extern crate rustc_serialize;
extern crate tempfile;

#[macro_use]
extern crate clap;

#[macro_use]
extern crate lazy_static;

use std::path::Path;
use std::io::{self, Write};
use std::process::Command;
use std::env;
use tempfile::NamedTempFile;
use rustc_serialize::json::Json;

use rt_result::RtResult;
use dependencies::dependency_trees;
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
    let config = Config::from_command_args()?;
    update_all_tags(&config)?;
    Ok(())
}

fn update_all_tags(config: &Config) -> RtResult<()> {
    let metadata = fetch_source_and_metadata(&config)?;
    update_std_lib_tags(&config)?;

    let dep_trees = dependency_trees(&config, &metadata)?;
    for tree in &dep_trees {
        if ! config.quiet {
            println!("Creating tags for '{}' ...", tree.source.name);
        }

        update_tags(&config, &tree)?;
    }

    Ok(())
}

fn fetch_source_and_metadata(config: &Config) -> RtResult<Json> {
    if ! config.quiet {
        println!("Fetching source and metadata ...");
    }

    env::set_current_dir(&config.start_dir)?;

    let mut cmd = Command::new("cargo");
    cmd.arg("metadata");

    let output = cmd.output()
        .map_err(|err| format!("'cargo' execution failed: {}\nIs 'cargo' correctly installed?", err))?;

    if ! output.status.success() {
        let mut msg = String::from_utf8_lossy(&output.stderr).into_owned();
        if msg.is_empty() {
            msg = String::from_utf8_lossy(&output.stdout).into_owned();
        }

        return Err(msg.into());
    }

    Ok(Json::from_str(&String::from_utf8_lossy(&output.stdout))?)
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

    if ! config.quiet {
        println!("Creating tags for the standard library ...");
    }

    let tmp_std_lib_tags = NamedTempFile::new_in(&src_path)?;
    create_tags(config, &src_dirs, tmp_std_lib_tags.path())?;
    move_tags(config, tmp_std_lib_tags.path(), &std_lib_tags)?;

    Ok(())
}
