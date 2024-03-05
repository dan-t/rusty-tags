use std::path::Path;

use semver::Version;
use fnv::FnvHashMap;

use rt_result::RtResult;
use types::{DepTree, Source, SourceId};
use config::Config;

type JsonValue = serde_json::Value;
type JsonObject = serde_json::Map<String, JsonValue>;

/// Returns the dependency tree of the whole cargo workspace.
pub fn dependency_tree(config: &Config, metadata: &JsonValue) -> RtResult<DepTree> {
    let mut dep_tree = DepTree::new();
    let packages = packages(config, metadata, &mut dep_tree)?;

    build_dep_tree(config, metadata, &packages, &mut dep_tree)?;
    dep_tree.compute_depths();

    Ok(dep_tree)
}

fn workspace_members<'a>(metadata: &'a JsonValue) -> RtResult<Vec<PackageId<'a>>> {
    let members = as_array_from_value("workspace_members", metadata)?;
    let mut member_ids = Vec::with_capacity(members.len());
    for member in members {
        let member_id = member.as_str()
            .ok_or(format!("Expected 'workspace_members' of type string but found: {}", to_string_pretty(member)))?;

        member_ids.push(member_id);
    }

    Ok(member_ids)
}

type PackageId<'a> = &'a str;

struct Package<'a> {
    pub name: &'a str,
    pub version: Version,
    pub source_id: SourceId,
    pub source_path: &'a Path
}

type Packages<'a> = FnvHashMap<PackageId<'a>, Package<'a>>;

fn packages<'a>(config: &Config,
                metadata: &'a JsonValue,
                dep_tree: &mut DepTree)
                -> RtResult<Packages<'a>> {
    let packages = as_array_from_value("packages", metadata)?;
    dep_tree.reserve_num_sources(packages.len());
    let mut package_map = FnvHashMap::default();
    for package in packages {
        let id = as_str_from_value("id", package)?;
        let name = as_str_from_value("name", package)?;
        let version = Version::parse(as_str_from_value("version", package)?)?;
        let source_path = {
            let path = source_path(config, package)?;
            if path == None {
                continue;
            }

            path.unwrap()
        };

        verbose!(config, "Found package of {} {} with source at '{}'", name, version, source_path.display());

        let source_id = dep_tree.new_source();
        package_map.insert(id, Package { name, version, source_id, source_path });
    }

    Ok(package_map)
}

fn build_dep_tree(config: &Config,
                  metadata: &JsonValue,
                  packages: &Packages,
                  dep_tree: &mut DepTree)
                  -> RtResult<()> {
    let root_ids = {
        let members_ids = workspace_members(metadata)?;
        verbose!(config, "Found workspace members: {:?}", members_ids);

        let mut source_ids = Vec::with_capacity(members_ids.len());
        for member_id in &members_ids {
            let member_package = package(&member_id, packages)?;
            source_ids.push(member_package.source_id);
            if config.omit_deps {
                let is_root = true;
                let source = Source::new(member_package.source_id, member_package.name, &member_package.version,
                                         member_package.source_path, is_root, config)?;
                dep_tree.set_source(source, vec![]);
            }
        }

        source_ids
    };

    dep_tree.set_roots(root_ids.clone());
    if config.omit_deps {
        return Ok(());
    }

    let nodes = {
        let resolve = as_object_from_value("resolve", metadata)?;
        as_array_from_object("nodes", resolve)?
    };

    for node in nodes {
        let node_id = as_str_from_value("id", node)?;
        let node_package = package(&node_id, packages)?;

        let dep_src_ids = {
            let dependencies = as_array_from_value("dependencies", node)?;
            let dep_pkg_ids = {
                let mut pkg_ids = Vec::with_capacity(dependencies.len());
                for dep in dependencies {
                    let pkg_id = dep.as_str()
                        .ok_or(format!("Couldn't find string in dependency:\n{}", to_string_pretty(dep)))?;

                    pkg_ids.push(pkg_id);
                }

                pkg_ids
            };

            if ! dep_pkg_ids.is_empty() {
                verbose!(config, "Found dependencies of {} {}: {:?}", node_package.name, node_package.version, dep_pkg_ids);
            }

            let mut src_ids = Vec::with_capacity(dep_pkg_ids.len());
            for pkg_id in &dep_pkg_ids {
                src_ids.push(package(&pkg_id, packages)?.source_id);
            }

            src_ids
        };

        verbose!(config, "Building tree for {} {}", node_package.name, node_package.version);

        let is_root = root_ids.iter().find(|id| **id == node_package.source_id) != None;
        let source = Source::new(node_package.source_id, node_package.name, &node_package.version,
                                 node_package.source_path, is_root, config)?;
        dep_tree.set_source(source, dep_src_ids);
    }

    Ok(())
}

fn package<'a>(package_id: &PackageId<'a>, packages: &'a Packages) -> RtResult<&'a Package<'a>> {
    packages.get(package_id)
        .ok_or(format!("Couldn't find package for id '{}'", package_id).into())
}

fn source_path<'a>(config: &Config, package: &'a JsonValue) -> RtResult<Option<&'a Path>> {
    let targets = as_array_from_value("targets", package)?;

    let manifest_dir = {
        let manifest_path = as_str_from_value("manifest_path", package).map(Path::new)?;

        manifest_path.parent()
            .ok_or(format!("Couldn't get directory of path '{:?}'", manifest_path.display()))?
    };

    for target in targets {
        let kinds = as_array_from_value("kind", target)?;

        for kind in kinds {
            let kind_str = kind.as_str()
                .ok_or(format!("Expected 'kind' of type string but found: {}", to_string_pretty(kind)))?;

            if kind_str != "bin" && ! kind_str.contains("lib") && kind_str != "proc-macro" && kind_str != "test" {
                verbose!(config, "Unsupported target kind: {}", kind_str);
                continue;
            }

            let mut src_path = as_str_from_value("src_path", target).map(Path::new)?;
            if src_path.is_absolute() && src_path.is_file() {
                src_path = src_path.parent()
                    .ok_or(format!("Couldn't get directory of path '{:?}' in target:\n{}\nof package:\n{}",
                                   src_path.display(), to_string_pretty(target), to_string_pretty(package)))?;
            }

            if src_path.is_relative() {
                src_path = manifest_dir;
            }

            if ! src_path.is_dir() {
                return Err(format!("Invalid source path directory '{:?}' in target:\n{}\nof package:\n{}",
                                   src_path.display(), to_string_pretty(target), to_string_pretty(package)).into());
            }

            return Ok(Some(src_path));
        }
    }

    Ok(None)
}

fn to_string_pretty(value: &JsonValue) -> String {
    serde_json::to_string_pretty(value).unwrap_or(String::new())
}

fn as_array_from_value<'a>(entry: &str, value: &'a JsonValue) -> RtResult<&'a Vec<JsonValue>> {
    value.get(entry)
         .and_then(JsonValue::as_array)
         .ok_or(format!("Couldn't find array entry '{}' in:\n{}", entry, to_string_pretty(value)).into())
}

fn as_str_from_value<'a>(entry: &str, value: &'a JsonValue) -> RtResult<&'a str> {
    value.get(entry)
         .and_then(JsonValue::as_str)
         .ok_or(format!("Couldn't find string entry '{}' in:\n{}", entry, to_string_pretty(value)).into())
}

fn as_object_from_value<'a>(entry: &str, value: &'a JsonValue) -> RtResult<&'a JsonObject> {
    value.get(entry)
         .and_then(JsonValue::as_object)
         .ok_or(format!("Couldn't find object entry '{}' in:\n{}", entry, to_string_pretty(value)).into())
}

fn as_array_from_object<'a>(entry: &str, object: &'a JsonObject) -> RtResult<&'a Vec<JsonValue>> {
    object.get(entry)
          .and_then(JsonValue::as_array)
          .ok_or(format!("Couldn't find array entry '{}' in:\n{:?}", entry, object).into())
}
