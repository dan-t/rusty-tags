use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use toml;

use app_result::AppResult;
use types::{DepTree, SourceKind};

/// Returns the dependency tree of the cargo project.
pub fn read_dependencies(cargo_toml_dir: &Path) -> AppResult<DepTree> {
    let toml_table = try!(parse_toml(&cargo_toml_dir.join("Cargo.toml")));

    // The direct dependencies of the cargo project have to be handeled differently then the
    // indirect ones - the dependencies of the dependencies - because the path for
    // local path dependencies isn't contained in the 'Cargo.lock' and has to be
    // read from the 'Cargo.toml'
    let deps = try!(direct_dependencies(&toml_table));
    if deps.is_empty() {
        return Ok(DepTree {
            source: SourceKind::Root { path: cargo_toml_dir.to_path_buf() },
            dependencies: Vec::new()
        });
    }

    let lock_table = try!(parse_toml(&cargo_toml_dir.join("Cargo.lock")));

    let packages: Vec<&toml::Table> = try!(
        lock_table.get("package")
            .and_then(toml::Value::as_slice)
            .map(|s| {
                s.iter()
                 .filter_map(toml::Value::as_table)
                 .collect()
        })
        .ok_or(format!("Couldn't get Array of Tables entry for 'package'!"))
    );

    let mut dep_trees = Vec::new();
    for dep in &deps {
        let src = try!(get_source_kind_of_dep(dep, cargo_toml_dir, &packages));
        dep_trees.push(Box::new(try!(build_dep_tree(src, &packages))));
    }

    Ok(DepTree {
        source: SourceKind::Root { path: cargo_toml_dir.to_path_buf() },
        dependencies: dep_trees
    })
}

fn build_dep_tree(source: SourceKind, packages: &Vec<&toml::Table>) -> AppResult<DepTree> {
    let pkg = try!(find_package(&packages, source.get_lib_name()));
    let deps = try!(get_dependencies(&pkg));

    let mut dep_trees = Vec::new();
    for dep in &deps {
        let dep_pkg = try!(find_package(&packages, *dep));
        let dep_src = try!(get_source_kind(&dep_pkg, *dep));
        dep_trees.push(Box::new(try!(build_dep_tree(dep_src, &packages))));
    }

    Ok(DepTree {
        source: source,
        dependencies: dep_trees
    })
}

fn get_source_kind_of_dep(&(lib_name, value): &Dep,
                          cargo_toml_dir: &Path,
                          packages: &Vec<&toml::Table>)
                          -> AppResult<SourceKind> {
    match *value {
        // handling crates.io dependencies with a version
        toml::Value::String(_) => {
            let pkg = try!(find_package(&packages, lib_name));
            Ok(try!(get_source_kind(pkg, lib_name)))
        }

        toml::Value::Table(ref table) => {
            // handling of local path dependencies
            if let Some(path) = table.get("path") {
                let mut path = try!(
                    path.as_str().ok_or_else(|| {
                        format!("Expected a String for 'path' entry in '{}'", value)
                    })
                    .map(PathBuf::from)
                );

                if path.is_relative() {
                    let mut abs_path = cargo_toml_dir.to_path_buf();
                    abs_path.push(path);
                    path = abs_path;
                }

                Ok(SourceKind::Path { lib_name: lib_name.to_string(), path: path })
            // handling of git and crates.io dependencies (may have additional parameters set: optional, etc.)
            } else if table.get("version").is_some() || table.get("git").is_some() {
                let pkg = try!(find_package(&packages, lib_name));
                Ok(try!(get_source_kind(pkg, lib_name)))
            } else {
                Err(format!("Couldn't find a 'path', 'version' or 'git' attribute for '{}' in '{}'", lib_name, value).into())
            }
        }

        _ => {
            Err(format!("Expected a String or a Table for the dependency with the name '{}', but got: '{}'", lib_name, value).into())
        }
    }
}

fn get_source_kind(lib_package: &toml::Table, lib_name: &str) -> AppResult<SourceKind> {
    let source = try!(
        lib_package.get("source")
            .and_then(toml::Value::as_str)
            .ok_or_else(|| format!("Couldn't find source string in package: '{:?}'!", lib_package))
    );

    let src_type = try!(
        source.split('+')
            .nth(0)
            .ok_or_else(|| format!("Couldn't find source type in: '{}'!", source))
    );

    let version = try!(
        lib_package.get("version")
            .and_then(toml::Value::as_str)
            .ok_or_else(|| format!("Couldn't find version string in package: '{:?}'!", lib_package))
    );

    match src_type {
        "git" => {
            let commit_hash = try!(
                source.split('#')
                    .last()
                    .ok_or_else(|| format!("Couldn't find commit hash in source entry: '{}'!", source))
            );

            Ok(SourceKind::Git { lib_name: lib_name.to_string(), commit_hash: commit_hash.to_string() })
        },

        "registry" => {
            Ok(SourceKind::CratesIo { lib_name: lib_name.to_string(), version: version.to_string() })
        }

        _ => {
            Err(format!("Unexpected source type '{}' in package: '{:?}'!", src_type, lib_package).into())
        }
    }
}

/// the name of a dependency
type DepName = String;
type DepVal = toml::Value;
type Dep<'a> = (&'a DepName, &'a DepVal);

/// Collects all direct dependencies specified in the `Cargo.toml`. The dependencies might
/// be specified by `dependencies`, `build-dependencies` or `dev-dependencies`.
fn direct_dependencies(cargo_toml: &toml::Table) -> AppResult<Vec<Dep>> {
    let mut deps = Vec::with_capacity(50);
    for dep_type in &["dependencies", "build-dependencies", "dev-dependencies"] {
        if let Some(deps_value) = cargo_toml.get(*dep_type) {
            let deps_table = try!(
                deps_value.as_table()
                    .ok_or(format!("Couldn't get toml::Table entry for '{}'! Got a '{}'!", dep_type, deps_value))
            );

            for dep in deps_table {
                deps.push(dep);
            }
        }
    }

    Ok(deps)
}

fn get_dependencies(lib_package: &toml::Table) -> AppResult<Vec<&str>> {
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
        .ok_or_else(|| format!("Couldn't get Array of Strings for 'dependencies' entry: '{:?}'!", lib_package))
    );

    for dep_str in dep_strs.iter() {
        let dep_name = try!(
            dep_str.split(' ')
                .nth(0)
                .ok_or_else(|| format!("Couldn't get name from dependency: '{}'!", dep_str))
            );

        dep_names.push(dep_name);
    }

    Ok(dep_names)
}

fn find_package<'a>(packages: &'a Vec<&toml::Table>, lib_name: &str) -> AppResult<&'a toml::Table> {
    let package = try!(
        packages.iter()
            .find(|p| p.get("name").and_then(toml::Value::as_str) == Some(lib_name))
            .ok_or_else(|| format!("Couldn't find package with name = '{}'!", lib_name))
    );

    Ok(*package)
}

fn parse_toml(path: &Path) -> AppResult<toml::Table> {
    let mut file = try!(File::open(path));
    let mut string = String::new();
    try!(file.read_to_string(&mut string));
    let mut parser = toml::Parser::new(&string);
    parser.parse().ok_or_else(|| format!("Couldn't parse '{}': {:?}", path.display(), parser.errors).into())
}
