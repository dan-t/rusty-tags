use std::path::Path;

use serde_json;
use fnv::FnvHashMap;

use rt_result::RtResult;
use types::{DepTree, Source, SourceVersion, SourceId};
use config::Config;

type SourceMap<'a> = FnvHashMap<SourceVersion<'a>, SourceId>;

/// Returns the dependency tree of the whole cargo workspace.
pub fn dependency_tree(config: &Config, metadata: &serde_json::Value) -> RtResult<DepTree> {
    let workspace_members = workspace_members(metadata)?;
    verbose!(config, "Found workspace members: {:?}", workspace_members);

    let mut dep_tree = DepTree::new();
    let packages = packages(config, metadata, &mut dep_tree)?;

    let resolved_nodes = resolved_nodes(config, metadata, &packages)?;

    let mut source_map = SourceMap::default();
    for member in &workspace_members {
        if let Some(source_id) = build_dep_tree(config, member, 0, &packages, &resolved_nodes,
                                                &mut source_map, &mut dep_tree)? {
            dep_tree.add_root(source_id);
        }
    }

    Ok(dep_tree)
}

fn build_dep_tree<'a>(config: &Config,
                      source_version: &SourceVersion<'a>,
                      depth: usize,
                      packages: &Packages<'a>,
                      resolved_nodes: &ResolvedNodes<'a>,
                      source_map: &mut SourceMap<'a>,
                      dep_tree: &mut DepTree)
                      -> RtResult<Option<SourceId>>
{
    if let Some(source_id) = source_map.get(source_version) {
        verbose!(config, "[{}] Reusing cached tree for {}", depth, source_version);
        return Ok(Some(*source_id));
    }

    let package = packages.get(source_version)
        .ok_or(format!("[{}] Couldn't find package of {}", depth, source_version))?;


    // make source_map entry before recursing into children
    // to handle cyclic dependencies
    source_map.insert(source_version.clone(), package.source_id);

    let dep_source_ids = {
        if config.omit_deps {
            Vec::new()
        } else {
            let mut ids = Vec::new();
            if let Some(dep_versions) = resolved_nodes.get(&package.source_id) {
                for version in dep_versions {
                    if let Some(id) = build_dep_tree(config, version, depth + 1, packages,
                                                     resolved_nodes, source_map, dep_tree)? {
                        ids.push(id);
                    }
                }
            }

            ids
        }
    };

    verbose!(config, "[{}] Building tree for {}", depth, source_version);

    let is_root = depth == 0;
    let source = Source::new(package.source_id, source_version.name, package.source_path, is_root, &config.tags_spec)?;
    dep_tree.set_source(source, dep_source_ids);
    Ok(Some(package.source_id))
}

fn workspace_members(metadata: &serde_json::Value) -> RtResult<Vec<SourceVersion>> {
    let members = metadata.get("workspace_members")
        .and_then(serde_json::Value::as_array)
        .ok_or(format!("Couldn't find array entry 'workspace_members' in metadata:\n{}", to_string_pretty(metadata)))?;

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
    pub source_path: &'a Path
}

type Packages<'a> = FnvHashMap<SourceVersion<'a>, Package<'a>>;

fn packages<'a>(config: &Config,
                metadata: &'a serde_json::Value,
                dep_tree: &mut DepTree)
                -> RtResult<Packages<'a>> {
    let packages = metadata.get("packages")
        .and_then(serde_json::Value::as_array)
        .ok_or(format!("Couldn't find array entry 'packages' in metadata:\n{}", to_string_pretty(metadata)))?;

    dep_tree.reserve_num_sources(packages.len());
    let mut package_map = FnvHashMap::default();
    for package in packages {
        let id = package.get("id")
            .and_then(serde_json::Value::as_str)
            .ok_or(format!("Couldn't find string entry 'id' in package:\n{}", to_string_pretty(package)))?;

        let source_version = SourceVersion::parse_from_id(id)?;

        let source_path = {
            let path = source_path(config, package)?;
            if path == None {
                continue;
            }

            path.unwrap()
        };

        verbose!(config, "Found package of {} with source at '{}'", source_version, source_path.display());

        let source_id = dep_tree.new_source();
        package_map.insert(source_version, Package { source_id, source_path });
    }

    Ok(package_map)
}

type ResolvedNodes<'a> = FnvHashMap<SourceId, Vec<SourceVersion<'a>>>;

fn resolved_nodes<'a>(config: &Config,
                      metadata: &'a serde_json::Value,
                      packages: &Packages)
                      -> RtResult<ResolvedNodes<'a>> {
    let resolve = metadata.get("resolve")
        .and_then(serde_json::Value::as_object)
        .ok_or(format!("Couldn't find object entry 'resolve' in metadata:\n{}", to_string_pretty(metadata)))?;

    let nodes = resolve.get("nodes")
        .and_then(serde_json::Value::as_array)
        .ok_or(format!("Couldn't find array entry 'nodes' in resolve:\n{:?}", resolve))?;

    let mut node_map = ResolvedNodes::default();
    for node in nodes {
        let node_version = {
            let id = node.get("id")
                .and_then(serde_json::Value::as_str)
                .ok_or(format!("Couldn't find string entry 'id' in node:\n{}", to_string_pretty(node)))?;

            SourceVersion::parse_from_id(id)?
        };

        let node_id = packages.get(&node_version)
            .map(|p| p.source_id)
            .ok_or(format!("Couldn't find package for {}", node_version))?;

        let dependencies = node.get("dependencies")
            .and_then(serde_json::Value::as_array)
            .ok_or(format!("Couldn't find array entry 'dependencies' in node:\n{}", to_string_pretty(node)))?;

        let mut dep_versions = Vec::with_capacity(dependencies.len());
        for dep in dependencies {
            let id = dep.as_str()
                .ok_or(format!("Couldn't find string in dependency:\n{}", to_string_pretty(dep)))?;

            dep_versions.push(SourceVersion::parse_from_id(id)?);
        }

        if ! dep_versions.is_empty() {
            verbose!(config, "Found dependencies of {}: {:?}", node_version, dep_versions);
        }

        node_map.insert(node_id, dep_versions);
    }

    Ok(node_map)
}

fn source_path<'a>(config: &Config, package: &'a serde_json::Value) -> RtResult<Option<&'a Path>> {
    let targets = package.get("targets")
        .and_then(serde_json::Value::as_array)
        .ok_or(format!("Couldn't find array entry 'targets' in package:\n{}", to_string_pretty(package)))?;

    let manifest_dir = {
        let manifest_path = package.get("manifest_path")
            .and_then(serde_json::Value::as_str)
            .map(Path::new)
            .ok_or(format!("Couldn't find string entry 'manifest_path' in package:\n{}", to_string_pretty(package)))?;

        manifest_path.parent()
            .ok_or(format!("Couldn't get directory of path '{:?}'", manifest_path.display()))?
    };

    for target in targets {
        let kinds = target.get("kind")
            .and_then(serde_json::Value::as_array)
            .ok_or(format!("Couldn't find array entry 'kind' in target:\n{}", to_string_pretty(target)))?;

        for kind in kinds {
            let kind_str = kind.as_str()
                .ok_or(format!("Expected 'kind' of type string but found: {}", to_string_pretty(kind)))?;

            if kind_str != "bin" && ! kind_str.contains("lib") && kind_str != "proc-macro" {
                verbose!(config, "Unsupported target kind: {}", kind_str);
                continue;
            }

            let mut src_path = target.get("src_path")
                .and_then(serde_json::Value::as_str)
                .map(Path::new)
                .ok_or(format!("Couldn't find string entry 'src_path' in target:\n{}", to_string_pretty(target)))?;

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

fn to_string_pretty(value: &serde_json::Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or(String::new())
}
