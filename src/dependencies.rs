use std::io;
use toml;

use app_result::{AppResult, app_err};

use types::{
   TagsRoot,
   TagsRoots,
   SourceKind
};

/// Reads the dependencies from the `Cargo.toml` located in `cargo_toml_dir`
pub fn read_dependencies(cargo_toml_dir: &Path) -> AppResult<TagsRoots>
{
   let mut cargo_toml = cargo_toml_dir.clone();
   cargo_toml.push("Cargo.toml");

   let toml_string = try!(io::File::open(&cargo_toml).read_to_string());
   let mut toml_parser = toml::Parser::new(&*toml_string);

   let toml_table = try!(
      toml_parser.parse()
         .ok_or_else(|| app_err(format!("Couldn't parse '{}': {}", cargo_toml.display(), toml_parser.errors)))
   );

   let mut tags_roots: TagsRoots = Vec::new();
   if ! toml_table.contains_key("dependencies") {
      tags_roots.push(TagsRoot::Src { src_dir: cargo_toml_dir.clone(), dependencies: Vec::new() });
      return Ok(tags_roots);
   }

   let deps_table = try!(
      toml_table.get("dependencies")
         .and_then(toml::Value::as_table)
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

   let packages: Vec<&toml::Table> = try!(
      lock_table.get("package")
         .and_then(toml::Value::as_slice)
         .map(|s| {
            s.iter()
             .filter_map(toml::Value::as_table)
             .collect()
         })
         .ok_or(app_err(format!("Couldn't get Array of Tables entry for 'package'!")))
   );

   for package in packages.iter() {
      let lib_name = try!(
         package.get("name")
            .and_then(toml::Value::as_str)
            .ok_or_else(|| app_err(format!("Couldn't find name string in package: '{}'!", package)))
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
   for (lib_name, value) in deps_table.iter() {
      match *value {
         toml::Value::String(_) | toml::Value::Table(_) => {
            let lib_package = try!(find_package(&packages, lib_name[]));
            if let Ok(src_kind) = get_source_kind(lib_package, lib_name[]) {
               lib_src_kinds.push(src_kind);
            }
         }

         _ => {
            return Err(app_err(format!(
               "Expected a String or a Table for the dependency with the name '{}', but got: '{}'", lib_name, value
            )));
         }
      }
   }

   tags_roots.push(TagsRoot::Src { src_dir: cargo_toml_dir.clone(), dependencies: lib_src_kinds });

   Ok(tags_roots)
}

fn get_source_kind(lib_package: &toml::Table, lib_name: &str) -> AppResult<SourceKind>
{
   let source = try!(
      lib_package.get("source")
         .and_then(toml::Value::as_str)
         .ok_or_else(|| app_err(format!("Couldn't find source string in package: '{}'!", lib_package)))
   );

   let src_type = try!(
      source.split('+')
         .nth(0)
         .ok_or_else(|| app_err(format!("Couldn't find source type in: '{}'!", source)))
   );

   let version = try!(
      lib_package.get("version")
         .and_then(toml::Value::as_str)
         .ok_or_else(|| app_err(format!("Couldn't find version string in package: '{}'!", lib_package)))
   );

   match src_type {
      "git" => {
         let commit_hash = try!(
            source.split('#')
               .last()
               .ok_or_else(|| app_err(format!("Couldn't find commit hash in source entry: '{}'!", source)))
         );

         Ok(SourceKind::Git { lib_name: lib_name.to_string(), commit_hash: commit_hash.to_string() })
      },

      "registry" => {
         Ok(SourceKind::CratesIo { lib_name: lib_name.to_string(), version: version.to_string() })
      }

      _ => {
         Err(app_err(format!("Unexpected source type '{}' in package: '{}'!", src_type, lib_package)))
      }
   }
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
         .ok_or_else(|| app_err(format!("Couldn't get Array of Strings for 'dependencies' entry: '{}'!", lib_package)))
   );

   for dep_str in dep_strs.iter() {
      let dep_name = try!(
         dep_str.split(' ')
            .nth(0)
            .ok_or_else(|| app_err(format!("Couldn't get name from dependency: '{}'!", dep_str)))
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
         .ok_or_else(|| app_err(format!("Couldn't find package with name = '{}'!", lib_name)))
   );

   Ok(*package)
}
