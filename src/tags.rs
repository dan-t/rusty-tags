use std::fs::{File, OpenOptions, copy, rename};
use std::io::{Read, Write, BufWriter};
use std::path::Path;

use tempfile::NamedTempFile;
use scoped_threadpool::Pool;
use streaming_iterator::StreamingIterator;
use fnv::FnvHashSet;

use rt_result::RtResult;
use types::{TagsKind, Source, Sources, DepTree, SourceWithDepth};
use config::Config;
use dirs::rusty_tags_cache_dir;

/// Update the tags of all sources of 'roots'.
pub fn update_tags(config: &Config, dep_tree: &DepTree) -> RtResult<()> {
    if ! config.quiet {
        let names: Vec<_> = dep_tree.roots().map(|r| &r.name).collect();
        println!("Creating tags for: {:?} ...", names);
    }

    let mut thread_pool = if config.num_threads > 1 {
        Some(Pool::new(config.num_threads))
    } else {
        None
    };

    let mut sources_by_depth = dep_tree.split_by_depth();
    while let Some(sources_of_depth) = sources_by_depth.next() {
        if sources_of_depth.is_empty() {
            continue;
        }

        let sources_to_update = sources_of_depth.iter().filter(|s| {
            s.source.needs_tags_update(config)
        });

        if config.verbose {
            println!("\nSources of depth={}", sources_of_depth[0].depth);

            for SourceWithDepth { source, .. } in sources_to_update.clone() {
                println!("   {}", source.recreate_status(config));
            }
        }

        if let Some(ref mut thread_pool) = thread_pool {
            thread_pool.scoped(|scoped| {
                for SourceWithDepth { source, .. } in sources_to_update {
                    scoped.execute(move || {
                        let deps = dep_tree.dependencies(source);
                        update_tags_internal(config, source, deps).unwrap();
                    });
                }
            });
        } else {
            for SourceWithDepth { source, .. } in sources_to_update {
                let deps = dep_tree.dependencies(source);
                update_tags_internal(config, source, deps)?;
            }
        }
    }

    return Ok(());

    fn update_tags_internal<'a>(config: &Config, source: &'a Source, dependencies: Sources<'a>) -> RtResult<()> {
        // create tags for 'source'
        let tmp_src_tags = NamedTempFile::new()?;
        create_tags(config, &[&source.dir], tmp_src_tags.path())?;

        // create the cached tags file of 'source' which
        // might also contain the tags of dependencies if they're
        // reexported
        {
            let reexported_crates = find_reexported_crates(&source.dir)?;

            if ! reexported_crates.is_empty() && config.verbose {
                println!("\nFound public reexports in '{}' of:", source.name);
                for rcrate in &reexported_crates {
                    println!("   {}", rcrate);
                }

                println!("");
            }

            // collect the tags files of reexported dependencies
            let reexported_tags_files: Vec<&Path> = dependencies.clone()
                .filter(|d| reexported_crates.iter().find(|c| **c == d.name) != None)
                .map(|d| d.cached_tags_file.as_path())
                .collect();

            let tmp_cached_tags = NamedTempFile::new_in(rusty_tags_cache_dir()?)?;
            if ! reexported_tags_files.is_empty() {
                merge_tags(config, tmp_src_tags.path(), &reexported_tags_files, tmp_cached_tags.path())?;
            } else {
                copy_tags(config, tmp_src_tags.path(), tmp_cached_tags.path())?;
            }

            move_tags(config, tmp_cached_tags.path(), &source.cached_tags_file)?;
        }

        // create the source tags file of 'source' by merging
        // the tags of 'source' and of its dependencies
        {
            let dep_tags_files: Vec<&Path> = dependencies.clone()
                .map(|d| d.cached_tags_file.as_path())
                .collect();

            let tmp_src_and_dep_tags = NamedTempFile::new_in(&source.dir)?;
            if ! dep_tags_files.is_empty() {
                merge_tags(config, tmp_src_tags.path(), &dep_tags_files, tmp_src_and_dep_tags.path())?;
            } else {
                copy_tags(config, tmp_src_tags.path(), tmp_src_and_dep_tags.path())?;
            }

            move_tags(config, tmp_src_and_dep_tags.path(), &source.tags_file)?;
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
        println!("\nCreating tags ...\n   for source:");
        for dir in src_dirs {
            println!("      {}", dir.as_ref().display());
        }

        println!("\n   cached at:\n      {}", tags_file.as_ref().display());
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

/// merges the library tag file `lib_tag_file` and its dependency tag files
/// `dependency_tag_files` into `into_tag_file`
fn merge_tags(config: &Config,
              lib_tag_file: &Path,
              dependency_tag_files: &[&Path],
              into_tag_file: &Path)
              -> RtResult<()> {
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
            if dependency_tag_files.is_empty() {
                if lib_tag_file != into_tag_file {
                    copy_tags(config, lib_tag_file, into_tag_file)?;
                }

                return Ok(());
            }

            let mut file_contents: Vec<String> = Vec::with_capacity(dependency_tag_files.len() + 1);
            let mut num_lines: usize = 0;
            {
                let mut file = File::open(lib_tag_file)?;
                let mut contents = String::new();
                file.read_to_string(&mut contents)?;
                num_lines += contents.lines().count();
                file_contents.push(contents);
            }

            for file in dependency_tag_files {
                let mut file = File::open(file)?;
                let mut contents = String::new();
                file.read_to_string(&mut contents)?;
                num_lines += contents.lines().count();
                file_contents.push(contents);
            }

            let mut merged_lines: Vec<&str> = Vec::with_capacity(num_lines);
            for content in file_contents.iter() {
                for line in content.lines() {
                    if let Some(chr) = line.chars().nth(0) {
                        if chr != '!' {
                            merged_lines.push(line);
                        }
                    }
                }
            }

            verbose!(config, "\nNum merged lines: {}", merged_lines.len());

            merged_lines.sort_unstable();
            merged_lines.dedup();

            let mut tag_file = BufWriter::with_capacity(64000, OpenOptions::new()
                .create(true)
                .truncate(true)
                .read(true)
                .write(true)
                .open(into_tag_file)?);

            tag_file.write_fmt(format_args!("{}\n", "!_TAG_FILE_FORMAT	2	/extended format; --format=1 will not append ;\" to lines/"))?;
            tag_file.write_fmt(format_args!("{}\n", "!_TAG_FILE_SORTED	1	/0=unsorted, 1=sorted, 2=foldcase/"))?;

            let new_line = "\n".as_bytes();
            for line in merged_lines {
                tag_file.write_all(line.as_bytes())?;
                tag_file.write_all(new_line)?;
            }
        },

        TagsKind::Emacs => {
            if lib_tag_file != into_tag_file {
                copy_tags(config, lib_tag_file, into_tag_file)?;
            }

            let mut tag_file = BufWriter::with_capacity(64000, OpenOptions::new()
                .create(true)
                .append(true)
                .read(true)
                .write(true)
                .open(into_tag_file)?);

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
    let mut pub_uses = FnvHashSet::<ModuleName>::default();

    #[derive(Eq, PartialEq, Hash)]
    struct ExternCrate<'a>
    {
        name: &'a str,
        as_name: &'a str
    }

    let mut extern_crates = FnvHashSet::<ExternCrate>::default();

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
