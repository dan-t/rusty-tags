use std::fs::{File, OpenOptions, copy, rename, remove_file};
use std::io::{Read, Write};
use std::process::Command;
use std::collections::HashSet;
use std::path::{PathBuf, Path};

use app_result::{AppResult, app_err_msg};
use types::{TagsKind, SourceKind, DepTree};
use config::Config;

pub fn update_tags(config: &Config, dep_tree: &DepTree) -> AppResult<()> {
    let tags_files = try!(dep_tree.source.tags_files(&config.tags_spec));
    if ! dep_tree.source.is_root() && ! config.force_recreate && tags_files.are_files() {
        return Ok(());
    }

    for dep in &dep_tree.dependencies {
        try!(update_tags(config, dep))
    }

    let tmp_src_tags = config.temp_file("src_tags");
    let src_dir = try!(tags_files.src_dir());
    try!(create_tags(config, &[&src_dir], &&tmp_src_tags));

    let direct_deps = dep_tree.direct_dep_sources();

    // create the cached tags file of 'dep_tree.source' which
    // might also contain the tags of dependencies if they're
    // reexported
    if let Some(cached_tags_file) = tags_files.cached_tags_file {
        let reexp_deps = try!(reexported_deps(config,
                                              &dep_tree.source,
                                              &direct_deps));

        let mut reexp_tags_files = Vec::new();
        for rdep in &reexp_deps {
            let tags_files = try!(rdep.tags_files(&config.tags_spec));
            if let Some(file) = tags_files.cached_tags_file {
                reexp_tags_files.push(file);
            }
        }

        let tmp_cached_tags = config.temp_file("cached_tags");
        if ! reexp_tags_files.is_empty() {
            try!(merge_tags(config, &tmp_src_tags, &reexp_tags_files, &tmp_cached_tags));
        } else {
            try!(copy(&tmp_src_tags, &tmp_cached_tags));
        }

        try!(move_tags(config, &tmp_cached_tags, &cached_tags_file));
    }

    // create the source tags file of 'dep_tree.source' by merging
    // the tags of 'source' and of its dependencies
    {
        let mut dep_tags_files = Vec::new();
        for dep in &direct_deps {
            let tags_files = try!(dep.tags_files(&config.tags_spec));
            if let Some(file) = tags_files.cached_tags_file {
                dep_tags_files.push(file);
            }
        }

        let tmp_src_and_deps_tags = config.temp_file("src_and_deps_tags");
        if ! dep_tags_files.is_empty() {
            try!(merge_tags(config, &tmp_src_tags, &dep_tags_files, &tmp_src_and_deps_tags));
        } else {
            try!(copy(&tmp_src_tags, &tmp_src_and_deps_tags));
        }

        try!(move_tags(config, &tmp_src_and_deps_tags, &tags_files.src_tags_file));
    }

    try!(remove_file(&tmp_src_tags));

    Ok(())
}

/// creates tags recursive for the directory hierarchies starting at `src_dirs`
/// and writes them to `tags_file`
pub fn create_tags<P: AsRef<Path>>(config: &Config, src_dirs: &[P], tags_file: &P) -> AppResult<()> {
    let mut cmd = Command::new("ctags");

    config.tags_spec.ctags_option().map(|opt| { cmd.arg(opt); () });

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
        println!("\nCreating tags ...\n   for source:");
        for dir in src_dirs {
            println!("      {}", dir.as_ref().display());
        }

        println!("\n   cached at:\n      {}", tags_file.as_ref().display());
    }

    let output = try!(cmd.output()
        .map_err(|err| app_err_msg(format!("ctags execution failed: {}", err))));

    if ! output.status.success() {
        let mut msg = String::from_utf8_lossy(&output.stderr).into_owned();
        if msg.is_empty() {
            msg = String::from_utf8_lossy(&output.stdout).into_owned();
        }

        if msg.is_empty() {
            msg = "ctags execution failed without any stderr or stdout output".to_string();
        }

        return Err(app_err_msg(msg));
    }

    Ok(())
}

pub fn move_tags(config: &Config, from_tags: &Path, to_tags: &Path) -> AppResult<()> {
    if config.verbose {
        println!("\nMove tags ...\n   from:\n      {}\n   to:\n      {}", from_tags.display(), to_tags.display());
    }

    let _ = try!(rename(from_tags, to_tags));
    Ok(())
}

fn reexported_deps(config: &Config,
                   source: &SourceKind,
                   deps: &[SourceKind])
                   -> AppResult<Vec<SourceKind>> {
    let tags_files = try!(source.tags_files(&config.tags_spec));
    let reexp_crates = try!(find_reexported_crates(&try!(tags_files.src_dir())));
    if reexp_crates.is_empty() {
        return Ok(Vec::new());
    }

    if config.verbose {
        println!("Found public reexports in '{}' of:", source.get_lib_name());
        for rcrate in reexp_crates.iter() {
            println!("   {}", rcrate);
        }
        println!("");
    }

    let mut reexp_deps = Vec::new();
    for rcrate in &reexp_crates {
        if let Some(crate_dep) = deps.iter().find(|d| d.get_lib_name() == *rcrate) {
            reexp_deps.push(crate_dep.clone());
        }
    }

    Ok(reexp_deps)
}

/// merges the library tag file `lib_tag_file` and its dependency tag files
/// `dependency_tag_files` into `into_tag_file`
fn merge_tags(config: &Config,
              lib_tag_file: &Path,
              dependency_tag_files: &[PathBuf],
              into_tag_file: &Path)
              -> AppResult<()> {
    if config.verbose {
        println!("\nMerging ...\n   tags:");
        println!("      {}", lib_tag_file.display());
        for file in dependency_tag_files {
            println!("      {}", file.display());
        }
        println!("\n   into:\n      {}", into_tag_file.display());
    }

    match config.tags_spec.kind {
        TagsKind::Vi => {
            let mut file_contents: Vec<String> = Vec::new();

            {
                let mut file = try!(File::open(lib_tag_file));
                let mut contents = String::new();
                try!(file.read_to_string(&mut contents));
                file_contents.push(contents);
            }

            for file in dependency_tag_files {
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

            let mut tag_file = try!(OpenOptions::new()
                .create(true)
                .truncate(true)
                .read(true)
                .write(true)
                .open(into_tag_file));

            try!(tag_file.write_fmt(format_args!("{}\n", "!_TAG_FILE_FORMAT	2	/extended format; --format=1 will not append ;\" to lines/")));
            try!(tag_file.write_fmt(format_args!("{}\n", "!_TAG_FILE_SORTED	1	/0=unsorted, 1=sorted, 2=foldcase/")));

            for line in merged_lines.iter() {
                try!(tag_file.write_fmt(format_args!("{}\n", *line)));
            }
        },

        TagsKind::Emacs => {
            if lib_tag_file != into_tag_file {
                try!(copy(lib_tag_file, into_tag_file));
            }

            let mut tag_file = try!(OpenOptions::new()
                .create(true)
                .append(true)
                .read(true)
                .write(true)
                .open(into_tag_file));

            for file in dependency_tag_files {
                if file.as_path() != into_tag_file {
                    try!(tag_file.write_fmt(format_args!("{},include\n", file.display())));
                }
            }
        }
    }

    Ok(())
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
