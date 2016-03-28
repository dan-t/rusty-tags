use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use toml;

use app_result::{AppResult, app_err_msg};

use types::{
   TagsRoot,
   TagsRoots,
   SourceKind
};

/// Reads the dependencies from the `Cargo.toml` located in `cargo_toml_dir`
pub fn read_dependencies(cargo_toml_dir: &Path) -> AppResult<TagsRoots>
{
   let toml_table = {
      let mut cargo_toml = cargo_toml_dir.to_path_buf();
      cargo_toml.push("Cargo.toml");
      try!(parse_toml(&cargo_toml))
   };

   let mut tags_roots: TagsRoots = Vec::new();
   let deps = try!(collect_dependencies(&toml_table));
   if deps.is_empty() {
      tags_roots.push(TagsRoot::Src { src_dir: cargo_toml_dir.to_path_buf(), dependencies: Vec::new() });
      return Ok(tags_roots);
   }

   let lock_table = {
      let mut cargo_lock = cargo_toml_dir.to_path_buf();
      cargo_lock.push("Cargo.lock");
      try!(parse_toml(&cargo_lock))
   };

   let packages: Vec<&toml::Table> = try!(
      lock_table.get("package")
         .and_then(toml::Value::as_slice)
         .map(|s| {
            s.iter()
             .filter_map(toml::Value::as_table)
             .collect()
         })
         .ok_or(app_err_msg(format!("Couldn't get Array of Tables entry for 'package'!")))
   );

   for package in packages.iter() {
      let lib_name = try!(
         package.get("name")
            .and_then(toml::Value::as_str)
            .ok_or_else(|| app_err_msg(format!("Couldn't find name string in package: '{:?}'!", package)))
      );

      let dep_names = try!(get_dependencies(*package));
      let mut dep_src_kinds: Vec<SourceKind> = Vec::new();
      for dep_name in dep_names.iter() {
         let dep_package = try!(find_package(&packages, *dep_name));
         if let Ok(src_kind) = get_source_kind(dep_package, *dep_name) {
            dep_src_kinds.push(src_kind);
         }
      }

      if let Ok(src_kind) = get_source_kind(*package, lib_name) {
         tags_roots.push(TagsRoot::Lib { src_kind: src_kind, dependencies: dep_src_kinds });
      }
   }

   let mut lib_src_kinds: Vec<SourceKind> = Vec::new();
   for (lib_name, value) in deps {
      match *value {
         // handling crates.io dependencies with a version
         toml::Value::String(_) => {
            let pkg = try!(find_package(&packages, lib_name));
            lib_src_kinds.push(try!(get_source_kind(pkg, lib_name)));
         }

         // handling of table dependencies
         toml::Value::Table(ref table) => {
            // handling of local path dependencies
            if let Some(path) = table.get("path") {
               let mut path = try!(
                  path.as_str().ok_or_else(|| {
                     app_err_msg(format!("Expected a String for 'path' entry in '{}'", value))
                  })
                  .map(PathBuf::from)
               );

               if path.is_relative() {
                  let mut abs_path = cargo_toml_dir.to_path_buf();
                  abs_path.push(path);
                  path = abs_path;
               }

               lib_src_kinds.push(SourceKind::Path { lib_name: lib_name.clone(), path: path });
            }
            // handling of git and crates.io dependencies (may have additional parameters set:
            // optional, etc.)
            else if table.get("version").is_some() || table.get("git").is_some() {
               let pkg = try!(find_package(&packages, lib_name));
               lib_src_kinds.push(try!(get_source_kind(pkg, lib_name)));
            }
            else {
               return Err(app_err_msg(format!(
                  "Couldn't find a 'path', 'version' or 'git' attribute for '{}' in '{}'", lib_name, value
               )));
            }
         }

         _ => {
            return Err(app_err_msg(format!(
               "Expected a String or a Table for the dependency with the name '{}', but got: '{}'", lib_name, value
            )));
         }
      }
   }

   tags_roots.push(TagsRoot::Src { src_dir: cargo_toml_dir.to_path_buf(), dependencies: lib_src_kinds });

   Ok(tags_roots)
}

fn get_source_kind(lib_package: &toml::Table, lib_name: &str) -> AppResult<SourceKind>
{
   let source = try!(
      lib_package.get("source")
         .and_then(toml::Value::as_str)
         .ok_or_else(|| app_err_msg(format!("Couldn't find source string in package: '{:?}'!", lib_package)))
   );

   let src_type = try!(
      source.split('+')
         .nth(0)
         .ok_or_else(|| app_err_msg(format!("Couldn't find source type in: '{}'!", source)))
   );

   let version = try!(
      lib_package.get("version")
         .and_then(toml::Value::as_str)
         .ok_or_else(|| app_err_msg(format!("Couldn't find version string in package: '{:?}'!", lib_package)))
   );

   match src_type {
      "git" => {
         let commit_hash = try!(
            source.split('#')
               .last()
               .ok_or_else(|| app_err_msg(format!("Couldn't find commit hash in source entry: '{}'!", source)))
         );

         Ok(SourceKind::Git { lib_name: lib_name.to_string(), commit_hash: commit_hash.to_string() })
      },

      "registry" => {
         Ok(SourceKind::CratesIo { lib_name: lib_name.to_string(), version: version.to_string() })
      }

      _ => {
         Err(app_err_msg(format!("Unexpected source type '{}' in package: '{:?}'!", src_type, lib_package)))
      }
   }
}

/// the name of a dependency
type DepName = String;

/// Collects all dependencies specified in the `Cargo.toml`. The dependencies might
/// be specified by `dependencies`, `build-dependencies` or `dev-dependencies`.
fn collect_dependencies(cargo_toml: &toml::Table) -> AppResult<Vec<(&DepName, &toml::Value)>>
{
   let mut deps = Vec::with_capacity(50);
   for dep_type in &["dependencies", "build-dependencies", "dev-dependencies"] {
      if let Some(deps_value) = cargo_toml.get(*dep_type) {
         let deps_table = try!(
            deps_value.as_table()
               .ok_or(app_err_msg(format!("Couldn't get toml::Table entry for '{}'! Got a '{}'!", dep_type, deps_value)))
            );

         for dep in deps_table {
            deps.push(dep);
         }
      }
   }

   Ok(deps)
}

fn get_dependencies(lib_package: &toml::Table) -> AppResult<Vec<&str>>
{
   let mut dep_names: Vec<&str> = Vec::new();

   if ! lib_package.contains_key("dependencies") {
      return Ok(dep_names);
   }

   let dep_strs: Vec<&str> = try!(
      lib_package.get("dependencies")
         .and_then(toml::Value::as_slice)
         .map(|s| {
            s.iter()
             .filter_map(toml::Value::as_str)
             .collect()
         })
         .ok_or_else(|| app_err_msg(format!("Couldn't get Array of Strings for 'dependencies' entry: '{:?}'!", lib_package)))
   );

   for dep_str in dep_strs.iter() {
      let dep_name = try!(
         dep_str.split(' ')
            .nth(0)
            .ok_or_else(|| app_err_msg(format!("Couldn't get name from dependency: '{}'!", dep_str)))
      );

      dep_names.push(dep_name);
   }

   Ok(dep_names)
}

fn find_package<'a>(packages: &'a Vec<&toml::Table>, lib_name: &str) -> AppResult<&'a toml::Table>
{
   let package = try!(
      packages.iter()
         .find(|p| p.get("name").and_then(toml::Value::as_str) == Some(lib_name))
         .ok_or_else(|| app_err_msg(format!("Couldn't find package with name = '{}'!", lib_name)))
   );

   Ok(*package)
}

fn parse_toml(path: &Path) -> AppResult<toml::Table>
{
   let mut file = try!(File::open(path));
   let mut string = String::new();
   try!(file.read_to_string(&mut string));
   let mut parser = toml::Parser::new(&string);
   parser.parse().ok_or_else(|| app_err_msg(format!("Couldn't parse '{}': {:?}", path.display(), parser.errors)))
}
