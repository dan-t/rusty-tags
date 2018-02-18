use std::env;
use std::path::{Path, PathBuf};
use std::fs::File;
use std::io::Read;
use std::cmp::max;
use std::process::Command;
use clap::App;
use toml;
use num_cpus;
use types::{TagsExe, TagsKind, TagsSpec};
use rt_result::RtResult;
use dirs;

/// the configuration used to run rusty-tags
pub struct Config {
    /// the tags that should be created
    pub tags_spec: TagsSpec,

    /// start directory for the search of the 'Cargo.toml'
    pub start_dir: PathBuf,

    /// do not generate tags for dependencies
    pub omit_deps: bool,

    /// forces the recreation of cached tags
    pub force_recreate: bool,

    /// verbose output about all operations
    pub verbose: bool,

    /// don't output anything but errors
    pub quiet: bool,

    /// num threads used for the tags creation
    pub num_threads: u32
}

impl Config {
   pub fn from_command_args() -> RtResult<Config> {
       let matches = App::new("rusty-tags")
           .about("Create ctags/etags for a cargo project and all of its dependencies")
           // Pull version from Cargo.toml
           .version(crate_version!())
           .author("Daniel Trstenjak <daniel.trstenjak@gmail.com>")
           .arg_from_usage("<TAGS_KIND> 'The kind of the created tags (vi, emacs)'")
           .arg_from_usage("-s --start-dir [DIR] 'Start directory for the search of the Cargo.toml (default: current working directory)'")
           .arg_from_usage("-o --omit-deps 'Do not generate tags for dependencies'")
           .arg_from_usage("-f --force-recreate 'Forces the recreation of the tags of all dependencies and the Rust standard library'")
           .arg_from_usage("-v --verbose 'Verbose output about all operations'")
           .arg_from_usage("-q --quiet 'Don't output anything but errors'")
           .arg_from_usage("-n --num-threads [NUM] 'Num threads used for the tags creation (default: num available physical cpus)'")
           .get_matches();

       let start_dir = matches.value_of("start-dir")
           .map(PathBuf::from)
           .unwrap_or(env::current_dir()?);

       if ! start_dir.is_dir() {
           return Err(format!("Invalid directory given to '--start-dir': '{}'!", start_dir.display()).into());
       }


       let (vi_tags, emacs_tags, ctags_options) = {
           let mut vt = "rusty-tags.vi".to_string();
           let mut et = "rusty-tags.emacs".to_string();
           let mut cto = "".to_string();
           if let Some(file_config) = ConfigFromFile::load()? {
               if let Some(fcvt) = file_config.vi_tags { vt = fcvt; }
               if let Some(fcet) = file_config.emacs_tags { et = fcet; }
               if let Some(fccto) = file_config.ctags_options { cto = fccto; }
           }

           (vt, et, cto)
       };

       let kind = value_t_or_exit!(matches.value_of("TAGS_KIND"), TagsKind);
       let omit_deps = matches.is_present("omit-deps");
       let force_recreate = matches.is_present("force-recreate");
       let quiet = matches.is_present("quiet");
       let verbose = if quiet { false } else { matches.is_present("verbose") };

       let num_threads = value_t!(matches.value_of("num-threads"), u32)
           .map(|n| max(1, n))
           .unwrap_or(num_cpus::get_physical() as u32);

       let ctags_exe = detect_tags_exe()?;
       if verbose {
           println!("Using configuration: vi_tags='{}', emacs_tags='{}', ctags_options='{}'",
                    vi_tags, emacs_tags, ctags_options);
           println!("Found {:?}", ctags_exe);
       }

       Ok(Config {
           tags_spec: TagsSpec::new(kind, ctags_exe, vi_tags, emacs_tags, ctags_options)?,
           start_dir: start_dir,
           omit_deps: omit_deps,
           force_recreate: force_recreate,
           verbose: verbose,
           quiet: quiet,
           num_threads: num_threads,
       })
   }
}

/// Represents the data from a `.rusty-tags/config.toml` configuration file.
#[derive(Deserialize, Debug, Default)]
struct ConfigFromFile {
    /// the file name used for vi tags
    vi_tags: Option<String>,

    /// the file name used for emacs tags
    emacs_tags: Option<String>,

    /// options given to the ctags executable
    ctags_options: Option<String>
}

impl ConfigFromFile {
    fn load() -> RtResult<Option<ConfigFromFile>> {
        let config_file = dirs::rusty_tags_dir().map(|p| p.join("config.toml"))?;
        if ! config_file.is_file() {
            return Ok(None);
        }

        let config = map_file(&config_file, |contents| {
            let config = toml::from_str(&contents)?;
            Ok(config)
        })?;

        Ok(Some(config))
    }
}

/// Reads `file` into a string which is passed to the function `f`
/// and its return value is returned by `map_file`.
fn map_file<R, F>(file: &Path, f: F) -> RtResult<R>
    where F: FnOnce(String) -> RtResult<R>
{
    let mut file = File::open(file)?;

    let mut contents = String::new();
    file.read_to_string(&mut contents)?;

    let r = f(contents)?;
    Ok(r)
}

fn detect_tags_exe() -> RtResult<TagsExe> {
    for exe in &["ctags", "exuberant-ctags", "universal-ctags"] {
        let mut cmd = Command::new(exe);
        cmd.arg("--version");

        if let Ok(output) = cmd.output() {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                if stdout.contains("Universal Ctags") {
                    return Ok(TagsExe::UniversalCtags(exe.to_string()));
                }

                return Ok(TagsExe::ExuberantCtags(exe.to_string()));
            }
        }
    }

    Err("Couldn't find 'ctags' executable! Is 'ctags' correctly installed?".into())
}
