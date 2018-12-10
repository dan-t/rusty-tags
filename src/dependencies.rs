use std::path::Path;

use serde_json;
use fnv::FnvHashMap;

use rt_result::RtResult;
use types::{DepTree, Source, SourceVersion, SourceId};
use config::Config;

type JsonValue = serde_json::Value;
type JsonObject = serde_json::Map<String, JsonValue>;

/// Returns the dependency tree of the whole cargo workspace.
pub fn dependency_tree(config: &Config, metadata: &JsonValue) -> RtResult<DepTree> {
    let mut dep_tree = DepTree::new();
    let packages = packages(config, metadata, &mut dep_tree)?;

    build_dep_tree(config, metadata, &packages, &mut dep_tree)?;

    Ok(dep_tree)
}

fn workspace_members(metadata: &JsonValue) -> RtResult<Vec<SourceVersion>> {
    let members = as_array_from_value("workspace_members", metadata)?;
    let mut source_versions = Vec::with_capacity(members.len());
    for member in members {
        let member_str = member.as_str()
            .ok_or(format!("Expected 'workspace_members' of type string but found: {}", to_string_pretty(member)))?;

        source_versions.push(SourceVersion::parse_from_id(member_str)?)
    }

    Ok(source_versions)
}

struct Package<'a> {
    pub source_id: SourceId,
    pub source_path: &'a Path,
    pub manifest_path: &'a Path
}

type Packages<'a> = FnvHashMap<SourceVersion<'a>, Package<'a>>;

fn packages<'a>(config: &Config,
                metadata: &'a JsonValue,
                dep_tree: &mut DepTree)
                -> RtResult<Packages<'a>> {
    let packages = as_array_from_value("packages", metadata)?;
    dep_tree.reserve_num_sources(packages.len());
    let mut package_map = FnvHashMap::default();
    for package in packages {
        let id = as_str_from_value("id", package)?;
        let source_version = SourceVersion::parse_from_id(id)?;

        if let Some((source_path, manifest_path)) = source_and_manifest_paths(config, package)? {
            verbose!(config, "Found package of {} with source at '{}' and manifest at '{}'", source_version, source_path.display(), manifest_path.display());

            let source_id = dep_tree.new_source();
            package_map.insert(source_version, Package { source_id, source_path, manifest_path });
        };
    }

    Ok(package_map)
}

fn build_dep_tree(config: &Config,
                  metadata: &JsonValue,
                  packages: &Packages,
                  dep_tree: &mut DepTree)
                  -> RtResult<()> {
    let root_ids = {
        let workspace_members = workspace_members(metadata)?;
        verbose!(config, "Found workspace members: {:?}", workspace_members);

        let mut ids = Vec::with_capacity(workspace_members.len());
        for member in &workspace_members {
            ids.push(package(member, packages)?.source_id);
        }

        ids
    };

    dep_tree.set_roots(root_ids.clone());

    let nodes = {
        let resolve = as_object_from_value("resolve", metadata)?;
        as_array_from_object("nodes", resolve)?
    };

    for node in nodes {
        let node_version = {
            let id = as_str_from_value("id", node)?;
            SourceVersion::parse_from_id(id)?
        };

        let node_package = package(&node_version, packages)?;

        let dep_ids = {
            let dependencies = as_array_from_value("dependencies", node)?;

            let dep_versions = {
                let mut vers = Vec::with_capacity(dependencies.len());
                for dep in dependencies {
                    let id = dep.as_str()
                        .ok_or(format!("Couldn't find string in dependency:\n{}", to_string_pretty(dep)))?;

                    vers.push(SourceVersion::parse_from_id(id)?);
                }

                vers
            };

            if ! dep_versions.is_empty() {
                verbose!(config, "Found dependencies of {}: {:?}", node_version, dep_versions);
            }

            let mut ids = Vec::with_capacity(dep_versions.len());
            for version in &dep_versions {
                ids.push(package(version, packages)?.source_id);
            }

            ids
        };

        verbose!(config, "Building tree for {}", node_version);

        let is_root = root_ids.iter().find(|id| **id == node_package.source_id) != None;
        let source = Source::new(node_package.source_id, &node_version, node_package.source_path, node_package.manifest_path, is_root, config)?;
        dep_tree.set_source(source, dep_ids);
    }

    Ok(())
}

fn package<'a>(source_version: &SourceVersion<'a>, packages: &'a Packages) -> RtResult<&'a Package<'a>> {
    packages.get(&source_version)
        .ok_or(format!("Couldn't find package for {}", source_version).into())
}

fn source_and_manifest_paths<'a>(config: &Config, package: &'a JsonValue) -> RtResult<Option<(&'a Path, &'a Path)>> {
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

            if kind_str != "bin" && ! kind_str.contains("lib") && kind_str != "proc-macro" {
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

            return Ok(Some((src_path, manifest_dir)));
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
