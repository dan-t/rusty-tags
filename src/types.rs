use std::fmt::{Show, Formatter, Error};

pub enum TagsRoot
{
   /// the source directory and the dependencies
   /// of the cargo project
   Src {
      src_dir: Path,
      dependencies: Vec<SourceKind>
   },

   /// a library and its depedencies
   Lib {
      src_kind: SourceKind,
      dependencies: Vec<SourceKind>
   }
}

pub type TagsRoots = Vec<TagsRoot>;

impl Show for TagsRoot
{
   fn fmt(&self, f: &mut Formatter) -> Result<(), Error>
   {
      match *self {
         TagsRoot::Src { ref src_dir, ref dependencies } => {
            write!(f, "Src ( src_dir: {}, dependencies: {} )", src_dir.display(), dependencies)
         },

         TagsRoot::Lib { ref src_kind, ref dependencies } => {
            write!(f, "Lib ( src_kind: {}, dependencies: {} )", src_kind, dependencies)
         }
      }
   }
}

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
   }
}

impl SourceKind
{
   pub fn tags_file_name(&self) -> String
   {
      match *self {
         SourceKind::Git { ref lib_name, ref commit_hash } => {
            format!("{}-{}", lib_name, commit_hash)
         },

         SourceKind::CratesIo { ref lib_name, ref version } => {
            format!("{}-{}", lib_name, version)
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
         }
      }
   }
}

impl Show for SourceKind
{
   fn fmt(&self, f: &mut Formatter) -> Result<(), Error>
   {
      match *self {
         SourceKind::Git { ref lib_name, ref commit_hash } => {
            write!(f, "{}-{}", lib_name, commit_hash)
         },

         SourceKind::CratesIo { ref lib_name, ref version } => {
            write!(f, "{}-{}", lib_name, version)
         }
      }
   }
}

pub struct Tags
{
   /// the root directory of the source code
   /// for which the tags have been created
   pub src_dir: Path,

   /// the tags file of the sources in `src_dir`
   pub tags_file: Path,

   /// indicates if the tags file is already existing
   /// and the cached tags file is returned
   pub cached: bool
}


impl Show for Tags
{
   fn fmt(&self, f: &mut Formatter) -> Result<(), Error>
   {
      write!(f, "Tags ( src_dir: {}, tags_file: {}, cached: {} )",
             self.src_dir.display(), self.tags_file.display(), self.cached)
   }
}
