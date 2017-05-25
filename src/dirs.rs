use std::fs;
use std::env;
use std::path::{Path, PathBuf};

use rt_result::RtResult;

lazy_static! {
    static ref HOME_DIR: RtResult<PathBuf> = home_dir_internal();
    static ref RUSTY_TAGS_DIR: RtResult<PathBuf> = rusty_tags_dir_internal();
    static ref RUSTY_TAGS_CACHE_DIR: RtResult<PathBuf> = rusty_tags_cache_dir_internal();
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
