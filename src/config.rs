use clap::App;
use types::TagsKind;

/// the configuration used to run rusty-tags
pub struct Config
{
   /// the kind of tags that should be created
   pub tags_kind: TagsKind,

   /// forces the recreation of cached tags
   pub force_recreate: bool,

   /// verbose output about all operations
   pub verbose: bool,

   /// don't output anything but errors
   pub quiet: bool
}

impl Config
{
   pub fn from_command_args() -> Config
   {
      let matches = App::new("rusty-tags")
         .about("Create ctags/etags for a cargo project and all of its dependencies")
         // Pull version from Cargo.toml
         .version(&*format!("v{}", crate_version!()))
         .author("Daniel Trstenjak <daniel.trstenjak@gmail.com>")
         .arg_from_usage("<TAGS_KIND> 'The kind of the created tags (vi, emacs)'")
         .arg_from_usage("-f --force-recreate 'Forces the recreation of all tags'")
         .arg_from_usage("-v --verbose 'Verbose output about all operations'")
         .arg_from_usage("-q --quiet 'Don't output anything but errors'")
         .get_matches();

      let quiet = matches.is_present("quiet");

      Config {
         tags_kind: value_t_or_exit!(matches.value_of("TAGS_KIND"), TagsKind),
         force_recreate: matches.is_present("force-recreate"),
         verbose: if quiet { false } else { matches.is_present("verbose") },
         quiet: quiet
      }
   }
}
