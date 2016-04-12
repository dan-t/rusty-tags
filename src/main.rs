// to silence a bogus warning about `tag_dir` being unused
#![allow(unused_assignments)]

extern crate toml;
extern crate glob;

#[macro_use]
extern crate clap;

#[macro_use]
extern crate lazy_static;

use std::fs;
use std::path::{PathBuf, Path};
use std::io::{self, Write};
use std::process::Command;
use std::env;

use app_result::{AppResult, AppErr, app_err_msg};
use dependencies::read_dependencies;
use types::TagsRoot;
use path_ext::PathExt;

use tags::{
    update_tags,
    update_tags_and_check_for_reexports,
    create_tags,
    merge_tags
};

use config::Config;

mod app_result;
mod dependencies;
mod dirs;
mod tags;
mod types;
mod path_ext;
mod config;

fn main() {
    execute().unwrap_or_else(|err| {
        writeln!(&mut io::stderr(), "{}", err).unwrap();
        std::process::exit(1);
    });
}

fn execute() -> AppResult<()> {
    let config = try!(Config::from_command_args());
    update_all_tags(&config)
}

fn update_all_tags(config: &Config) -> AppResult<()> {
    try!(fetch_source_of_dependencies(config));
    try!(update_std_lib_tags(&config));

    let cargo_dir = try!(find_cargo_toml_dir(&config.start_dir));
    let tags_roots = try!(read_dependencies(&cargo_dir));

    let mut missing_sources = Vec::new();
    for tags_root in tags_roots.iter() {
        let mut tag_files: Vec<PathBuf> = Vec::new();
        let mut tag_dir: Option<PathBuf> = None;

        match *tags_root {
            TagsRoot::Proj { ref root_dir, ref dependencies } => {
                let mut src_tags = root_dir.clone();
                src_tags.push(config.tags_kind.tags_file_name());

                let src_dir = root_dir.join("src");

                try!(create_tags(config, &[&src_dir], &src_tags));
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

                tag_dir = Some(root_dir.clone());
            },

            TagsRoot::Lib { ref src_kind, ref dependencies } => {
                let lib_tags = match update_tags_and_check_for_reexports(config, src_kind, dependencies) {
                    Ok(tags) => {
                        if tags.is_up_to_date(&config.tags_kind) && ! config.force_recreate {
                            continue;
                        } else {
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

        let mut tags_file = tag_dir.unwrap();
        tags_file.push(config.tags_kind.tags_file_name());

        try!(merge_tags(config, &tag_files, &tags_file));
    }

    if ! config.quiet {
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
    }

    Ok(())
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
            return Err(app_err_msg(format!("Couldn't find 'Cargo.toml' starting at directory '{}'!", start_dir.display())));
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
        return Err(app_err_msg(format!("Missing rust source code at '{}'!", src_path.display())));
    }

    let tags_file = src_path.join(config.tags_kind.tags_file_name());
    if tags_file.is_file() && ! config.force_recreate {
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

    try!(create_tags(&config, &src_dirs, tags_file));
    Ok(())
}
