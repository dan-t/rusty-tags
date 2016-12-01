use std::env;
use std::path::{Path, PathBuf};
use std::fs::File;
use std::io::Read;
use clap::{App, Arg};
use toml;
use rustc_serialize::Decodable;
use tempdir::TempDir;
use types::{TagsKind, TagsSpec};
use rt_result::RtResult;
use dirs;

/// the configuration used to run rusty-tags
pub struct Config {
    /// the tags that should be created
    pub tags_spec: TagsSpec,

    /// start directory for the search of the 'Cargo.toml'
    pub start_dir: PathBuf,

    /// forces the recreation of cached tags
    pub force_recreate: bool,

    /// verbose output about all operations
    pub verbose: bool,

    /// don't output anything but errors
    pub quiet: bool,

    cache_temp_dir: TempDir,

    cargo_temp_dir: TempDir,

    source_temp_dir: TempDir
}

impl Config {
   pub fn from_command_args() -> RtResult<Config> {
       let matches = App::new("rusty-tags")
           .about("Create ctags/etags for a cargo project and all of its dependencies")
           // Pull version from Cargo.toml
           .version(crate_version!())
           .author("Daniel Trstenjak <daniel.trstenjak@gmail.com>")
           .arg_from_usage("<TAGS_KIND> 'The kind of the created tags (vi, emacs)'")
           .arg(Arg::with_name("start-dir")
                .short("s")
                .long("start-dir")
                .value_names(&["DIR"])
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
           return Err(format!("Invalid directory given to '--start-dir': '{}'!", start_dir.display()).into());
       }

       let quiet = matches.is_present("quiet");
       let kind = value_t_or_exit!(matches.value_of("TAGS_KIND"), TagsKind);

       let (vi_tags, emacs_tags) = {
           let mut vt = "rusty-tags.vi".to_string();
           let mut et = "rusty-tags.emacs".to_string();
           if let Some(file_config) = try!(ConfigFromFile::load()) {
               if let Some(fcvt) = file_config.vi_tags { vt = fcvt; }
               if let Some(fcet) = file_config.emacs_tags { et = fcet; }
           }

           (vt, et)
       };

       let cache_temp_dir = try!(TempDir::new_in(try!(dirs::rusty_tags_cache_dir()), "cache-temp-dir"));
       let cargo_temp_dir = try!(TempDir::new_in(try!(dirs::cargo_dir()), "cargo-temp-dir"));
       let src_temp_dir = try!(TempDir::new_in(&start_dir, "source-temp-dir"));

       Ok(Config {
           tags_spec: try!(TagsSpec::new(kind, vi_tags, emacs_tags)),
           start_dir: start_dir,
           force_recreate: matches.is_present("force-recreate"),
           verbose: if quiet { false } else { matches.is_present("verbose") },
           quiet: quiet,
           cache_temp_dir: cache_temp_dir,
           cargo_temp_dir: cargo_temp_dir,
           source_temp_dir: src_temp_dir
       })
   }

   pub fn cache_temp_file(&self, name: &str) -> RtResult<PathBuf> {
       unique_file(self.cache_temp_dir.path(), name)
   }

   pub fn cargo_temp_file(&self, name: &str) -> RtResult<PathBuf> {
       unique_file(self.cargo_temp_dir.path(), name)
   }

   pub fn source_temp_file(&self, name: &str) -> RtResult<PathBuf> {
       unique_file(self.source_temp_dir.path(), name)
   }

   pub fn close_temp_dirs(self) -> RtResult<()> {
       try!(self.cache_temp_dir.close());
       try!(self.cargo_temp_dir.close());
       try!(self.source_temp_dir.close());
       Ok(())
   }
}

/// Represents the data from a `.rusty-tags/config.toml` configuration file.
#[derive(RustcDecodable, Debug, Default)]
struct ConfigFromFile {
    /// the file name used for vi tags
    vi_tags: Option<String>,

    /// the file name used for emacs tags
    emacs_tags: Option<String>,
}

impl ConfigFromFile {
    fn load() -> RtResult<Option<ConfigFromFile>> {
        let config_file = try!(dirs::rusty_tags_dir().map(|p| p.join("config.toml")));
        if ! config_file.is_file() {
            return Ok(None);
        }

        let config = try!(map_file(&config_file, |contents| {
            let mut parser = toml::Parser::new(&contents);
            let value = try!(parser.parse()
                .ok_or_else(|| format!("Couldn't parse toml file '{}': {:?}", config_file.display(), parser.errors)));

            let mut decoder = toml::Decoder::new(toml::Value::Table(value));
            Ok(try!(ConfigFromFile::decode(&mut decoder)))
        }));

        Ok(Some(config))
    }
}

/// Reads `file` into a string which is passed to the function `f`
/// and its return value is returned by `map_file`.
pub fn map_file<R, F>(file: &Path, f: F) -> RtResult<R>
    where F: FnOnce(String) -> RtResult<R>
{
    let mut file = try!(File::open(file));

    let mut contents = String::new();
    try!(file.read_to_string(&mut contents));

    let r = try!(f(contents));
    Ok(r)
}

fn unique_file(dir: &Path, name: &str) -> RtResult<PathBuf> {
    let file = dir.join(name);
    if ! file.is_file() {
        return Ok(file);
    }

    for i in 0..(1 << 31) {
        let file = dir.join(format!("{}_{}", name, i));
        if ! file.is_file() {
            return Ok(file);
        }
    }

    Err(format!("Couldn't generate unique file for '{}' inside of '{}'!", name, dir.display()).into())
}
