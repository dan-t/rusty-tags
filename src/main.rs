//#![allow(dead_code)]
//#![allow(unused_variables)]

extern crate toml;
extern crate tempfile;
extern crate num_cpus;
extern crate scoped_threadpool;
extern crate serde;
extern crate serde_json;
extern crate fnv;
extern crate semver;
extern crate dirs as extern_dirs;

#[macro_use]
extern crate serde_derive;

#[macro_use]
extern crate clap;

#[macro_use]
extern crate lazy_static;

use std::path::Path;
use std::io::{self, Write};
use std::process::Command;
use std::env;

use tempfile::NamedTempFile;

use rt_result::RtResult;
use dependencies::dependency_tree;
use tags::{update_tags, create_tags, move_tags};
use config::Config;
use types::SourceLock;

#[macro_use]
mod output;

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

    let mut source_locks = Vec::new();
    let dep_tree = {
        let mut dep_tree = dependency_tree(&config, &metadata)?;
        let unlocked_root_ids: Vec<_> = {
            let mut unlocked_roots = Vec::new();
            for source in dep_tree.roots() {
                match source.lock(&config.tags_spec)? {
                    SourceLock::AlreadyLocked { ref path } => {
                        info!(config, "Already creating tags for '{}', if this isn't the case remove the lock file '{}'",
                              source.name, path.display());
                        continue;
                    }

                    sl@SourceLock::Locked { .. } => {
                        source_locks.push(sl);
                        unlocked_roots.push(source);
                    }
                }
            }

            unlocked_roots.iter().map(|r| r.id).collect()
        };

        if unlocked_root_ids.is_empty() {
            return Ok(());
        }

        dep_tree.set_roots(unlocked_root_ids);
        dep_tree
    };

    update_tags(&config, &dep_tree)?;
    Ok(())
}

fn fetch_source_and_metadata(config: &Config) -> RtResult<serde_json::Value> {
    info!(config, "Fetching source and metadata ...");

    env::set_current_dir(&config.start_dir)?;

    let mut cmd = Command::new("cargo");
    cmd.arg("metadata");
    cmd.arg("--format-version=1");

    let output = cmd.output()
        .map_err(|err| format!("'cargo' execution failed: {}\nIs 'cargo' correctly installed?", err))?;

    if ! output.status.success() {
        let mut msg = String::from_utf8_lossy(&output.stderr).into_owned();
        if msg.is_empty() {
            msg = String::from_utf8_lossy(&output.stdout).into_owned();
        }

        return Err(msg.into());
    }

    Ok(serde_json::from_str(&String::from_utf8_lossy(&output.stdout))?)
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

    let output_path = match config.output_dir_std {
        Some(ref path_buf) => path_buf.as_path(),
        None => src_path,
    };
    let std_lib_tags = output_path.join(config.tags_spec.file_name());
    if std_lib_tags.is_file() && ! config.force_recreate {
        return Ok(());
    }

    let possible_src_dirs = [
        // rustc >= 1.47.0
        "alloc",
        "core",
        "panic_abort",
        "panic_unwind",
        "proc_macro",
        "profiler_builtins",
        "rtstartup",
        "std",
        "stdarch",
        "term",
        "test",
        "unwind",

        // rustc < 1.47.0
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

    if src_dirs.is_empty() {
        return Err(format!(r#"
No source directories found for standard library source at $RUST_SRC_PATH:
    '{}'

Please set the standard library source path depending on your rustc version.

For rustc >= 1.47.0:
    $ export RUST_SRC_PATH=$(rustc --print sysroot)/lib/rustlib/src/rust/library/

For rustc < 1.47.0:
    $ export RUST_SRC_PATH=$(rustc --print sysroot)/lib/rustlib/src/rust/src/"#, src_path.display()).into());
    }

    info!(config, "Creating tags for the standard library ...");

    let tmp_std_lib_tags = NamedTempFile::new_in(&output_path)?;
    create_tags(config, &src_dirs, tmp_std_lib_tags.path())?;
    move_tags(config, tmp_std_lib_tags.path(), &std_lib_tags)?;

    Ok(())
}
