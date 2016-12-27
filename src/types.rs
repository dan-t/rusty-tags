use std::fmt::{Debug, Display, Formatter, Error};
use std::path::{Path, PathBuf};
use std::process::Command;
use rt_result::RtResult;

use dirs::{
    cargo_git_src_dir,
    cargo_crates_io_src_dir,
    rusty_tags_cache_dir,
    glob_path
};

/// the tree describing the dependencies of the whole cargo project
#[derive(Debug)]
pub struct DepTree {
    pub source: SourceKind,
    pub dependencies: Vec<Box<DepTree>>
}

impl DepTree {
    pub fn direct_dep_sources(&self) -> Vec<SourceKind> {
        let mut srcs = Vec::new();
        for dep in &self.dependencies {
            srcs.push(dep.source.clone());
        }

        srcs
    }
}

#[derive(Clone)]
pub enum SourceKind {
    /// the source of the cargo project
    Root {
        path: PathBuf
    },

    /// the source of a dependency from a git repository
    Git {
        lib_name: String,
        commit_hash: String
    },

    /// the source of a dependency from crates.io
    CratesIo {
        lib_name: String,
        version: String
    },

    /// the source of a dependency from a local directory
    Path {
        lib_name: String,
        path: PathBuf
    }
}

impl SourceKind {
    pub fn tags_files(&self, tags_spec: &TagsSpec) -> RtResult<TagsFiles> {
        let src_root_dir = try!(self.src_root_dir());
        Ok(TagsFiles {
            cached_tags_file: try!(self.cached_tags_file(tags_spec)),
            src_tags_file: src_root_dir.join(tags_spec.file_name()),
            src_dir: src_root_dir.join("src")
        })
    }

    pub fn get_lib_name(&self) -> &str {
        match *self {
            SourceKind::Root { .. } => {
                "project root"
            }

            SourceKind::Git { ref lib_name, .. } => {
                lib_name
            }

            SourceKind::CratesIo { ref lib_name, .. } => {
                lib_name
            }

            SourceKind::Path { ref lib_name, .. } => {
                lib_name
            }
        }
    }

    pub fn is_root(&self) -> bool {
        match *self {
            SourceKind::Root { .. } => true,
            _ => false
        }
    }

    fn cached_tags_file(&self, tags_spec: &TagsSpec) -> RtResult<Option<PathBuf>> {
        if let Some(name) = self.cached_tags_file_name(tags_spec) {
            Ok(Some(try!(rusty_tags_cache_dir()).join(name)))
        } else {
            Ok(None)
        }
    }

    /// find the root source directory, for git sources the directories
    /// in `~/.cargo/git/checkouts` are considered and for crates.io sources
    /// the directories in `~/.cargo/registry/src/github.com-*` are considered
    fn src_root_dir(&self) -> RtResult<PathBuf> {
        match *self {
            SourceKind::Git { ref lib_name, ref commit_hash } => {
                let mut lib_src = lib_name.clone();
                lib_src.push_str("-*");

                let src_dir = try!(cargo_git_src_dir())
                    .join(&lib_src)
                    .join("master");

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
                let src_dir = try!(cargo_git_src_dir())
                    .join("*")
                    .join("master");

                let src_paths = try!(glob_path(&format!("{}", src_dir.display())));
                for src_path in src_paths {
                    if let Ok(path) = src_path {
                        let src_commit_hash = try!(get_commit_hash(&path));
                        if *commit_hash == src_commit_hash {
                            return Ok(path);
                        }
                    }
                }

                Err(self.into())
            },

            SourceKind::CratesIo { ref lib_name, ref version } => {
                let mut lib_src = lib_name.clone();
                lib_src.push('-');
                lib_src.push_str(&**version);

                let src_dir = try!(cargo_crates_io_src_dir())
                    .join(&lib_src);

                if ! src_dir.is_dir() {
                    return Err(self.into());
                }

                Ok(src_dir)
            },

            SourceKind::Path { ref path, .. } => {
                if ! path.is_dir() {
                    return Err(self.into());
                }

                Ok(path.clone())
            }

            SourceKind::Root { ref path } => {
                Ok(path.clone())
            }
        }
    }

    fn cached_tags_file_name(&self, tags_spec: &TagsSpec) -> Option<String> {
        match *self {
            SourceKind::Git { ref lib_name, ref commit_hash } => {
                Some(format!("{}-{}.{}", lib_name, commit_hash, tags_spec.file_extension()))
            }

            SourceKind::CratesIo { ref lib_name, ref version } => {
                Some(format!("{}-{}.{}", lib_name, version, tags_spec.file_extension()))
            }

            SourceKind::Path { .. } | SourceKind::Root { .. } => None
        }
    }

    fn display(&self, f: &mut Formatter) -> Result<(), Error> {
        match *self {
            SourceKind::Root { ref path } => {
                write!(f, "project root: {}", path.display())
            }

            SourceKind::Git { ref lib_name, ref commit_hash } => {
                write!(f, "{}-{}", lib_name, commit_hash)
            }

            SourceKind::CratesIo { ref lib_name, ref version } => {
                write!(f, "{}-{}", lib_name, version)
            }

            SourceKind::Path { ref lib_name, ref path } => {
                write!(f, "{}: {}", lib_name, path.display())
            }
        }
    }
}

impl Debug for SourceKind {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        self.display(f)
    }
}

impl Display for SourceKind {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        self.display(f)
    }
}

pub struct TagsFiles {
    pub cached_tags_file: Option<PathBuf>,
    pub src_tags_file: PathBuf,
    pub src_dir: PathBuf
}

impl TagsFiles {
    pub fn are_files(&self) -> bool {
        if let Some(ref file) = self.cached_tags_file {
            if ! file.is_file() {
                return false;
            }
        }

        self.src_tags_file.is_file()
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

/// get the commit hash of the current `HEAD` of the git repository located at `git_dir`
fn get_commit_hash(git_dir: &Path) -> RtResult<String> {
    let mut cmd = Command::new("git");
    cmd.current_dir(git_dir)
        .arg("rev-parse")
        .arg("HEAD");

    let out = try!(cmd.output()
        .map_err(|err| format!("git execution failed: {}", err)));

    String::from_utf8(out.stdout)
        .map(|s| s.trim().to_string())
        .map_err(|_| "Couldn't convert 'git rev-parse HEAD' output to utf8!".into())
}
