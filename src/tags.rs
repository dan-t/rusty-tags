use std::io;
use std::io::fs::PathExtensions;
use std::io::process::Command;
use std::collections::HashSet;

use app_result::{AppResult, app_err};
use types::{Tags, TagsKind, SourceKind};

use dirs::{
   rusty_tags_cache_dir,
   cargo_git_src_dir,
   cargo_crates_io_src_dir,
   glob_path
};

/// Checks if there's already a tags file for `src_kind`
/// and if not it's creating a new tags file and returning it.
pub fn update_tags(src_kind: &SourceKind, tags_kind: &TagsKind) -> AppResult<Tags>
{
   let cache_dir = try!(rusty_tags_cache_dir());

   let mut src_tags = cache_dir.clone();
   src_tags.push(src_kind.tags_file_name(tags_kind));

   let src_dir = try!(find_src_dir(src_kind));
   if src_tags.is_file() {
      return Ok(Tags { src_dir: src_dir, tags_file: src_tags, cached: true });
   }

   try!(create_tags(&src_dir, tags_kind, &src_tags));
   Ok(Tags { src_dir: src_dir, tags_file: src_tags, cached: false })
}

/// Does the same thing as `update_tags`, but also checks if the `lib.rs`
/// file of the library has public reexports of external crates. If
/// that's the case, then the tags of the public reexported external
/// crates are merged into the tags of the library.
pub fn update_tags_and_check_for_reexports(src_kind: &SourceKind,
                                           dependencies: &Vec<SourceKind>,
                                           tags_kind: &TagsKind) -> AppResult<Tags>
{
   let lib_tags = try!(update_tags(src_kind, tags_kind));
   if lib_tags.cached {
      return Ok(lib_tags);
   }

   let reexp_crates = try!(find_reexported_crates(&lib_tags.src_dir));
   if reexp_crates.is_empty() {
      return Ok(lib_tags);
   }

   println!("Found public reexports in '{}' of:", src_kind.get_lib_name());
   for rcrate in reexp_crates.iter() {
      println!("   {}", rcrate);
   }
   println!("");

   let mut crate_tags = Vec::<Path>::new();
   for rcrate in reexp_crates.iter() {
      if let Some(crate_dep) = dependencies.iter().find(|d| d.get_lib_name() == *rcrate) {
         crate_tags.push(try!(update_tags(crate_dep, tags_kind)).tags_file.clone());
      }
   }

   if crate_tags.is_empty() {
      return Ok(lib_tags);
   }

   crate_tags.push(lib_tags.tags_file.clone());
   try!(merge_tags(&crate_tags, &lib_tags.tags_file));
   Ok(lib_tags)
}

/// merges `tag_files` into `into_tag_file`
pub fn merge_tags(tag_files: &Vec<Path>, into_tag_file: &Path) -> AppResult<()>
{
   println!("Merging ...\n   tags:");

   let mut file_contents: Vec<String> = Vec::new();
   for file in tag_files.iter() {
      println!("      {}", file.display());
      file_contents.push(try!(io::File::open(file).read_to_string()));
   }

   println!("\n   into:\n      {}\n", into_tag_file.display());

   let mut merged_lines: Vec<&str> = Vec::with_capacity(100_000);
   for content in file_contents.iter() {
      for line in content.as_slice().lines_any() {
         if ! line.is_empty() && line.char_at(0) != '!' {
            merged_lines.push(line);
         }
      }
   }

   merged_lines.sort();
   merged_lines.dedup();

   let mut tag_file = try!(io::File::open_mode(into_tag_file, io::Truncate, io::ReadWrite));
   try!(tag_file.write_line("!_TAG_FILE_FORMAT	2	/extended format; --format=1 will not append ;\" to lines/"));
   try!(tag_file.write_line("!_TAG_FILE_SORTED	1	/0=unsorted, 1=sorted, 2=foldcase/"));

   for line in merged_lines.iter() {
      try!(tag_file.write_line(*line));
   }

   Ok(())
}

/// creates tags recursive for the directory hierarchy starting at `src_dir`
/// and writes them to `tags_file`
pub fn create_tags(src_dir: &Path, tags_kind: &TagsKind, tags_file: &Path) -> AppResult<()>
{
   let mut cmd = Command::new("ctags");

   tags_kind.ctags_option().map(|opt| { cmd.arg(opt); () });

   cmd.arg("--recurse")
      .arg("--languages=Rust")
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

   println!("Creating tags ...\n   for source:\n      {}\n\n   cached at:\n      {}\n",
            src_dir.display(), tags_file.display());

   try!(cmd.output());
   Ok(())
}

/// find the source directory of `src_kind`, for git sources the directories
/// in `~/.cargo/git/checkouts` are considered and for crates.io sources
/// the directories in `~/.cargo/registry/src/github.com-*` are considered
fn find_src_dir(src_kind: &SourceKind) -> AppResult<Path>
{
   match *src_kind {
      SourceKind::Git { ref lib_name, ref commit_hash } => {
         let mut lib_src = lib_name.clone();
         lib_src.push_str("-*");

         let mut src_dir = try!(cargo_git_src_dir());
         src_dir.push(&lib_src);
         src_dir.push("master");

         let mut src_paths = glob_path(&src_dir);
         for src_path in src_paths {
            let src_commit_hash = try!(get_commit_hash(&src_path));
            if *commit_hash == src_commit_hash {
               return Ok(src_path);
            }
         }

         // the git repository name hasn't to match the name of the library,
         // so here we're just going through all git directories and searching
         // for the one with a matching commit hash
         let mut src_dir = try!(cargo_git_src_dir());
         src_dir.push("*");
         src_dir.push("master");

         let mut src_paths = glob_path(&src_dir);
         for src_path in src_paths {
            let src_commit_hash = try!(get_commit_hash(&src_path));
            if *commit_hash == src_commit_hash {
               return Ok(src_path);
            }
         }

         Err(app_err(format!("
   Couldn't find git repository of the dependency '{}'!
   Have you run 'cargo build' at least once or have you added/updated a dependency without calling 'cargo build' again?", lib_name)))
      },

      SourceKind::CratesIo { ref lib_name, ref version } => {
         let mut lib_src = lib_name.clone();
         lib_src.push('-');
         lib_src.push_str(&**version);

         let mut src_dir = try!(cargo_crates_io_src_dir());
         src_dir.push(&lib_src);

         if ! src_dir.is_dir() {
            return Err(app_err(format!("
   Couldn't find source code of the dependency '{}'!
   Have you run 'cargo build' at least once or have you added/updated a dependency without calling 'cargo build' again?", lib_name)))
         }

         Ok(src_dir)
      }
   }
}

type CrateName = String;

/// searches in the file `<src_dir>/src/lib.rs` for external crates
/// that are reexpored and returns their names
fn find_reexported_crates(src_dir: &Path) -> AppResult<Vec<CrateName>>
{
   let mut lib_file = src_dir.clone();
   lib_file.push("src");
   lib_file.push("lib.rs");

   if ! lib_file.is_file() {
      return Ok(Vec::new());
   }

   let contents = try!(io::File::open(&lib_file).read_to_string());
   let mut lines = contents.lines_any();

   type ModuleName = String;
   let mut pub_uses = HashSet::<ModuleName>::new();

   #[deriving(Eq, PartialEq, Hash)]
   struct ExternCrate<'a>
   {
      name: &'a str,
      as_name: &'a str
   }

   let mut extern_crates = HashSet::<ExternCrate>::new();

   for line in lines {
      let items = line.trim_chars(';').split(' ').collect::<Vec<&str>>();
      if items.len() < 3 {
         continue;
      }

      if items[0] == "pub" && items[1] == "use" {
         let mods = items[2].split_str("::").collect::<Vec<&str>>();
         if mods.len() >= 1 {
            pub_uses.insert(mods[0].to_string());
         }
      }

      if items[0] == "extern" && items[1] == "crate" {
         if items.len() == 3 {
            extern_crates.insert(ExternCrate { name: items[2].trim_chars('"'), as_name: items[2] });
         }
         else if items.len() == 5 && items[3] == "as" {
            extern_crates.insert(ExternCrate { name: items[2].trim_chars('"'), as_name: items[4] });
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
