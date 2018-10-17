use std::path::Path;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use serde_json;

use rt_result::RtResult;
use types::{DepTree, Source, SourceKind};
use config::Config;

type SourceName = str;

/// Returns the dependency tree of the cargo project.
pub fn dependency_trees(config: &Config, metadata: &serde_json::Value) -> RtResult<Vec<Arc<DepTree>>> {
    let packages = packages(&metadata)?;
    let root_names = root_names(&metadata)?;
    verbose!(config, "Found workspace members: '{:?}'", root_names);

    let mut dep_trees = Vec::new();
    let mut dep_tree_cache: HashMap<&SourceName, Arc<DepTree>> = HashMap::new();
    for name in &root_names {
        let mut dep_graph = DepGraph::new();
        if let Some(tree) = build_dep_tree(config, name, SourceKind::Root, packages,
                                           &mut dep_graph, &mut dep_tree_cache, 0)? {
            dep_trees.push(tree);
        }
    }

    Ok(dep_trees)
}

struct DepGraph<'a> {
    dep_graph: Vec<&'a str>,
    sorted_deps: HashSet<&'a str>
}

impl<'a> DepGraph<'a> {
    pub fn new() -> DepGraph<'a> {
        DepGraph {
            dep_graph: Vec::new(),
            sorted_deps: HashSet::new()
        }
    }

    pub fn push(&mut self, dep: &'a str) {
        self.dep_graph.push(dep);
        self.sorted_deps.insert(dep);
    }

    pub fn pop(&mut self) {
        if let Some(dep) = self.dep_graph.pop() {
            self.sorted_deps.remove(dep);
        }
    }

    pub fn contains(&self, dep: &str) -> bool {
        self.sorted_deps.contains(dep)
    }

    pub fn get(&self) -> &Vec<&'a str> {
        &self.dep_graph
    }
}

fn build_dep_tree<'a>(config: &Config,
                      src_name: &'a str,
                      kind: SourceKind,
                      packages: &'a Vec<serde_json::Value>,
                      dep_graph: &mut DepGraph<'a>,
                      dep_tree_cache: &mut HashMap<&'a SourceName, Arc<DepTree>>,
                      level: u32)
                      -> RtResult<Option<Arc<DepTree>>> {
    if dep_graph.contains(src_name) {
        verbose!(config, "\n[{}] Found cyclic dependency on source {:?}='{}' in dependency graph:\n{:?}\n", level, kind, src_name, dep_graph.get());
        return Ok(None);
    }

    if let Some(dep_tree) = dep_tree_cache.get(src_name) {
        verbose!(config, "[{}] Reusing cached tree for source {:?}='{}'", level, kind, src_name);
        return Ok(Some(dep_tree.clone()));
    }

    dep_graph.push(src_name);

    let mut opt_dep_tree = None;
    if let Some(pkg) = find_package(src_name, packages) {
        verbose!(config, "[{}] Found package of {:?}='{}'", level, kind, src_name);
        if let Some(src_path) = src_path(config, pkg, kind)? {
            let mut dep_trees = Vec::new();
            if !config.omit_deps {
                let dep_names = dependency_names(pkg)?;
                if ! dep_names.is_empty() {
                    verbose!(config, "[{}] Found dependencies of {:?}='{}': '{:?}'", level, kind, src_name, dep_names);
                }

                for name in &dep_names {
                    if let Some(tree) = build_dep_tree(config, name, SourceKind::Dep, packages,
                                                       dep_graph, dep_tree_cache, level + 1)? {
                        dep_trees.push(tree);
                    }
                }
            }

            verbose!(config, "[{}] Building tree for source {:?}='{}'", level, kind, src_name);
            let dep_tree = Arc::new(DepTree {
                source: Source::new(kind, src_name, src_path, &config.tags_spec)?,
                dependencies: dep_trees
            });

            dep_tree_cache.insert(src_name, dep_tree.clone());
            opt_dep_tree = Some(dep_tree);
        }
    }

    dep_graph.pop();
    Ok(opt_dep_tree)
}

fn root_names(metadata: &serde_json::Value) -> RtResult<Vec<&str>> {
    let members = metadata.get("workspace_members")
        .and_then(serde_json::Value::as_array)
        .ok_or(format!("Couldn't find array entry 'workspace_members' in metadata:\n{}", to_string_pretty(metadata)))?;

    let mut names = Vec::new();
    for member in members {
        let member_str = member.as_str()
            .ok_or(format!("Expected 'workspace_members' of type string but found: {}", to_string_pretty(member)))?;

        let name = member_str.split(' ')
            .nth(0)
            .ok_or(format!("Couldn't extract 'workspace_members' name from string: '{}'", member_str))?;

        names.push(name);
    }

    Ok(names)
}

fn packages(metadata: &serde_json::Value) -> RtResult<&Vec<serde_json::Value>> {
    metadata.get("packages")
        .and_then(serde_json::Value::as_array)
        .ok_or(format!("Couldn't find array entry 'packages' in metadata:\n{}", to_string_pretty(metadata)).into())
}

fn find_package<'a>(name: &str, packages: &'a Vec<serde_json::Value>) -> Option<&'a serde_json::Value> {
    for package in packages {
        if Some(name) == package.get("name").and_then(serde_json::Value::as_str) {
            return Some(package);
        }
    }

    None
}

fn dependency_names(package: &serde_json::Value) -> RtResult<Vec<&str>> {
    let deps = package.get("dependencies")
        .and_then(serde_json::Value::as_array)
        .ok_or(format!("Couldn't find array entry 'dependencies' in package:\n{}", to_string_pretty(package)))?;

    let mut names = Vec::new();
    for dep in deps {
        let name = dep.get("name")
            .and_then(serde_json::Value::as_str)
            .ok_or(format!("Couldn't find string entry 'name' in dependency:\n{}", to_string_pretty(dep)))?;

        names.push(name);
    }

    names.sort();
    names.dedup();
    Ok(names)
}

fn src_path<'a>(config: &Config, package: &'a serde_json::Value, source_kind: SourceKind) -> RtResult<Option<&'a Path>> {
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

            match source_kind {
                SourceKind::Root => {
                    if kind_str != "bin" && ! kind_str.contains("lib") && kind_str != "proc-macro" {
                        verbose!(config, "Unsupported target kind for root: {}", kind_str);
                        continue;
                    }
                },

                SourceKind::Dep => {
                    if ! kind_str.contains("lib") {
                        verbose!(config, "Unsupported target kind for dependency: {}", kind_str);
                        continue;
                    }
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
