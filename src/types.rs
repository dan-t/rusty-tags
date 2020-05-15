use std::path::{Path, PathBuf};
use std::fs::{self, File};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::process::Command;
use std::ops::{Drop, Deref};
use std::fmt;

use semver::Version;
use rt_result::RtResult;
use dirs::{rusty_tags_cache_dir, rusty_tags_locks_dir};
use config::Config;
use tempfile::NamedTempFile;

/// The tree describing the dependencies of the whole cargo project.
#[derive(Debug)]
pub struct DepTree {
    /// the roots, the workspace members of the cargo project,
    /// the source ids are indices into 'sources'
    roots: Vec<SourceId>,

    /// all sources of the cargo project, the roots and all direct
    /// and indirect dependencies
    sources: Vec<Option<Source>>,

    /// the dependencies of each source in 'sources', the source
    /// ids are indices into 'sources'
    dependencies: Vec<Option<Vec<SourceId>>>,

    /// the parents - the dependent sources - of each
    /// source in 'sources', the source ids are indices into
    /// 'sources'
    parents: Vec<Option<Vec<SourceId>>>
}

impl DepTree {
    pub fn new() -> DepTree {
        DepTree {
            roots: Vec::with_capacity(10),
            sources: Vec::new(),
            dependencies: Vec::new(),
            parents: Vec::new()
        }
    }

    pub fn reserve_num_sources(&mut self, num: usize) {
        self.sources.reserve(num);
        self.dependencies.reserve(num);
        self.parents.reserve(num);
    }

    pub fn roots(&self) -> Sources {
        Sources::new(&self.sources, Some(&self.roots))
    }

    pub fn dependencies(&self, source: &Source) -> Sources {
        Sources::new(&self.sources, self.dependencies_slice(source))
    }

    pub fn all_sources<'a>(&'a self) -> Box<dyn Iterator<Item=&Source> + 'a> {
        Box::new(self.sources
                     .iter()
                     .filter_map(|s| s.as_ref()))
    }

    /// Get all of the ancestors of 'sources' till the roots.
    pub fn ancestors<'a>(&'a self, sources: &[&Source]) -> Vec<&'a Source> {
        let mut ancestor_srcs = Vec::with_capacity(50000);
        let mut dep_graph = Vec::with_capacity(100);
        for src in sources {
            self.ancestors_internal(src, &mut ancestor_srcs, &mut dep_graph);
        }

        unique_sources(&mut ancestor_srcs);
        ancestor_srcs
    }

    /// Reserve space for a new source and return its source id.
    pub fn new_source(&mut self) -> SourceId {
        let id = self.sources.len();
        self.sources.push(None);
        self.dependencies.push(None);
        self.parents.push(None);
        SourceId { id }
    }

    pub fn set_roots(&mut self, ids: Vec<SourceId>) {
        self.roots = ids;
    }

    pub fn set_source(&mut self, src: Source, dependencies: Vec<SourceId>) {
        let src_id = src.id;
        self.sources[*src_id] = Some(src);
        if dependencies.is_empty() {
            return;
        }

        for dep in &dependencies {
            let dep_id: usize = **dep;
            if self.parents[dep_id].is_none() {
                self.parents[dep_id] = Some(Vec::with_capacity(10));
            }

            if let Some(ref mut parents) = self.parents[dep_id] {
                parents.push(src_id);
            }
        }

        self.dependencies[*src_id] = Some(dependencies);
    }

    fn dependencies_slice(&self, source: &Source) -> Option<&[SourceId]> {
        self.dependencies[*source.id].as_ref().map(Vec::as_slice)
    }

    fn ancestors_internal<'a>(&'a self, source: &Source,
                              ancestor_srcs: &mut Vec<&'a Source>,
                              dep_graph: &mut Vec<SourceId>) {
        dep_graph.push(source.id);
        if let Some(ref parents) = self.parents[*source.id] {
            for p_id in parents {
                // cyclic dependency detected
                if dep_graph.iter().find(|id| *id == p_id) != None {
                    continue;
                }

                if let Some(ref p) = self.sources[**p_id] {
                    ancestor_srcs.push(p);
                    self.ancestors_internal(p, ancestor_srcs, dep_graph);
                }
            }
        }

        dep_graph.pop();
    }
}

/// An iterator over sources by their source ids.
#[derive(Clone)]
pub struct Sources<'a> {
    /// all sources
    sources: &'a [Option<Source>],

    /// the sources to iterate over, 'source_ids'
    /// are indices into 'sources'
    source_ids: Option<&'a [SourceId]>,

    /// the current index into 'source_ids'
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

/// Lock a source to prevent that multiple running instances
/// of 'rusty-tags' update the same source at once.
///
/// This is only an optimization and not needed for correctness,
/// because the tags are written to a temporary file which is then
/// moved to its final place. It's ensured that the moving happens
/// on the same partition/file system and therefore the move is
/// an atomic operation which can't be affected by an other
/// running instance of 'rusty-tags'. So multiple running
/// 'rusty-tags' can't write at once to the same file.
pub enum SourceLock {
    /// this running instance of 'rusty-tags' holds the lock
    Locked {
        path: PathBuf,
        file: File
    },

    /// an other instance of 'rusty-tags' holds the lock,
    /// or the other instance couldn't cleanup the lock correctly
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
        match *self {
            SourceLock::Locked { ref path, .. } => {
                if path.is_file() {
                    let _ = fs::remove_file(&path);
                }
            }

            SourceLock::AlreadyLocked { .. } => {}
        }
    }
}

#[derive(Debug)]
pub struct Source {
    /// rusty-tags specific internal id of the source
    pub id: SourceId,

    /// the 'Cargo.toml' name of the source
    pub name: String,

    /// the 'Cargo.toml' version of the source
    pub version: Version,

    /// the root source directory
    pub dir: PathBuf,

    /// hash of 'dir'
    pub hash: String,

    /// if the source is a root of the dependency tree,
    /// which means that it's a workspace member
    pub is_root: bool,

    /// path to the tags file in the source directory,
    /// beside of the 'Cargo.toml' file, this tags file
    /// contains of the tags of the source and of its
    /// dependencies
    pub tags_file: PathBuf,

    /// path to the tags file in the rusty-tags cache directory,
    /// this tags file contains the tags of the source and
    /// only the tags of the dependencies that have a public
    /// export from the source
    pub cached_tags_file: PathBuf,
}

impl Source {
    pub fn new(id: SourceId, source_version: &SourceVersion, dir: &Path, is_root: bool, config: &Config) -> RtResult<Source> {
        let tags_dir = find_dir_upwards_containing("Cargo.toml", dir).unwrap_or(dir.to_path_buf());
        let tags_file = tags_dir.join(config.tags_spec.file_name());
        let hash = source_hash(dir);
        let cached_tags_file = {
            let cache_dir = rusty_tags_cache_dir()?;
            let file_name = format!("{}-{}.{}", source_version.name, hash, config.tags_spec.file_extension());
            cache_dir.join(&file_name)
        };

        Ok(Source {
            id: id,
            name: source_version.name.to_owned(),
            version: source_version.version.clone(),
            dir: dir.to_owned(),
            hash: hash,
            is_root: is_root,
            tags_file: tags_file,
            cached_tags_file: cached_tags_file
        })
    }

    pub fn needs_tags_update(&self, config: &Config) -> bool {
        if config.force_recreate {
            return true;
        }

        // Tags of the root (the cargo project) should be always recreated,
        // because we don't know which source file has been changed and
        // even if we would know it, we couldn't easily just replace the
        // tags of the changed source file.
        if self.is_root {
            return true;
        }

        ! self.cached_tags_file.is_file() || ! self.tags_file.is_file()
    }

    pub fn recreate_status(&self, config: &Config) -> String {
        if config.force_recreate {
            format!("Forced recreating of tags for {}", self.source_version())
        } else if self.is_root {
            format!("Recreating tags for cargo project root {}", self.source_version())
        } else if ! self.cached_tags_file.is_file() {
            format!("Recreating tags for {}, because of missing cache file at '{:?}'",
                     self.source_version(), self.cached_tags_file)
        } else if ! self.tags_file.is_file() {
            format!("Recreating tags for {}, because of missing tags file at '{:?}'",
                     self.source_version(), self.tags_file)
        } else {
            format!("Recreating tags for {}, because one of its dependencies was updated",
                    self.source_version())
        }
    }

    pub fn lock(&self, tags_spec: &TagsSpec) -> RtResult<SourceLock> {
        SourceLock::new(self, tags_spec)
    }

    fn source_version(&self) -> String {
        format!("({}, {})", self.name, self.version)
    }
}

/// Temporary struct for the tags updating of the source. It's
/// used to create and associate a temporary file to the source
/// for its tags creation.
pub struct SourceWithTmpTags<'a> {
    /// the source to update
    pub source: &'a Source,

    /// temporary file for the tags of the source
    pub tags_file: NamedTempFile,
}

impl<'a> SourceWithTmpTags<'a> {
    pub fn new(source: &'a Source) -> RtResult<SourceWithTmpTags<'a>> {
        Ok(SourceWithTmpTags {
            source,
            tags_file: NamedTempFile::new()?
        })
    }
}

/// An unique runtime specific 'rusty-tags' internal id
/// of the source.
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

/// A temporary struct used for the reading of the result of 'cargo metadata'.
#[derive(PartialEq, Eq, Clone, PartialOrd, Ord, Hash)]
pub struct SourceVersion<'a> {
    /// the 'Cargo.toml' name of the source
    pub name: &'a str,

    /// the 'Cargo.toml' version of the source
    pub version: Version
}

impl<'a> SourceVersion<'a> {
    pub fn new(name: &'a str, version: Version) -> SourceVersion<'a> {
        SourceVersion { name, version }
    }

    /// Parses an id from 'cargo metadata' (e.g "dtoa 0.4.2 (registry+https://github.com/rust-lang/crates.io-index)")
    /// into a 'SourceVersion'.
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
    let mut hasher = DefaultHasher::new();
    source_dir.hash(&mut hasher);
    hasher.finish().to_string()
}

// which kind of tags are created
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
                   .arg("--regex-Rust=/^[ \\t]*(pub[ \\t]+)?(unsafe[ \\t]+)?trait[ \\t]+([a-zA-Z0-9_]+)/\\3/t,traits,traits/")
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

pub fn unique_sources(sources: &mut Vec<&Source>) {
    sources.sort_unstable_by(|a, b| a.id.cmp(&b.id));
    sources.dedup_by_key(|s| &s.id);
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
