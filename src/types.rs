use std::fmt::{Debug, Display, Formatter, Error};
use std::path::PathBuf;

use path_ext::PathExt;

/// For every `TagsRoot` a `rusty-tags.{vi,emacs}` file will be created.
///
/// `Proj` is the tags root of the current cargo project. Its tags file will contain the tags of
/// the source code of the cargo project and of its direct dependencies. The tags file will be
/// placed at the root of the cargo project, beside of the `Cargo.toml`.
///
/// `Lib` represents a direct or indirect (a dependency of a dependency) dependency of the cargo
/// project. For each dependency a tags file will be created containing the tags of the source
/// code of the dependency and its direct dependecies. The tags file will be placed at the root of
/// the source code of the dependency.
pub enum TagsRoot
{
   /// the source directory and the dependencies
   /// of the cargo project
   Proj {
      src_dir: PathBuf,
      dependencies: Vec<SourceKind>
   },

   /// a library and its dependencies
   Lib {
      src_kind: SourceKind,
      dependencies: Vec<SourceKind>
   }
}

pub type TagsRoots = Vec<TagsRoot>;

impl Debug for TagsRoot
{
   fn fmt(&self, f: &mut Formatter) -> Result<(), Error>
   {
      match *self {
         TagsRoot::Proj { ref src_dir, ref dependencies } => {
            write!(f, "Src ( src_dir: {}, dependencies: {:?} )", src_dir.display(), dependencies)
         },

         TagsRoot::Lib { ref src_kind, ref dependencies } => {
            write!(f, "Lib ( src_kind: {}, dependencies: {:?} )", src_kind, dependencies)
         }
      }
   }
}

/// Where the source code of a dependency is from. From a git repository, from `crates.io` or from
/// a local path.
#[derive(Clone)]
pub enum SourceKind
{
   /// the source is from a git repository
   Git {
      lib_name: String,
      commit_hash: String
   },

   /// the source is from crates.io
   CratesIo {
      lib_name: String,
      version: String
   },

   /// the source is from a local directory
   Path {
      lib_name: String,
      path: PathBuf
   }
}

impl SourceKind
{
   pub fn tags_file_name(&self, tags_kind: &TagsKind) -> String
   {
      match *self {
         SourceKind::Git { ref lib_name, ref commit_hash } => {
            format!("{}-{}.{}", lib_name, commit_hash, tags_kind.tags_file_extension())
         },

         SourceKind::CratesIo { ref lib_name, ref version } => {
            format!("{}-{}.{}", lib_name, version, tags_kind.tags_file_extension())
         },

         SourceKind::Path { .. } => {
            tags_kind.tags_file_name().to_owned()
         }
      }
   }

   pub fn get_lib_name(&self) -> String
   {
      match *self {
         SourceKind::Git { ref lib_name, .. } => {
            lib_name.clone()
         },

         SourceKind::CratesIo { ref lib_name, .. } => {
            lib_name.clone()
         },

         SourceKind::Path { ref lib_name, .. } => {
            lib_name.clone()
         }
      }
   }

   fn display(&self, f: &mut Formatter) -> Result<(), Error>
   {
      match *self {
         SourceKind::Git { ref lib_name, ref commit_hash } => {
            write!(f, "{}-{}", lib_name, commit_hash)
         },

         SourceKind::CratesIo { ref lib_name, ref version } => {
            write!(f, "{}-{}", lib_name, version)
         },

         SourceKind::Path { ref lib_name, ref path } => {
            write!(f, "{}: {}", lib_name, path.display())
         }
      }
   }
}

impl Debug for SourceKind
{
   fn fmt(&self, f: &mut Formatter) -> Result<(), Error>
   {
      self.display(f)
   }
}

impl Display for SourceKind
{
   fn fmt(&self, f: &mut Formatter) -> Result<(), Error>
   {
      self.display(f)
   }
}

pub struct Tags
{
   /// the root directory of the source code
   /// for which the tags have been created
   pub src_dir: PathBuf,

   /// the tags file of the sources in `src_dir`
   pub tags_file: PathBuf,

   /// indicates if the tags file is already existing
   /// and the cached tags file is returned
   cached: bool
}

impl Tags
{
   pub fn new(src_dir: &PathBuf, tags_file: &PathBuf, cached: bool) -> Tags
   {
      Tags { src_dir: src_dir.clone(), tags_file: tags_file.clone(), cached: cached }
   }

   pub fn is_up_to_date(&self, tags_kind: &TagsKind) -> bool
   {
      if ! self.cached {
         return false;
      }

      let mut src_tags = self.src_dir.clone();
      src_tags.push(tags_kind.tags_file_name());

      src_tags.as_path().is_file()
   }
}

impl Debug for Tags
{
   fn fmt(&self, f: &mut Formatter) -> Result<(), Error>
   {
      write!(f, "Tags ( src_dir: {}, tags_file: {}, cached: {} )",
             self.src_dir.display(), self.tags_file.display(), self.cached)
   }
}

/// which kind of tags are created
arg_enum!{
   #[derive(Eq, PartialEq, Debug)]
   pub enum TagsKind
   {
      Vi,
      Emacs
   }
}

impl TagsKind
{
   pub fn tags_file_extension(&self) -> &'static str
   {
      match *self {
         TagsKind::Vi    => "vi",
         TagsKind::Emacs => "emacs"
      }
   }

   /// the name under which the tags files are saved
   pub fn tags_file_name(&self) -> &'static str
   {
      match *self {
         TagsKind::Vi    => "rusty-tags.vi",
         TagsKind::Emacs => "rusty-tags.emacs"
      }
   }

   pub fn ctags_option(&self) -> Option<&'static str>
   {
      match *self {
         TagsKind::Vi    => None,
         TagsKind::Emacs => Some("-e")
      }
   }
}
