use std::path::{Path, PathBuf};
use std::fs;
use std::hash::{Hash, Hasher};
use std::collections::hash_map::DefaultHasher;

use rt_result::RtResult;
use dirs::rusty_tags_cache_dir;

/// the tree describing the dependencies of the whole cargo project
#[derive(Debug)]
pub struct DepTree {
    pub source: Source,
    pub dependencies: Vec<Box<DepTree>>
}

impl DepTree {
    pub fn direct_dep_sources(&self) -> Vec<&Source> {
        self.dependencies.iter()
            .map(|d| &d.source)
            .collect()
    }

    pub fn deps_by_depth(&self, which: WhichDep) -> Vec<Vec<&DepTree>> {
        let mut deps = Vec::with_capacity(50);
        self.deps_by_depth_internal(0, which, &mut deps);
        deps.into_iter().rev().collect()
    }

    fn deps_by_depth_internal<'a>(&'a self, depth: usize, which: WhichDep, deps: &mut Vec<Vec<&'a DepTree>>) {
        if which == WhichDep::NeedsTagsUpdate && ! self.source.needs_tags_update() {
            return;
        }

        if deps.len() <= depth {
            deps.push(Vec::with_capacity(50));
        }

        deps[depth].push(&self);

        for dep in &self.dependencies {
            dep.deps_by_depth_internal(depth + 1, which, deps);
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WhichDep {
    All,
    NeedsTagsUpdate
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceKind {
    /// the source of the cargo project
    Root,

    /// the source of a dependency
    Dep
}

#[derive(Clone, Debug)]
pub struct Source {
    pub kind: SourceKind,
    pub name: String,
    pub dir: PathBuf,
    pub tags_file: PathBuf,
    pub cached_tags_file: Option<PathBuf>
}

impl Source {
    pub fn new(kind: SourceKind, name: &str, dir: &Path, tags_spec: &TagsSpec) -> RtResult<Source> {
        let cargo_toml_dir = find_dir_upwards_containing("Cargo.toml", dir)?;
        let tags_file = cargo_toml_dir.join(tags_spec.file_name());

        let cached_tags_file = if kind == SourceKind::Dep {
            let cache_dir = rusty_tags_cache_dir()?;
            Some(cache_dir.join(format!("{}-{}.{}", name, hash(dir), tags_spec.file_extension())))
        } else {
            None
        };

        Ok(Source {
            kind: kind,
            name: name.to_owned(),
            dir: dir.to_owned(),
            tags_file: tags_file,
            cached_tags_file: cached_tags_file
        })
    }

    pub fn needs_tags_update(&self) -> bool {
        // tags of the root (the cargo project) should be always rebuild
        if self.kind == SourceKind::Root {
            return true;
        }

        if let Some(ref file) = self.cached_tags_file {
            if ! file.is_file() {
                return true;
            }
        }

        ! self.tags_file.is_file()
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

/// holds additional info for the kind of tags, which extension
/// they use for caching and which user viewable file names they get
pub struct TagsSpec {
    pub kind: TagsKind,

    /// the file name for vi tags
    vi_tags: String,

    /// the file name for emacs tags
    emacs_tags: String
}

impl TagsSpec {
    pub fn new(kind: TagsKind, vi_tags: String, emacs_tags: String) -> RtResult<TagsSpec> {
        if vi_tags == emacs_tags {
            return Err(format!("It's not supported to use the same tags name '{}' for vi and emacs!", vi_tags).into());
        }

        Ok(TagsSpec {
            kind: kind,
            vi_tags: vi_tags,
            emacs_tags: emacs_tags
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

    pub fn ctags_option(&self) -> Option<&'static str> {
        match self.kind {
            TagsKind::Vi    => None,
            TagsKind::Emacs => Some("-e")
        }
    }
}

fn hash(path: &Path) -> String {
    let mut hasher = DefaultHasher::new();
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
