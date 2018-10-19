use std::path::{Path, PathBuf};
use std::fs::{self, File};
use std::hash::{Hash, Hasher};
use std::process::Command;
use std::cmp::Ordering;
use std::ops::Drop;
use std::sync::Arc;
use fnv::{FnvHasher, FnvHashMap};

use streaming_iterator::StreamingIterator;
use rt_result::RtResult;
use dirs::{rusty_tags_cache_dir, rusty_tags_locks_dir};
use config::Config;

/// The tree describing the dependencies of the whole cargo project.
/// In the case of a cargo workspace, there's a separate 'DepTree'
/// for every member of the workspace.
#[derive(Debug)]
pub struct DepTree {
    pub source: Source,
    pub dependencies: Vec<Arc<DepTree>>
}

impl DepTree {
    /// The sources of the children of the 'DepTree'.
    pub fn children_sources(&self) -> Vec<&Source> {
        self.dependencies.iter()
            .map(|d| &d.source)
            .collect()
    }
}

/// A tree with its depth in the dependency hierarchy.
pub struct DepthWithTree<'a> {
    pub depth: usize,
    pub tree: &'a DepTree
}

/// Split the whole trees by their depth, starting with the highest depth,
/// Each unique 'DepTree' is returned once with its highest depth. Only
/// 'DepTree' are considered for which 'predicate' returns true.
pub fn split_by_depth<'a, P>(config: &Config,
                             roots: &'a Vec<Arc<DepTree>>,
                             predicate: P)
                             -> SplitByDepth<'a>
    where P: Fn(&DepTree) -> bool
{
    let mut dep_trees = Vec::with_capacity(100_000);

    type Depth = usize;
    type SourceHash = str;
    let mut max_depth = FnvHashMap::<&SourceHash, Depth>::default();

    for root in roots {
        collect(root, 0, &predicate, &mut max_depth, &mut dep_trees);
    }

    // sort first by source and then by higher depth
    dep_trees.sort_unstable_by(|a, b| {
        let ord = a.tree.source.hash.cmp(&b.tree.source.hash);
        if ord != Ordering::Equal {
            return ord;
        }

        b.depth.cmp(&a.depth)
    });

    // dedup to source with highest depth
    dep_trees.dedup_by_key(|i| &i.tree.source.hash);

    // sort sources by higher depth
    dep_trees.sort_unstable_by(|a, b| b.depth.cmp(&a.depth));

    if config.verbose {
        println!("Num sources: {}", dep_trees.len());
        println!("Source update order:");
        for DepthWithTree { depth, tree } in &dep_trees {
            println!("  {} '{}'", depth, tree.source.name);
        }
    }

    return SplitByDepth::new(dep_trees);

    fn collect<'a, P>(dep_tree: &'a DepTree,
                      depth: usize,
                      predicate: &P,
                      max_depth: &mut FnvHashMap<&'a SourceHash, Depth>,
                      dep_trees: &mut Vec<DepthWithTree<'a>>)
        where P: Fn(&DepTree) -> bool
    {
        if ! predicate(dep_tree) {
            return;
        }

        {
            let max_depth_entry = max_depth.entry(&dep_tree.source.hash).or_insert(0);
            if *max_depth_entry > depth {
                return;
            } else {
                *max_depth_entry = depth;
            }
        }

        dep_trees.push(DepthWithTree { depth, tree: dep_tree });

        for dep in &dep_tree.dependencies {
            collect(dep, depth + 1, predicate, max_depth, dep_trees);
        }
    }
}

/// Split the 'dep_trees' in continous regions of the same depth.
pub struct SplitByDepth<'a> {
    dep_trees: Vec<DepthWithTree<'a>>,

    first_idx: usize,
    cur_depth: usize,
    cur_idx: usize,
}

impl<'a> SplitByDepth<'a> {
    pub fn new(dep_trees: Vec<DepthWithTree<'a>>) -> SplitByDepth<'a> {
        SplitByDepth {
            first_idx: 0,
            cur_depth: if dep_trees.is_empty() { 0 } else { dep_trees[0].depth },
            cur_idx: 0,
            dep_trees: dep_trees
        }
    }
}

impl<'a> StreamingIterator for SplitByDepth<'a> {
    type Item = [DepthWithTree<'a>];

    #[inline]
    fn advance(&mut self) {
        if self.cur_idx > self.dep_trees.len() {
            return;
        }

        self.first_idx = self.cur_idx;
        self.cur_idx += 1;

        self.cur_depth = if self.first_idx < self.dep_trees.len() {
            self.dep_trees[self.first_idx].depth
        } else {
            0
        };

        while self.cur_idx < self.dep_trees.len()
                  && self.cur_depth == self.dep_trees[self.cur_idx].depth {
            self.cur_idx += 1;
        }
    }

    #[inline]
    fn get(&self) -> Option<&Self::Item> {
        if self.cur_idx > self.dep_trees.len() {
            return None;
        }

        Some(&self.dep_trees[self.first_idx .. self.cur_idx])
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceKind {
    /// The source of the cargo project. In the case
    /// of a cargo workspace, there's a 'Root' for
    /// every member of the workspace.
    Root,

    /// The source of a dependency.
    Dep
}

/// Lock a source to prevent that multiple running instances
/// of 'rusty-tags' update the same source.
pub enum SourceLock {
    Locked {
        path: PathBuf,
        file: File
    },

    AlreadyLocked {
        path: PathBuf
    }
}

impl SourceLock {
    fn new(source: &Source) -> RtResult<SourceLock> {
        let lock_file = rusty_tags_locks_dir()?.join(&source.hash);
        if lock_file.is_file() {
            Ok(SourceLock::AlreadyLocked { path: lock_file })
        } else {
            Ok(SourceLock::Locked {
                file: File::create(&lock_file)?,
                path: lock_file
            })
        }
    }
}

impl Drop for SourceLock {
    fn drop(&mut self) {
        match self {
            SourceLock::Locked { path, .. } => {
                if path.is_file() {
                    let _ = fs::remove_file(&path);
                }
            }

            SourceLock::AlreadyLocked { .. } => {}
        }
    }
}

#[derive(Clone, Debug)]
pub struct Source {
    pub kind: SourceKind,
    pub name: String,
    pub dir: PathBuf,
    pub hash: String,
    pub tags_file: PathBuf,
    pub cached_tags_file: PathBuf,
}

impl Source {
    pub fn new(kind: SourceKind, name: &str, dir: &Path, tags_spec: &TagsSpec) -> RtResult<Source> {
        let cargo_toml_dir = find_dir_upwards_containing("Cargo.toml", dir)?;
        let tags_file = cargo_toml_dir.join(tags_spec.file_name());
        let hash = hash(dir);
        let cached_tags_file = {
            let cache_dir = rusty_tags_cache_dir()?;
            let file_name = format!("{}-{}.{}", name, hash, tags_spec.file_extension());
            cache_dir.join(&file_name)
        };

        Ok(Source {
            kind: kind,
            name: name.to_owned(),
            dir: dir.to_owned(),
            hash: hash,
            tags_file: tags_file,
            cached_tags_file: cached_tags_file
        })
    }

    pub fn needs_tags_update(&self) -> bool {
        // Tags of the root (the cargo project) should be always recreated,
        // because we don't know which source file has been changed and
        // even if we would know it, we couldn't easily just replace the
        // tags of the changed source file.
        if self.kind == SourceKind::Root {
            return true;
        }

        ! self.cached_tags_file.is_file() || ! self.tags_file.is_file()
    }

    pub fn print_recreate_status(&self, config: &Config) {
        if config.force_recreate {
            println!("Forced recreating of tags for '{}'", self.name);
        } else if self.kind == SourceKind::Root {
            println!("Recreating tags for cargo project root '{}'", self.name);
        } else if ! self.cached_tags_file.is_file() {
            println!("Recreating tags for '{}', because of missing cache file at '{:?}'",
                     self.name, self.cached_tags_file);
        } else if ! self.tags_file.is_file() {
            println!("Recreating tags for '{}', because of missing tags file at '{:?}'",
                     self.name, self.tags_file);
        } else {
            println!("No recreation needed for '{}'", self.name);
        }
    }

    pub fn lock(&self) -> RtResult<SourceLock> {
        SourceLock::new(self)
    }
}

/// which kind of tags are created
arg_enum! {
    #[derive(Eq, PartialEq, Debug)]
    pub enum TagsKind {
        Vi,
        Emacs
    }
}

type ExeName = String;

/// which ctags executable is used
#[derive(Debug)]
pub enum TagsExe {
    ExuberantCtags(ExeName),
    UniversalCtags(ExeName)
}

/// holds additional info for the kind of tags, which extension
/// they use for caching and which user viewable file names they get
pub struct TagsSpec {
    pub kind: TagsKind,

    exe: TagsExe,

    /// the file name for vi tags
    vi_tags: String,

    /// the file name for emacs tags
    emacs_tags: String,

    /// options given to the ctags executable
    ctags_options: String
}

impl TagsSpec {
    pub fn new(kind: TagsKind, exe: TagsExe, vi_tags: String, emacs_tags: String, ctags_options: String) -> RtResult<TagsSpec> {
        if vi_tags == emacs_tags {
            return Err(format!("It's not supported to use the same tags name '{}' for vi and emacs!", vi_tags).into());
        }

        Ok(TagsSpec {
            kind: kind,
            exe: exe,
            vi_tags: vi_tags,
            emacs_tags: emacs_tags,
            ctags_options: ctags_options
        })
    }

    pub fn file_extension(&self) -> &'static str {
        match self.kind {
            TagsKind::Vi    => "vi",
            TagsKind::Emacs => "emacs"
        }
    }

    pub fn file_name(&self) -> &str {
        match self.kind {
            TagsKind::Vi    => &self.vi_tags,
            TagsKind::Emacs => &self.emacs_tags
        }
    }

    pub fn ctags_command(&self) -> Command {
        match self.exe {
            TagsExe::ExuberantCtags(ref exe_name) => {
                let mut cmd = Command::new(&exe_name);
                self.generic_ctags_options(&mut cmd);
                cmd.arg("--languages=Rust")
                   .arg("--langdef=Rust")
                   .arg("--langmap=Rust:.rs")
                   .arg("--regex-Rust=/^[ \\t]*(#\\[[^\\]]\\][ \\t]*)*(pub[ \\t]+)?(extern[ \\t]+)?(\"[^\"]+\"[ \\t]+)?(unsafe[ \\t]+)?fn[ \\t]+([a-zA-Z0-9_]+)/\\6/f,functions,function definitions/")
                   .arg("--regex-Rust=/^[ \\t]*(pub[ \\t]+)?type[ \\t]+([a-zA-Z0-9_]+)/\\2/T,types,type definitions/")
                   .arg("--regex-Rust=/^[ \\t]*(pub[ \\t]+)?enum[ \\t]+([a-zA-Z0-9_]+)/\\2/g,enum,enumeration names/")
                   .arg("--regex-Rust=/^[ \\t]*(pub[ \\t]+)?struct[ \\t]+([a-zA-Z0-9_]+)/\\2/s,structure names/")
                   .arg("--regex-Rust=/^[ \\t]*(pub[ \\t]+)?mod[ \\t]+([a-zA-Z0-9_]+)\\s*\\{/\\2/m,modules,module names/")
                   .arg("--regex-Rust=/^[ \\t]*(pub[ \\t]+)?(static|const)[ \\t]+([a-zA-Z0-9_]+)/\\3/c,consts,static constants/")
                   .arg("--regex-Rust=/^[ \\t]*(pub[ \\t]+)?trait[ \\t]+([a-zA-Z0-9_]+)/\\2/t,traits,traits/")
                   .arg("--regex-Rust=/^[ \\t]*macro_rules![ \\t]+([a-zA-Z0-9_]+)/\\1/d,macros,macro definitions/");

                cmd
            }

            TagsExe::UniversalCtags(ref exe_name) => {
                let mut cmd = Command::new(&exe_name);
                self.generic_ctags_options(&mut cmd);
                cmd.arg("--languages=Rust");

                cmd
            }
        }
    }

    fn generic_ctags_options(&self, cmd: &mut Command) {
        match self.kind {
            TagsKind::Vi    => {}
            TagsKind::Emacs => { cmd.arg("-e"); }
        }

        cmd.arg("--recurse");
        if ! self.ctags_options.is_empty() {
            cmd.arg(&self.ctags_options);
        }
    }
}

fn hash(path: &Path) -> String {
    let mut hasher = FnvHasher::default();
    path.hash(&mut hasher);
    hasher.finish().to_string()
}

fn find_dir_upwards_containing(file_name: &str, start_dir: &Path) -> RtResult<PathBuf> {
    let mut dir = start_dir.to_path_buf();
    loop {
        if let Ok(files) = fs::read_dir(&dir) {
            for path in files.map(|r| r.map(|d| d.path())) {
                match path {
                    Ok(ref path) if path.is_file() =>
                        match path.file_name() {
                            Some(name) if name.to_str() == Some(file_name) => return Ok(dir),
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
