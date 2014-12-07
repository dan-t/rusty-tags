use std::io;
use toml;

use app_result::{AppResult, app_err};

use types::{
   TagsRoot,
   TagsRoots,
   SourceKind
};

/// Reads the dependecies from the `Cargo.toml` located in `cargo_toml_dir`
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
   let deps_string = "dependencies".to_string();
   if ! toml_table.contains_key(&deps_string) {
      return Ok(tags_roots);
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
               .collect::<Vec<&toml::Table>>();

            if ! tables.is_empty() { Some(tables) } else { None }
         })
         .ok_or(app_err(format!("Couldn't get Array of Tables entry for 'package'!")))
   );

   for package in packages.iter() {
      let lib_name = try!(
         package.get(&"name".to_string())
            .and_then(|n| n.as_str().map(|n| n.to_string()))
            .ok_or_else(|| app_err(format!("Couldn't find name string in package: '{}'!", package)))
      );

      let dep_names = try!(get_dependencies(*package));
      let mut dep_src_kinds: Vec<SourceKind> = Vec::new();
      for dep_name in dep_names.iter() {
         let dep_package = try!(find_package(&packages, dep_name));
         dep_src_kinds.push(try!(get_source_kind(dep_package, dep_name)));
      }

      let src_kind = try!(get_source_kind(*package, &lib_name));
      tags_roots.push(TagsRoot::Lib { src_kind: src_kind, dependencies: dep_src_kinds });
   }

   let mut lib_src_kinds: Vec<SourceKind> = Vec::new();
   for (lib_name, value) in deps_table.iter() {
      match *value {
         toml::Value::String(_) | toml::Value::Table(_) => {
            let lib_package = try!(find_package(&packages, lib_name));
            lib_src_kinds.push(try!(get_source_kind(lib_package, lib_name)));
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

fn get_source_kind(lib_package: &toml::Table, lib_name: &String) -> AppResult<SourceKind>
{
   let version_str = &String::from_str("version");
   let source_str = &String::from_str("source");

   let source = try!(
      lib_package.get(source_str)
         .and_then(|src| src.as_str())
         .ok_or_else(|| app_err(format!("Couldn't find source string in package: '{}'!", lib_package)))
   );

   let src_type = try!(
      source.split('+')
         .nth(0)
         .ok_or_else(|| app_err(format!("Couldn't find source type in: '{}'!", source)))
   );

   let version = try!(
      lib_package.get(version_str)
         .and_then(|vers| vers.as_str())
         .ok_or_else(|| app_err(format!("Couldn't find version string in package: '{}'!", lib_package)))
   );

   match src_type {
      "git" => {
         let commit_hash = try!(
            source.split('#')
               .last()
               .ok_or_else(|| app_err(format!("Couldn't find commit hash in source entry: '{}'!", source)))
         );

         Ok(SourceKind::Git { lib_name: lib_name.clone(), commit_hash: commit_hash.to_string() })
      },

      "registry" => {
         Ok(SourceKind::CratesIo { lib_name: lib_name.clone(), version: version.to_string() })
      }

      _ => {
         Err(app_err(format!("Unexpected source type '{}' in package: '{}'!", src_type, lib_package)))
      }
   }
}

fn get_dependencies(lib_package: &toml::Table) -> AppResult<Vec<String>>
{
   let deps_str = &String::from_str("dependencies");
   let dep_strs = match lib_package.get(deps_str) {
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
               .ok_or_else(|| app_err(format!("Couldn't get Array of Strings for 'dependencies' entry: '{}'!", lib_package)))
         )
      }
   };

   let mut dep_names: Vec<String> = Vec::new();
   for dep_str in dep_strs.iter() {
      let dep_name = try!(
         dep_str.split(' ')
            .nth(0)
            .map(|d| d.to_string())
            .ok_or_else(|| app_err(format!("Couldn't get name from dependency: '{}'!", dep_str)))
      );

      dep_names.push(dep_name);
   }

   Ok(dep_names)
}

fn find_package<'a>(packages: &'a Vec<&toml::Table>, lib_name: &String) -> AppResult<&'a toml::Table>
{
   let name_str = &String::from_str("name");

   let package = try!(
      packages.iter()
         .find(|t| t.get(name_str).and_then(|n| n.as_str()) == Some(lib_name[]))
         .ok_or_else(|| app_err(format!("Couldn't find package with name = '{}'!", lib_name)))
   );

   Ok(*package)
}
