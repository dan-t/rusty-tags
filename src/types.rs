use std::path::{Path, PathBuf};
use std::fs::{self, File};
use std::hash::{Hash, Hasher};
use std::process::Command;
use std::cmp::Ordering;
use std::ops::{Drop, Deref};
use std::fmt;

use fnv::{FnvHasher, FnvHashMap};
use semver::Version;
use streaming_iterator::StreamingIterator;
use rt_result::RtResult;
use dirs::{rusty_tags_cache_dir, rusty_tags_locks_dir};
use config::Config;

type Depth = usize;

/// The tree describing the dependencies of the whole cargo project.
#[derive(Debug)]
pub struct DepTree {
    roots: Vec<SourceId>,
    sources: Vec<Option<Source>>,
    dependencies: Vec<Option<Vec<SourceId>>>
}

impl DepTree {
    pub fn new() -> DepTree {
        DepTree {
            roots: Vec::with_capacity(10),
            sources: Vec::new(),
            dependencies: Vec::new()
        }
    }

    pub fn reserve_num_sources(&mut self, num: usize) {
        self.sources.reserve(num);
        self.dependencies.reserve(num);
    }

    pub fn roots(&self) -> Sources {
        Sources::new(&self.sources, Some(&self.roots))
    }

    pub fn dependencies(&self, source: &Source) -> Sources {
        Sources::new(&self.sources, self.dependencies_slice(source))
    }

    /// Split the whole tree by its depth, starting with the biggest depth.
    /// Each unique 'Source' is returned once with its biggest depth.
    pub fn split_by_depth(&self) -> SplitByDepth {
        type Depth = usize;
        let mut dep_graph = Vec::with_capacity(100);
        let mut max_depth = FnvHashMap::<SourceId, Depth>::default();
        let mut sources = Vec::with_capacity(100_000);
        for root in self.roots() {
            self.collect(root, 0, &mut dep_graph, &mut max_depth, &mut sources);
        }

        // sort first by source id and then by bigger depth
        sources.sort_unstable_by(|a, b| {
            let ord = a.source.id.cmp(&b.source.id);
            if ord != Ordering::Equal {
                return ord;
            }

            b.depth.cmp(&a.depth)
        });

        // dedup to sources with biggest depth
        sources.dedup_by_key(|i| &i.source.id);

        // sort sources by bigger depth
        sources.sort_unstable_by(|a, b| b.depth.cmp(&a.depth));

        SplitByDepth::new(sources)
    }

    pub fn new_source(&mut self) -> SourceId {
        let id = self.sources.len();
        self.sources.push(None);
        self.dependencies.push(None);
        SourceId { id }
    }

    pub fn add_root(&mut self, id: SourceId) {
        self.roots.push(id);
    }

    pub fn set_roots(&mut self, ids: Vec<SourceId>) {
        self.roots = ids;
    }

    pub fn set_source(&mut self, src: Source, dependencies: Vec<SourceId>) {
        let id = *src.id;
        self.sources[id] = Some(src);
        if ! dependencies.is_empty() {
            self.dependencies[id] = Some(dependencies);
        }
    }

    fn dependencies_slice(&self, source: &Source) -> Option<&[SourceId]> {
        self.dependencies[*source.id].as_ref().map(Vec::as_slice)
    }

    fn collect<'a>(&'a self, source: &'a Source, depth: usize,
                   dep_graph: &mut Vec<SourceId>,
                   max_depth: &mut FnvHashMap<SourceId, Depth>,
                   sources: &mut Vec<SourceWithDepth<'a>>) {

        {
            let max_depth_entry = max_depth.entry(source.id).or_insert(0);

            // the source was already found deeper in the dependency hierarchy
            if *max_depth_entry > depth {
                return;
            } else {
                *max_depth_entry = depth;
            }
        }

        // cyclic dependency detected
        if let Some(_) = dep_graph.iter().find(|d| **d == source.id) {
            return;
        }

        sources.push(SourceWithDepth { source, depth });
        dep_graph.push(source.id);

        for dep in self.dependencies(source) {
            self.collect(dep, depth + 1, dep_graph, max_depth, sources);
        }

        dep_graph.pop();
    }
}

#[derive(Clone)]
pub struct Sources<'a> {
    sources: &'a [Option<Source>],
    source_ids: Option<&'a [SourceId]>,
    idx: usize
}

impl<'a> Sources<'a> {
    fn new(sources: &'a [Option<Source>], source_ids: Option<&'a [SourceId]>) -> Sources<'a> {
        Sources { sources, source_ids, idx: 0 }
    }
}

impl<'a> Iterator for Sources<'a> {
    type Item = &'a Source;

     fn next(&mut self) -> Option<Self::Item> {
         if let Some(source_ids) = self.source_ids {
             if self.idx >= source_ids.len() {
                 None
             } else {
                 let id = source_ids[self.idx];
                 let src = self.sources[*id].as_ref();
                 self.idx += 1;
                 src
             }
         } else {
             None
         }
     }
}

pub struct SourceWithDepth<'a> {
    pub source: &'a Source,
    pub depth: usize,
}

/// Split the 'sources' in continous regions of the same depth.
pub struct SplitByDepth<'a> {
    sources: Vec<SourceWithDepth<'a>>,

    first_idx: usize,
    cur_depth: usize,
    cur_idx: usize,
}

impl<'a> SplitByDepth<'a> {
    pub fn new(sources: Vec<SourceWithDepth<'a>>) -> SplitByDepth<'a> {
        SplitByDepth {
            first_idx: 0,
            cur_depth: if sources.is_empty() { 0 } else { sources[0].depth },
            cur_idx: 0,
            sources: sources
        }
    }
}

impl<'a> StreamingIterator for SplitByDepth<'a> {
    type Item = [SourceWithDepth<'a>];

    #[inline]
    fn advance(&mut self) {
        if self.cur_idx > self.sources.len() {
            return;
        }

        self.first_idx = self.cur_idx;
        self.cur_idx += 1;

        self.cur_depth = if self.first_idx < self.sources.len() {
            self.sources[self.first_idx].depth
        } else {
            0
        };

        while self.cur_idx < self.sources.len()
                  && self.cur_depth == self.sources[self.cur_idx].depth {
            self.cur_idx += 1;
        }
    }

    #[inline]
    fn get(&self) -> Option<&Self::Item> {
        if self.cur_idx > self.sources.len() {
            return None;
        }

        Some(&self.sources[self.first_idx .. self.cur_idx])
    }
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
    fn new(source: &Source, tags_spec: &TagsSpec) -> RtResult<SourceLock> {
        let file_name = format!("{}-{}.{}", source.name, source.hash, tags_spec.file_extension());
        let lock_file = rusty_tags_locks_dir()?.join(file_name);
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
    pub id: SourceId,
    pub name: String,
    pub dir: PathBuf,
    pub hash: String,
    pub is_root: bool,
    pub tags_file: PathBuf,
    pub cached_tags_file: PathBuf,
}

impl Source {
    pub fn new(id: SourceId, name: &str, dir: &Path, is_root: bool, tags_spec: &TagsSpec) -> RtResult<Source> {
        let cargo_toml_dir = find_dir_upwards_containing("Cargo.toml", dir)?;
        let tags_file = cargo_toml_dir.join(tags_spec.file_name());
        let hash = source_hash(dir);
        let cached_tags_file = {
            let cache_dir = rusty_tags_cache_dir()?;
            let file_name = format!("{}-{}.{}", name, hash, tags_spec.file_extension());
            cache_dir.join(&file_name)
        };

        Ok(Source {
            id: id,
            name: name.to_owned(),
            dir: dir.to_owned(),
            hash: hash,
            is_root: is_root,
            tags_file: tags_file,
            cached_tags_file: cached_tags_file
        })
    }

    pub fn needs_tags_update(&self) -> bool {
        // Tags of the root (the cargo project) should be always recreated,
        // because we don't know which source file has been changed and
        // even if we would know it, we couldn't easily just replace the
        // tags of the changed source file.
        if self.is_root {
            return true;
        }

        ! self.cached_tags_file.is_file() || ! self.tags_file.is_file()
    }

    pub fn print_recreate_status(&self, config: &Config) {
        if config.force_recreate {
            println!("Forced recreating of tags for '{}'", self.name);
        } else if self.is_root {
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

    pub fn lock(&self, tags_spec: &TagsSpec) -> RtResult<SourceLock> {
        SourceLock::new(self, tags_spec)
    }
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Hash, Clone, Copy, Debug)]
pub struct SourceId {
    id: usize
}

impl Deref for SourceId {
    type Target = usize;

    fn deref(&self) -> &Self::Target {
        &self.id
    }
}

#[derive(PartialEq, Eq, Clone, PartialOrd, Ord, Hash)]
pub struct SourceVersion<'a> {
    pub name: &'a str,
    pub version: Version
}

impl<'a> SourceVersion<'a> {
    pub fn new(name: &'a str, version: Version) -> SourceVersion<'a> {
        SourceVersion { name, version }
    }

    pub fn parse_from_id(id: &'a str) -> RtResult<SourceVersion<'a>> {
        let mut split = id.split(' ');
        let name = split.next();
        if name == None {
            return Err(format!("Couldn't extract name from id: '{}'", id).into());
        }
        let name = name.unwrap();

        let version = split.next();
        if version == None {
            return Err(format!("Couldn't extract version from id: '{}'", id).into());
        }
        let version = version.unwrap();

        Ok(SourceVersion::new(name, Version::parse(version)?))
    }
}

impl<'a> fmt::Debug for SourceVersion<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "({}, {})", self.name, self.version)
    }
}

impl<'a> fmt::Display for SourceVersion<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "({}, {})", self.name, self.version)
    }
}

fn source_hash(source_dir: &Path) -> String {
    let mut hasher = FnvHasher::default();
    source_dir.hash(&mut hasher);
    hasher.finish().to_string()
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
