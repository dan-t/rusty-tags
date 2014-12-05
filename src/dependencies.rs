use std::io::fs::PathExtensions;
use std::io;
use toml;

use app_result::{AppResult, app_err};

#[deriving(Show)]
pub enum Dependency
{
   /// the depedency is based on a git repository
   Git {
      lib_name: String,
      commit_hash: String,
      dependencies: Vec<Dependency>
   },

   /// the depedency is from crates.io
   CratesIo {
      lib_name: String,
      version: String,
      dependencies: Vec<Dependency>
   }
}

pub type Dependencies = Vec<Dependency>;

/// Reads the dependecies from the `Cargo.toml` located in `cargo_toml_dir`
/// and for git depedencies the commit hash is read from the `Cargo.lock`. 
pub fn read_dependencies(cargo_toml_dir: &Path) -> AppResult<Dependencies>
{
   let mut cargo_toml = cargo_toml_dir.clone();
   cargo_toml.push("Cargo.toml");

   let toml_string = try!(io::File::open(&cargo_toml).read_to_string());
   let mut toml_parser = toml::Parser::new(&*toml_string);

   let toml_table = try!(
      toml_parser.parse()
         .ok_or_else(|| app_err(format!("Couldn't parse '{}': {}", cargo_toml.display(), toml_parser.errors)))
   );

   let mut deps: Dependencies = Vec::new();
   let deps_string = "dependencies".to_string();
   if ! toml_table.contains_key(&deps_string) {
      return Ok(deps);
   }

   let deps_table = try!(
      toml_table.get(&deps_string)
         .and_then(|value| value.as_table())
         .ok_or(app_err(format!("Couldn't get toml::Table entry for 'dependency'!")))
   );

   let mut cargo_lock = cargo_toml_dir.clone();
   cargo_lock.push("Cargo.lock");

   let lock_string = try!(io::File::open(&cargo_lock).read_to_string());
   let mut lock_parser = toml::Parser::new(&*lock_string);

   let lock_table = try!(
      lock_parser.parse()
         .ok_or_else(|| app_err(format!("Couldn't parse '{}': {}", cargo_lock.display(), lock_parser.errors)))
   );

   let packages = try!(
      lock_table.get(&"package".to_string())
         .and_then(|value| value.as_slice())
         .and_then(|slice| {
            let tables = slice.iter()
               .filter_map(|value| value.as_table())
               .collect::<Vec<&toml::TomlTable>>();

            if ! tables.is_empty() { Some(tables) } else { None }
         })
         .ok_or(app_err(format!("Couldn't get Array of Tables entry for 'package'!")))
   );

   let name_str = &String::from_str("name");
   let version_str = &String::from_str("version");
   let source_str = &String::from_str("source");

   for (lib_name, value) in deps_table.iter() {
      match *value {
         toml::String(_) | toml::Table(_) => {
            deps.push(try!(get_dependency(&packages, lib_name)));
         }

         _ => {
            return Err(app_err(format!(
               "Expected a String or a Table for the dependency with the name '{}', but got: '{}'", lib_name, value
            )));
         }
      }
   }

   Ok(deps)
}

fn get_dependency(packages: &Vec<&toml::TomlTable>, lib_name: &String) -> AppResult<Dependency>
{
   let name_str = &String::from_str("name");

   let package = try!(
      packages.iter()
         .find(|t| t.get(name_str).and_then(|n| n.as_str()) == Some(lib_name[]))
         .ok_or_else(|| app_err(format!("Couldn't find package with name = '{}'!", lib_name)))
   );

   let version_str = &String::from_str("version");
   let source_str = &String::from_str("source");
   let deps_str = &String::from_str("dependencies");

   let source = try!(
      package.get(source_str)
         .and_then(|src| src.as_str())
         .ok_or_else(|| app_err(format!("Couldn't find source string in package: '{}'!", package)))
   );

   let src_type = try!(
      source.split('+')
         .nth(0)
         .ok_or_else(|| app_err(format!("Couldn't find source type in: '{}'!", source)))
   );

   let version = try!(
      package.get(version_str)
         .and_then(|vers| vers.as_str())
         .ok_or_else(|| app_err(format!("Couldn't find version string in package: '{}'!", package)))
   );

   let dep_strs = match package.get(deps_str) {
      None => {
         Vec::<&str>::new()
      },

      Some(value) => {
         try!(
            value.as_slice()
               .and_then(|slice| {
                  let ds = slice.iter()
                     .filter_map(|v| v.as_str())
                     .collect::<Vec<&str>>();

                  if ! ds.is_empty() { Some(ds) } else { None }
               })
               .ok_or_else(|| app_err(format!("Couldn't get Array of Strings for 'dependencies' entry: '{}'!", package)))
         )
      }
   };

   let mut dep_names: Vec<&str> = Vec::new();
   for dep_str in dep_strs.iter() {
      let dep_name = try!(
         dep_str.split(' ')
            .nth(0)
            .ok_or_else(|| app_err(format!("Couldn't get name from dependency: '{}'!", dep_str)))
      );

      dep_names.push(dep_name);
   }

   let mut deps: Vec<Dependency> = Vec::new();
   for dep_name in dep_names.iter() {
      deps.push(try!(get_dependency(packages, &dep_name.to_string())));
   }

   match src_type {
      "git" => {
         let commit_hash = try!(
            source.split('#')
               .last()
               .ok_or_else(|| app_err(format!("Couldn't find commit hash in source entry: '{}'!", source)))
         );

         Ok(Dependency::Git { lib_name: lib_name.clone(), commit_hash: commit_hash.to_string(), dependencies: deps })
      },

      "registry" => {
         Ok(Dependency::CratesIo { lib_name: lib_name.clone(), version: version.to_string(), dependencies: deps })
      }

      _ => {
         Err(app_err(format!("Unexpected source type '{}' in package: '{}'!", src_type, package)))
      }
   }
}
