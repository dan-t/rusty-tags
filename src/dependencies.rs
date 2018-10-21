use std::path::Path;

use serde_json;
use fnv::FnvHashMap;
use semver::{Version, VersionReq};

use rt_result::RtResult;
use types::{DepTree, Source, SourceReq, SourceId};
use config::Config;

type SourceName = str;
type SourceMap<'a> = FnvHashMap<&'a SourceName, Vec<(Version, SourceId)>>;

/// Returns the dependency tree of the whole cargo workspace.
pub fn dependency_tree(config: &Config, metadata: &serde_json::Value) -> RtResult<DepTree> {
    let packages = packages(&metadata)?;
    let workspace_members = workspace_members(&metadata)?;
    verbose!(config, "Found workspace members: {:?}", workspace_members);

    let mut source_map = SourceMap::default();
    let mut dep_tree = DepTree::new();
    for member in &workspace_members {
        if let Some(source_id) = build_dep_tree(config, member, 0, packages, &mut source_map, &mut dep_tree)? {
            dep_tree.add_root(source_id);
        }
    }

    Ok(dep_tree)
}

fn build_dep_tree<'a>(config: &Config,
                      source_req: &SourceReq<'a>,
                      level: usize,
                      packages: &'a Vec<serde_json::Value>,
                      source_map: &mut SourceMap<'a>,
                      dep_tree: &mut DepTree)
                      -> RtResult<Option<SourceId>>
{
    if let Some(sources) = source_map.get(source_req.name) {
        for (version, source_id) in sources {
            if source_req.req.matches(version) {
                verbose!(config, "[{}] Reusing cached tree of {} for req {}", level, display(&source_req.name, &version), source_req.req);
                return Ok(Some(*source_id));
            }
        }
    }

    let package = find_package(source_req, packages)?;
    if package == None {
        return Ok(None);
    }
    let package = package.unwrap();

    let version = version(package)?;
    verbose!(config, "[{}] Found package for {}", level, display(&source_req.name, &version));

    let is_root = level == 0;
    let source_path = source_path(config, package, is_root)?;
    if source_path == None {
        return Ok(None);
    }
    let source_path = source_path.unwrap();

    let source_id = dep_tree.new_source();
    {
        let sources = source_map.entry(source_req.name).or_insert(Vec::with_capacity(10));
        sources.push((version.clone(), source_id));
    }

    let mut dep_source_ids = Vec::new();
    if ! config.omit_deps {
        let deps = dependencies(package)?;
        if ! deps.is_empty() {
            dep_source_ids.reserve(deps.len());
            verbose!(config, "[{}] Found dependencies of {}: {:?}", level, display(&source_req.name, &version), deps);
        }

        for dep in &deps {
            if let Some(source_id) = build_dep_tree(config, dep, level + 1, packages, source_map, dep_tree)? {
                dep_source_ids.push(source_id);
            }
        }
    }

    verbose!(config, "[{}] Building tree for {}", level, display(&source_req.name, &version));

    let source = Source::new(source_id, source_req.name, source_path, is_root, &config.tags_spec)?;
    dep_tree.set_source(source, dep_source_ids);
    Ok(Some(source_id))
}

fn workspace_members<'a>(metadata: &'a serde_json::Value) -> RtResult<Vec<SourceReq<'a>>> {
    let members = metadata.get("workspace_members")
        .and_then(serde_json::Value::as_array)
        .ok_or(format!("Couldn't find array entry 'workspace_members' in metadata:\n{}", to_string_pretty(metadata)))?;

    let mut source_reqs = Vec::with_capacity(50);
    for member in members {
        let member_str = member.as_str()
            .ok_or(format!("Expected 'workspace_members' of type string but found: {}", to_string_pretty(member)))?;

        let mut split = member_str.split(' ');
        let name = split.next();
        if name == None {
            return Err(format!("Couldn't extract 'workspace_members' name from string: '{}'", member_str).into());
        }

        let version = split.next();
        if version == None {
            return Err(format!("Couldn't extract 'workspace_members' version from string: '{}'", member_str).into());
        }

        source_reqs.push(SourceReq::new(name.unwrap(), VersionReq::parse(version.unwrap())?));
    }

    Ok(source_reqs)
}

fn packages(metadata: &serde_json::Value) -> RtResult<&Vec<serde_json::Value>> {
    metadata.get("packages")
        .and_then(serde_json::Value::as_array)
        .ok_or(format!("Couldn't find array entry 'packages' in metadata:\n{}", to_string_pretty(metadata)).into())
}

fn find_package<'a>(source_req: &SourceReq<'a>, packages: &'a Vec<serde_json::Value>) -> RtResult<Option<&'a serde_json::Value>> {
    for package in packages {
        let name = package.get("name")
            .and_then(serde_json::Value::as_str)
            .ok_or(format!("Couldn't find string entry 'name' in package:\n{}", to_string_pretty(package)))?;

        let version = package.get("version")
            .and_then(serde_json::Value::as_str)
            .ok_or(format!("Couldn't find string entry 'version' in package:\n{}", to_string_pretty(package)))?;

        let version = Version::parse(version)?;
        if name == source_req.name && source_req.req.matches(&version) {
            return Ok(Some(package));
        }
    }

    Ok(None)
}

fn version(package: &serde_json::Value) -> RtResult<Version> {
    let version = package.get("version")
        .and_then(serde_json::Value::as_str)
        .ok_or(format!("Couldn't find string entry 'version' in package:\n{}", to_string_pretty(package)))?;

    Ok(Version::parse(version)?)
}

fn dependencies<'a>(package: &'a serde_json::Value) -> RtResult<Vec<SourceReq<'a>>> {
    let deps = package.get("dependencies")
        .and_then(serde_json::Value::as_array)
        .ok_or(format!("Couldn't find array entry 'dependencies' in package:\n{}", to_string_pretty(package)))?;

    let mut source_reqs = Vec::new();
    for dep in deps {
        let name = dep.get("name")
            .and_then(serde_json::Value::as_str)
            .ok_or(format!("Couldn't find string entry 'name' in dependency:\n{}", to_string_pretty(dep)))?;

        let req = dep.get("req")
            .and_then(serde_json::Value::as_str)
            .ok_or(format!("Couldn't find string entry 'req' in dependency:\n{}", to_string_pretty(dep)))?;
        let req = VersionReq::parse(req)?;

        source_reqs.push(SourceReq::new(name, req));
    }

    source_reqs.sort_unstable();
    Ok(source_reqs)
}

fn source_path<'a>(config: &Config, package: &'a serde_json::Value, is_root: bool) -> RtResult<Option<&'a Path>> {
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

            if is_root {
                if kind_str != "bin" && ! kind_str.contains("lib") && kind_str != "proc-macro" {
                    verbose!(config, "Unsupported target kind for root: {}", kind_str);
                    continue;
                }
            } else {
                if ! kind_str.contains("lib") {
                    verbose!(config, "Unsupported target kind for dependency: {}", kind_str);
                    continue;
                }
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

fn display(name: &str, version: &Version) -> String {
    format!("({}, {})", name, version)
}
