use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::process::Command;
use std::collections::HashSet;
use std::path::{PathBuf, Path};

use app_result::{AppResult, app_err_msg, app_err_missing_src};
use types::{Tags, TagsKind, SourceKind};
use path_ext::PathExt;
use config::Config;

use dirs::{
    rusty_tags_cache_dir,
    cargo_git_src_dir,
    cargo_crates_io_src_dir,
    glob_path
};

/// Checks if there's already a tags file for `source`
/// and if not it's creating a new tags file and returning it.
pub fn update_tags(config: &Config, source: &SourceKind) -> AppResult<Tags> {
    let src_tags = try!(cached_tags_file(&config.tags_kind, source));
    let src_dir = try!(find_src_dir(source));
    if src_tags.as_path().is_file() && ! config.force_recreate {
        return Ok(Tags::new(&src_dir, &src_tags, true));
    }

    try!(create_tags(config, &[&src_dir], &src_tags));
    Ok(Tags::new(&src_dir, &src_tags, false))
}

/// Does the same thing as `update_tags`, but also checks if the `lib.rs`
/// file of the library has public reexports of external crates. If
/// that's the case, then the tags of the public reexported external
/// crates are merged into the tags of the library.
pub fn update_tags_and_check_for_reexports(config: &Config,
                                           source: &SourceKind,
                                           dependencies: &Vec<SourceKind>) 
                                           -> AppResult<Tags> {
    let lib_tags = try!(update_tags(config, source));
    if lib_tags.is_up_to_date(&config.tags_kind) && ! config.force_recreate {
        return Ok(lib_tags);
    }

    let reexp_crates = try!(find_reexported_crates(&lib_tags.src_dir));
    if reexp_crates.is_empty() {
        return Ok(lib_tags);
    }

    if config.verbose {
        println!("Found public reexports in '{}' of:", source.get_lib_name());
        for rcrate in reexp_crates.iter() {
            println!("   {}", rcrate);
        }
        println!("");
    }

    let mut crate_tags = Vec::<PathBuf>::new();
    for rcrate in reexp_crates.iter() {
        if let Some(crate_dep) = dependencies.iter().find(|d| d.get_lib_name() == *rcrate) {
            crate_tags.push(try!(update_tags(config, crate_dep)).tags_file.clone());
        }
    }

    if crate_tags.is_empty() {
        return Ok(lib_tags);
    }

    crate_tags.push(lib_tags.tags_file.clone());
    try!(merge_tags(config, &crate_tags, &lib_tags.tags_file));
    Ok(lib_tags)
}

/// merges `tag_files` into `into_tag_file`
pub fn merge_tags(config: &Config, tag_files: &Vec<PathBuf>, into_tag_file: &Path) -> AppResult<()> {
    if config.verbose {
        println!("Merging ...\n   tags:");

        for file in tag_files.iter() {
            println!("      {}", file.display());
        }

        println!("\n   into:\n      {}\n", into_tag_file.display());
    }

    match config.tags_kind {
        TagsKind::Vi => {
            let mut file_contents: Vec<String> = Vec::new();
            for file in tag_files.iter() {
                let mut file = try!(File::open(file));
                let mut contents = String::new();
                try!(file.read_to_string(&mut contents));
                file_contents.push(contents);
            }

            let mut merged_lines: Vec<&str> = Vec::with_capacity(100_000);
            for content in file_contents.iter() {
                for line in content.lines() {
                    if let Some(chr) = line.chars().nth(0) {
                        if chr != '!' {
                            merged_lines.push(line);
                        }
                    }
                }
            }

            merged_lines.sort();
            merged_lines.dedup();

            let mut tag_file = try!(
                OpenOptions::new()
                .create(true)
                .truncate(true)
                .read(true)
                .write(true)
                .open(into_tag_file)
            );

            try!(tag_file.write_fmt(format_args!("{}\n", "!_TAG_FILE_FORMAT	2	/extended format; --format=1 will not append ;\" to lines/")));
            try!(tag_file.write_fmt(format_args!("{}\n", "!_TAG_FILE_SORTED	1	/0=unsorted, 1=sorted, 2=foldcase/")));

            for line in merged_lines.iter() {
                try!(tag_file.write_fmt(format_args!("{}\n", *line)));
            }
        },

        TagsKind::Emacs => {
            let mut tag_file = try!(
                OpenOptions::new()
                .create(true)
                .append(true)
                .read(true)
                .write(true)
                .open(into_tag_file)
            );

            for file in tag_files.iter() {
                if file.as_path() != into_tag_file {
                    try!(tag_file.write_fmt(format_args!("{},include\n", file.display())));
                }
            }
        }
    }

    Ok(())
}

/// creates tags recursive for the directory hierarchy starting at `src_dirs`
/// and writes them to `tags_file`
pub fn create_tags<P: AsRef<Path>>(config: &Config, src_dirs: &[P], tags_file: P) -> AppResult<()> {
    let mut cmd = Command::new("ctags");

    config.tags_kind.ctags_option().map(|opt| { cmd.arg(opt); () });

    cmd.arg("--recurse")
        .arg("--languages=Rust")
        .arg("--langdef=Rust")
        .arg("--langmap=Rust:.rs")
        .arg("--regex-Rust=/^[ \\t]*(#\\[[^\\]]\\][ \\t]*)*(pub[ \\t]+)?(extern[ \\t]+)?(\"[^\"]+\"[ \\t]+)?(unsafe[ \\t]+)?fn[ \\t]+([a-zA-Z0-9_]+)/\\6/f,functions,function definitions/")
        .arg("--regex-Rust=/^[ \\t]*(pub[ \\t]+)?type[ \\t]+([a-zA-Z0-9_]+)/\\2/T,types,type definitions/")
        .arg("--regex-Rust=/^[ \\t]*(pub[ \\t]+)?enum[ \\t]+([a-zA-Z0-9_]+)/\\2/g,enum,enumeration names/")
        .arg("--regex-Rust=/^[ \\t]*(pub[ \\t]+)?struct[ \\t]+([a-zA-Z0-9_]+)/\\2/s,structure names/")
        .arg("--regex-Rust=/^[ \\t]*(pub[ \\t]+)?mod[ \\t]+([a-zA-Z0-9_]+)\\s*\\{/\\2/m,modules,module names/")
        .arg("--regex-Rust=/^[ \\t]*(pub[ \\t]+)?(static|const)[ \\t]+([a-zA-Z0-9_]+)/\\3/c,consts,static constants/")
        .arg("--regex-Rust=/^[ \\t]*(pub[ \\t]+)?trait[ \\t]+([a-zA-Z0-9_]+)/\\2/t,traits,traits/")
        .arg("--regex-Rust=/^[ \\t]*macro_rules![ \\t]+([a-zA-Z0-9_]+)/\\1/d,macros,macro definitions/")
        .arg("-o")
        .arg(tags_file.as_ref());

    for dir in src_dirs {
        cmd.arg(dir.as_ref());
    }

    if config.verbose {
        println!("Creating tags ...\n   for source:");
        for dir in src_dirs {
            println!("      {}", dir.as_ref().display());
        }

        println!("\n   cached at:\n      {}\n", tags_file.as_ref().display());
    }

    try!(cmd.output());
    Ok(())
}

/// find the source directory of `source`, for git sources the directories
/// in `~/.cargo/git/checkouts` are considered and for crates.io sources
/// the directories in `~/.cargo/registry/src/github.com-*` are considered
fn find_src_dir(source: &SourceKind) -> AppResult<PathBuf> {
    match *source {
        SourceKind::Git { ref lib_name, ref commit_hash } => {
            let mut lib_src = lib_name.clone();
            lib_src.push_str("-*");

            let mut src_dir = try!(cargo_git_src_dir().map(Path::to_path_buf));
            src_dir.push(&lib_src);
            src_dir.push("master");

            let src_paths = try!(glob_path(&format!("{}", src_dir.display())));
            for src_path in src_paths {
                if let Ok(path) = src_path {
                    let src_commit_hash = try!(get_commit_hash(&path));
                    if *commit_hash == src_commit_hash {
                        return Ok(path);
                    }
                }
            }

            // the git repository name hasn't to match the name of the library,
            // so here we're just going through all git directories and searching
            // for the one with a matching commit hash
            let mut src_dir = try!(cargo_git_src_dir().map(Path::to_path_buf));
            src_dir.push("*");
            src_dir.push("master");

            let src_paths = try!(glob_path(&format!("{}", src_dir.display())));
            for src_path in src_paths {
                if let Ok(path) = src_path {
                    let src_commit_hash = try!(get_commit_hash(&path));
                    if *commit_hash == src_commit_hash {
                        return Ok(path);
                    }
                }
            }

            Err(app_err_missing_src(source))
        },

        SourceKind::CratesIo { ref lib_name, ref version } => {
            let mut lib_src = lib_name.clone();
            lib_src.push('-');
            lib_src.push_str(&**version);

            let mut src_dir = try!(cargo_crates_io_src_dir().map(Path::to_path_buf));
            src_dir.push(&lib_src);

            if ! src_dir.is_dir() {
                return Err(app_err_missing_src(source));
            }

            Ok(src_dir)
        },

        SourceKind::Path { ref path, .. } => {
            if ! path.is_dir() {
                return Err(app_err_missing_src(source));
            }

            Ok(path.clone())
        }
    }
}

/// returns the position and name of the cached tags file of `source`
fn cached_tags_file(tags_kind: &TagsKind, source: &SourceKind) -> AppResult<PathBuf> {
    match *source {
        SourceKind::Git { .. } | SourceKind::CratesIo { .. } => {
            let mut tags_file = try!(rusty_tags_cache_dir().map(Path::to_path_buf));
            tags_file.push(&source.tags_file_name(tags_kind));
            Ok(tags_file)
        },

        SourceKind::Path { ref path, .. } => {
            let mut tags_file = path.clone();
            tags_file.push(&source.tags_file_name(tags_kind));
            Ok(tags_file)
        }
    }
}

type CrateName = String;

/// searches in the file `<src_dir>/src/lib.rs` for external crates
/// that are reexpored and returns their names
fn find_reexported_crates(src_dir: &Path) -> AppResult<Vec<CrateName>> {
    let mut lib_file = src_dir.to_path_buf();
    lib_file.push("src");
    lib_file.push("lib.rs");

    if ! lib_file.is_file() {
        return Ok(Vec::new());
    }

    let contents = {
        let mut file = try!(File::open(&lib_file));
        let mut contents = String::new();
        try!(file.read_to_string(&mut contents));
        contents
    };

    let lines = contents.lines();

    type ModuleName = String;
    let mut pub_uses = HashSet::<ModuleName>::new();

    #[derive(Eq, PartialEq, Hash)]
    struct ExternCrate<'a>
    {
        name: &'a str,
        as_name: &'a str
    }

    let mut extern_crates = HashSet::<ExternCrate>::new();

    for line in lines {
        let items = line.trim_matches(';').split(' ').collect::<Vec<&str>>();
        if items.len() < 3 {
            continue;
        }

        if items[0] == "pub" && items[1] == "use" {
            let mods = items[2].split("::").collect::<Vec<&str>>();
            if mods.len() >= 1 {
                pub_uses.insert(mods[0].to_string());
            }
        }

        if items[0] == "extern" && items[1] == "crate" {
            if items.len() == 3 {
                extern_crates.insert(ExternCrate { name: items[2].trim_matches('"'), as_name: items[2] });
            } else if items.len() == 5 && items[3] == "as" {
                extern_crates.insert(ExternCrate { name: items[2].trim_matches('"'), as_name: items[4] });
            }
        }
    }

    let mut reexp_crates = Vec::<CrateName>::new();
    for extern_crate in extern_crates.iter() {
        if pub_uses.contains(extern_crate.as_name) {
            reexp_crates.push(extern_crate.name.to_string());
        }
    }

    Ok(reexp_crates)
}

/// get the commit hash of the current `HEAD` of the git repository located at `git_dir`
fn get_commit_hash(git_dir: &Path) -> AppResult<String> {
    let mut cmd = Command::new("git");
    cmd.current_dir(git_dir)
        .arg("rev-parse")
        .arg("HEAD");

    let out = try!(cmd.output());
    String::from_utf8(out.stdout)
        .map(|s| s.trim().to_string())
        .map_err(|_| app_err_msg("Couldn't convert 'git rev-parse HEAD' output to utf8!".to_string()))
}
