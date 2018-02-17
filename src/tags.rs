use std::fs::{File, OpenOptions, copy, rename};
use std::io::{Read, Write};
use std::collections::HashSet;
use std::path::Path;
use tempfile::NamedTempFile;
use scoped_threadpool::Pool;

use rt_result::RtResult;
use types::{TagsKind, Source, DepTree, WhichDep};
use config::Config;
use dirs::rusty_tags_cache_dir;

pub fn update_tags(config: &Config, dep_tree: &DepTree) -> RtResult<()> {
    let which_deps = if config.force_recreate { WhichDep::All } else { WhichDep::NeedsTagsUpdate };
    let deps_by_depth = dep_tree.deps_by_depth(which_deps);
    if deps_by_depth.is_empty() {
        return Ok(());
    }

    if deps_by_depth.len() == 1 && deps_by_depth[0].len() == 1 {
        update_tags_internal(config, deps_by_depth[0][0])?;
    }
    else {
        let mut thread_pool = Pool::new(config.num_threads);
        for deps in &deps_by_depth {
            thread_pool.scoped(|scoped| {
                for dep in deps {
                    scoped.execute(move || {
                        update_tags_internal(config, dep).unwrap();
                    });
                }
            });
        }
    }

    return Ok(());

    fn update_tags_internal(config: &Config, dep_tree: &DepTree) -> RtResult<()> {
        if ! config.force_recreate && ! dep_tree.source.needs_tags_update() {
            return Ok(());
        }

        // create a separate temporary file for every tags file
        // and don't share any temporary directories

        let tmp_src_tags = NamedTempFile::new()?;
        create_tags(config, &[&dep_tree.source.dir], tmp_src_tags.path())?;

        let direct_dep_sources = dep_tree.direct_dep_sources();

        // create the cached tags file of 'dep_tree.source' which
        // might also contain the tags of dependencies if they're
        // reexported
        {
            let cached_tags_file = &dep_tree.source.cached_tags_file;
            let reexp_sources = reexported_sources(config, &dep_tree.source, &direct_dep_sources)?;
            let mut reexp_tags_files = Vec::new();
            for source in &reexp_sources {
                reexp_tags_files.push(source.cached_tags_file.as_path());
            }

            let tmp_cached_tags = NamedTempFile::new_in(rusty_tags_cache_dir()?)?;
            if ! reexp_tags_files.is_empty() {
                merge_tags(config, tmp_src_tags.path(), &reexp_tags_files, tmp_cached_tags.path())?;
            } else {
                copy_tags(config, tmp_src_tags.path(), tmp_cached_tags.path())?;
            }

            move_tags(config, tmp_cached_tags.path(), &cached_tags_file)?;
        }

        // create the source tags file of 'dep_tree.source' by merging
        // the tags of 'source' and of its dependencies
        {
            let mut dep_tags_files = Vec::new();
            for source in &direct_dep_sources {
                dep_tags_files.push(source.cached_tags_file.as_path());
            }

            let tmp_src_and_dep_tags = NamedTempFile::new_in(&dep_tree.source.dir)?;
            if ! dep_tags_files.is_empty() {
                merge_tags(config, tmp_src_tags.path(), &dep_tags_files, tmp_src_and_dep_tags.path())?;
            } else {
                copy_tags(config, tmp_src_tags.path(), tmp_src_and_dep_tags.path())?;
            }

            move_tags(config, tmp_src_and_dep_tags.path(), &dep_tree.source.tags_file)?;
        }

        Ok(())
    }
}

/// creates tags recursive for the directory hierarchies starting at `src_dirs`
/// and writes them to `tags_file`
pub fn create_tags<P1, P2>(config: &Config, src_dirs: &[P1], tags_file: P2) -> RtResult<()>
    where P1: AsRef<Path>,
          P2: AsRef<Path>
{
    let mut cmd = config.tags_spec.ctags_command();
    cmd.arg("-o")
       .arg(tags_file.as_ref());

    for dir in src_dirs {
        cmd.arg(dir.as_ref());
    }

    if config.verbose {
        verbose_!("\nCreating tags ...\n   for source:");
        for dir in src_dirs {
            verbose_!("      {}", dir.as_ref().display());
        }

        verbose_!("\n   cached at:\n      {}", tags_file.as_ref().display());
    }

    let output = cmd.output()
        .map_err(|err| format!("'ctags' execution failed: {}\nIs 'ctags' correctly installed?", err))?;

    if ! output.status.success() {
        let mut msg = String::from_utf8_lossy(&output.stderr).into_owned();
        if msg.is_empty() {
            msg = String::from_utf8_lossy(&output.stdout).into_owned();
        }

        if msg.is_empty() {
            msg = "ctags execution failed without any stderr or stdout output".to_string();
        }

        return Err(msg.into());
    }

    Ok(())
}

pub fn copy_tags(config: &Config, from_tags: &Path, to_tags: &Path) -> RtResult<()> {
    verbose!(config, "\nCopy tags ...\n   from:\n      {}\n   to:\n      {}",
             from_tags.display(), to_tags.display());

    let _ = copy(from_tags, to_tags)?;
    Ok(())
}

pub fn move_tags(config: &Config, from_tags: &Path, to_tags: &Path) -> RtResult<()> {
    verbose!(config, "\nMove tags ...\n   from:\n      {}\n   to:\n      {}",
             from_tags.display(), to_tags.display());

    let _ = rename(from_tags, to_tags)?;
    Ok(())
}

fn reexported_sources<'a>(config: &Config,
                          source: &Source,
                          dep_sources: &[&'a Source])
                          -> RtResult<Vec<&'a Source>> {
    let reexp_crates = find_reexported_crates(&source.dir)?;
    if reexp_crates.is_empty() {
        return Ok(Vec::new());
    }

    if config.verbose {
        verbose_!("\nFound public reexports in '{}' of:", source.name);
        for rcrate in &reexp_crates {
            verbose_!("   {}", rcrate);
        }

        verbose_!("");
    }

    let mut reexp_deps = Vec::new();
    for rcrate in reexp_crates {
        if let Some(crate_dep) = dep_sources.iter().find(|d| d.name == *rcrate) {
            reexp_deps.push(*crate_dep);
        }
    }

    Ok(reexp_deps)
}

/// merges the library tag file `lib_tag_file` and its dependency tag files
/// `dependency_tag_files` into `into_tag_file`
fn merge_tags(config: &Config,
              lib_tag_file: &Path,
              dependency_tag_files: &[&Path],
              into_tag_file: &Path)
              -> RtResult<()> {
    if config.verbose {
        verbose_!("\nMerging ...\n   tags:");
        verbose_!("      {}", lib_tag_file.display());
        for file in dependency_tag_files {
            verbose_!("      {}", file.display());
        }
        verbose_!("\n   into:\n      {}", into_tag_file.display());
    }

    match config.tags_spec.kind {
        TagsKind::Vi => {
            let mut file_contents: Vec<String> = Vec::new();

            {
                let mut file = File::open(lib_tag_file)?;
                let mut contents = String::new();
                file.read_to_string(&mut contents)?;
                file_contents.push(contents);
            }

            for file in dependency_tag_files {
                let mut file = File::open(file)?;
                let mut contents = String::new();
                file.read_to_string(&mut contents)?;
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

            let mut tag_file = OpenOptions::new()
                .create(true)
                .truncate(true)
                .read(true)
                .write(true)
                .open(into_tag_file)?;

            tag_file.write_fmt(format_args!("{}\n", "!_TAG_FILE_FORMAT	2	/extended format; --format=1 will not append ;\" to lines/"))?;
            tag_file.write_fmt(format_args!("{}\n", "!_TAG_FILE_SORTED	1	/0=unsorted, 1=sorted, 2=foldcase/"))?;

            for line in merged_lines.iter() {
                tag_file.write_fmt(format_args!("{}\n", *line))?;
            }
        },

        TagsKind::Emacs => {
            if lib_tag_file != into_tag_file {
                copy_tags(config, lib_tag_file, into_tag_file)?;
            }

            let mut tag_file = OpenOptions::new()
                .create(true)
                .append(true)
                .read(true)
                .write(true)
                .open(into_tag_file)?;

            for file in dependency_tag_files {
                if *file != into_tag_file {
                    tag_file.write_fmt(format_args!("{},include\n", file.display()))?;
                }
            }
        }
    }

    Ok(())
}

type CrateName = String;

/// searches in the file `<src_dir>/lib.rs` for external crates
/// that are reexpored and returns their names
fn find_reexported_crates(src_dir: &Path) -> RtResult<Vec<CrateName>> {
    let lib_file = src_dir.join("lib.rs");
    if ! lib_file.is_file() {
        return Ok(Vec::new());
    }

    let contents = {
        let mut file = File::open(&lib_file)?;
        let mut contents = String::new();
        file.read_to_string(&mut contents)?;
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
