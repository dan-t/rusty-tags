use std::io;
use std::io::fs;
use std::io::fs::PathExtensions;
use std::io::process::Command;
use std::fmt::{Show, Formatter, Error};

use app_result::{AppResult, app_err};

use dirs::{
   tags_dir,
   git_src_dir,
   crates_io_src_dir,
   glob_path
};

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

/// Checks if there's already a tags file for `lib_name` and `commit_hash`
/// and if not it's creating a new tags file and returning it.
pub fn update_git_tags(lib_name: &String, commit_hash: &String) -> AppResult<Tags>
{
   let mut lib_tags = lib_name.clone();
   lib_tags.push('-');
   lib_tags.push_str(&**commit_hash);

   let mut tags_file = try!(tags_dir());
   tags_file.push(&lib_tags);

   let mut lib_src = lib_name.clone();
   lib_src.push_str("-*");

   let mut src_dir = try!(git_src_dir());
   src_dir.push(&lib_src);
   src_dir.push("master");

   let mut src_paths = glob_path(&src_dir);
   for src_path in src_paths {
      let src_commit_hash = try!(get_commit_hash(&src_path));
      if *commit_hash == src_commit_hash {
         let mut cached = true;
         if ! tags_file.is_file() {
            try!(create_tags(&src_path, &tags_file));
            cached = false;
         }

         return Ok(Tags { src_dir: src_path.clone(), tags_file: tags_file, cached: cached });
      }
   }

   // the git repository name hasn't to match the name of the library,
   // so here we're just going through all git directories and searching
   // for the one with a matching commit hash
   let mut src_dir = try!(git_src_dir());
   src_dir.push("*");
   src_dir.push("master");

   let mut src_paths = glob_path(&src_dir);
   for src_path in src_paths {
      let src_commit_hash = try!(get_commit_hash(&src_path));
      if *commit_hash == src_commit_hash {
         let mut cached = true;
         if ! tags_file.is_file() {
            try!(create_tags(&src_path, &tags_file));
            cached = false;
         }

         return Ok(Tags { src_dir: src_path.clone(), tags_file: tags_file, cached: cached });
      }
   }

   Err(app_err(format!("
   Couldn't find git repository of the dependency '{}'!
   Have you run 'cargo build' at least once after adding the dependency?", lib_name)))
}

/// Checks if there's already a tags file for `lib_name` and `version`
/// and if not it's creating a new tags file and returning it.
pub fn update_crates_io_tags(lib_name: &String, version: &String) -> AppResult<Tags>
{
   let mut lib_tags = lib_name.clone();
   lib_tags.push('-');
   lib_tags.push_str(&**version);

   let mut tags_file = try!(tags_dir());
   tags_file.push(&lib_tags);

   let mut src_dir = try!(crates_io_src_dir());
   src_dir.push(&lib_tags);

   if ! src_dir.is_dir() {
      return Err(app_err(format!("
   Couldn't find source code of the dependency '{}'!
   Have you run 'cargo build' at least once after adding the dependency?", lib_name)));
   }

   let mut cached = true;
   if ! tags_file.is_file() {
      try!(create_tags(&src_dir, &tags_file));
      cached = false;
   }

   Ok(Tags { src_dir: src_dir, tags_file: tags_file, cached: cached })
}

/// merges `tag_files` into `merged_tag_file`
pub fn merge_tags(tag_files: &Vec<Path>, merged_tag_file: &Path) -> AppResult<()>
{
   let mut file_contents: Vec<String> = Vec::new();
   for file in tag_files.iter() {
      file_contents.push(try!(io::File::open(file).read_to_string()));
   }

   let mut merged_lines: Vec<&str> = Vec::with_capacity(100_000);
   for content in file_contents.iter() {
      for line in content.as_slice().lines_any() {
         if ! line.is_empty() && line.char_at(0) != '!' {
            merged_lines.push(line);
         }
      }
   }

   merged_lines.sort();

   let mut tag_file = try!(io::File::open_mode(merged_tag_file, io::Truncate, io::ReadWrite));
   try!(tag_file.write_line("!_TAG_FILE_FORMAT	2	/extended format; --format=1 will not append ;\" to lines/"));
   try!(tag_file.write_line("!_TAG_FILE_SORTED	1	/0=unsorted, 1=sorted, 2=foldcase/"));

   for line in merged_lines.iter() {
      try!(tag_file.write_line(*line));
   }

   Ok(())
}

/// creates tags recursive for the directory hierarchy starting at `src_dir`
/// and writes them to `tags_file`
pub fn create_tags(src_dir: &Path, tags_file: &Path) -> AppResult<()>
{
   let mut cmd = Command::new("ctags");
   cmd.arg("--recurse")
      .arg("--langdef=Rust")
      .arg("--langmap=Rust:.rs")
      .arg("--regex-Rust=/^[ \\t]*(#\\[[^\\]]\\][ \\t]*)*(pub[ \\t]+)?(extern[ \\t]+)?(\"[^\"]+\"[ \\t]+)?(unsafe[ \\t]+)?fn[ \\t]+([a-zA-Z0-9_]+)/\\6/f,functions,function definitions/")
      .arg("--regex-Rust=/^[ \\t]*(pub[ \\t]+)?type[ \\t]+([a-zA-Z0-9_]+)/\\2/T,types,type definitions/")
      .arg("--regex-Rust=/^[ \\t]*(pub[ \\t]+)?enum[ \\t]+([a-zA-Z0-9_]+)/\\2/g,enum,enumeration names/")
      .arg("--regex-Rust=/^[ \\t]*(pub[ \\t]+)?struct[ \\t]+([a-zA-Z0-9_]+)/\\2/s,structure names/")
      .arg("--regex-Rust=/^[ \\t]*(pub[ \\t]+)?mod[ \\t]+([a-zA-Z0-9_]+)/\\2/m,modules,module names/")
      .arg("--regex-Rust=/^[ \\t]*(pub[ \\t]+)?static[ \\t]+([a-zA-Z0-9_]+)/\\2/c,consts,static constants/")
      .arg("--regex-Rust=/^[ \\t]*(pub[ \\t]+)?trait[ \\t]+([a-zA-Z0-9_]+)/\\2/t,traits,traits/")
      .arg("--regex-Rust=/^[ \\t]*(pub[ \\t]+)?impl([ \\t\\n]*<[^>]*>)?[ \\t]+(([a-zA-Z0-9_:]+)[ \\t]*(<[^>]*>)?[ \\t]+(for)[ \\t]+)?([a-zA-Z0-9_]+)/\\4 \\6 \\7/i,impls,trait implementations/")
      .arg("--regex-Rust=/^[ \\t]*macro_rules![ \\t]+([a-zA-Z0-9_]+)/\\1/d,macros,macro definitions/")
      .arg("-o")
      .arg(tags_file)
      .arg(src_dir);

   try!(cmd.output());

   Ok(())
}

/// get the commit hash of the current `HEAD` of the git repository located at `git_dir`
fn get_commit_hash(git_dir: &Path) -> AppResult<String>
{
   let mut cmd = Command::new("git");
   cmd.cwd(git_dir)
      .arg("rev-parse")
      .arg("HEAD");

   let out = try!(cmd.output());
   String::from_utf8(out.output)
      .map(|s| s.as_slice().trim().to_string())
      .map_err(|_| app_err("Couldn't convert git output to utf8!".to_string()))
}

impl Show for Tags
{
   fn fmt(&self, f: &mut Formatter) -> Result<(), Error>
   {
      write!(f, "Tags ( src_dir: {}, tags_file: {}, cached: {} )",
             self.src_dir.display(), self.tags_file.display(), self.cached)
   }
}
