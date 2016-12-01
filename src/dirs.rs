use std::fs;
use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;
use glob::{glob, Paths};

use rt_result::RtResult;

lazy_static! {
    static ref HOME_DIR               : RtResult<PathBuf> = home_dir_internal();
    static ref RUSTY_TAGS_DIR         : RtResult<PathBuf> = rusty_tags_dir_internal();
    static ref RUSTY_TAGS_CACHE_DIR   : RtResult<PathBuf> = rusty_tags_cache_dir_internal();
    static ref CARGO_DIR              : RtResult<PathBuf> = cargo_dir_internal();
    static ref CARGO_GIT_SRC_DIR      : RtResult<PathBuf> = cargo_git_src_dir_internal();
    static ref CARGO_CRATES_IO_SRC_DIR: RtResult<PathBuf> = cargo_crates_io_src_dir_internal();
}

/// where rusty-tags puts all of its stuff
pub fn rusty_tags_dir() -> RtResult<&'static Path> {
    RUSTY_TAGS_DIR
        .as_ref()
        .map(|pb| pb.as_path())
        .map_err(|err| err.clone())
}

/// where `rusty-tags` caches its tag files
pub fn rusty_tags_cache_dir() -> RtResult<&'static Path> {
    RUSTY_TAGS_CACHE_DIR
        .as_ref()
        .map(|pb| pb.as_path())
        .map_err(|err| err.clone())
}

/// the root directory of cargo
pub fn cargo_dir() -> RtResult<PathBuf> {
    CARGO_DIR.clone()
}

/// where cargo puts its git checkouts
pub fn cargo_git_src_dir() -> RtResult<&'static Path> {
    CARGO_GIT_SRC_DIR
        .as_ref()
        .map(|pb| pb.as_path())
        .map_err(|err| err.clone())
}

/// where cargo puts the source code of crates.io
pub fn cargo_crates_io_src_dir() -> RtResult<&'static Path> {
    CARGO_CRATES_IO_SRC_DIR
        .as_ref()
        .map(|pb| pb.as_path())
        .map_err(|err| err.clone())
}

pub fn glob_path(pattern: &String) -> RtResult<Paths> {
    Ok(try!(glob(&pattern)))
}

fn home_dir() -> RtResult<PathBuf> {
    HOME_DIR.clone()
}

fn home_dir_internal() -> RtResult<PathBuf> {
    if let Some(path) = env::home_dir() {
        Ok(path)
    } else {
        Err("Couldn't read home directory!".into())
    }
}

fn rusty_tags_cache_dir_internal() -> RtResult<PathBuf> {
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

fn rusty_tags_dir_internal() -> RtResult<PathBuf> {
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

fn cargo_git_src_dir_internal() -> RtResult<PathBuf> {
    cargo_dir().map(|mut d| {
        d.push("git");
        d.push("checkouts");
        d
    })
}

fn cargo_crates_io_src_dir_internal() -> RtResult<PathBuf> {
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
    } else {
        Err(format!("Expected one matching path for '{}'!", src_str).into())
    }
}

fn cargo_dir_internal() -> RtResult<PathBuf> {
    if let Ok(out) = Command::new("multirust").arg("show-override").output() {
        let output = try!(
            String::from_utf8(out.stdout)
                .map_err(|_| "Couldn't convert 'multirust show-override' output to utf8!")
        );

        // Make it compatible with 'rustup' which currently still installs
        // a 'multirust' binary, but there's no 'cargo' directory anymore
        // under the toolchain path.
        //
        // The downloaded sources of 'cargo' are now again under '~/.cargo/'.
        let mut found_location_but_without_cargo_dir = false;

        for line in output.lines() {
            let strs: Vec<&str> = line.split(" location: ").collect();
            if strs.len() == 2 {
                let mut path = PathBuf::new();
                path.push(strs[1]);
                path.push("cargo");
                if path.is_dir() {
                    return Ok(path);
                } else {
                    found_location_but_without_cargo_dir = true;
                    break;
                }
            }
        }

        if ! found_location_but_without_cargo_dir {
            return Err(format!("Couldn't get multirust cargo location from output:\n{}", output).into());
        }
    }

    if let Ok(d) = env::var("CARGO_HOME") {
        Ok(PathBuf::from(d))
    } else {
        home_dir().map(|mut d| { d.push(".cargo"); d })
    }
}
