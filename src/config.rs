use std::env;
use std::path::PathBuf;
use clap::{App, Arg};
use types::TagsKind;
use app_result::{AppResult, app_err_msg};
use path_ext::PathExt;

/// the configuration used to run rusty-tags
pub struct Config
{
   /// the kind of tags that should be created
   pub tags_kind: TagsKind,

   /// start directory for the search of the 'Cargo.toml'
   pub start_dir: PathBuf,

   /// forces the recreation of cached tags
   pub force_recreate: bool,

   /// verbose output about all operations
   pub verbose: bool,

   /// don't output anything but errors
   pub quiet: bool
}

impl Config
{
   pub fn from_command_args() -> AppResult<Config>
   {
      let start_dir_value_name = ["DIR"];

      let matches = App::new("rusty-tags")
         .about("Create ctags/etags for a cargo project and all of its dependencies")
         // Pull version from Cargo.toml
         .version(&*format!("v{}", crate_version!()))
         .author("Daniel Trstenjak <daniel.trstenjak@gmail.com>")
         .arg_from_usage("<TAGS_KIND> 'The kind of the created tags (vi, emacs)'")
         .arg(Arg::with_name("start-dir")
                 .short("s")
                 .long("start-dir")
                 .value_names(&start_dir_value_name)
                 .help("Start directory for the search of the Cargo.toml (default: current working directory)")
                 .takes_value(true))
         .arg_from_usage("-f --force-recreate 'Forces the recreation of all tags'")
         .arg_from_usage("-v --verbose 'Verbose output about all operations'")
         .arg_from_usage("-q --quiet 'Don't output anything but errors'")
         .get_matches();

      let start_dir = matches.value_of("start-dir")
         .map(PathBuf::from)
         .unwrap_or(try!(env::current_dir()));

      if ! start_dir.is_dir() {
         return Err(app_err_msg(format!("Invalid directory given to '--start-dir': '{}'!", start_dir.display())));
      }

      let quiet = matches.is_present("quiet");
      Ok(Config {
         tags_kind: value_t_or_exit!(matches.value_of("TAGS_KIND"), TagsKind),
         start_dir: start_dir,
         force_recreate: matches.is_present("force-recreate"),
         verbose: if quiet { false } else { matches.is_present("verbose") },
         quiet: quiet
      })
   }
}
