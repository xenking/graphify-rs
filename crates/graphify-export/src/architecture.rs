//! Compact service architecture manifest and LikeC4 service/landscape export.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use graphify_core::graph::KnowledgeGraph;
use graphify_core::model::{GraphEdge, GraphNode, NodeType};
use graphify_core::quality;
use serde::{Deserialize, Serialize};
use tracing::info;

const ROOT_ID: &str = "graphify";
const DEFAULT_MAX_PACKAGES: usize = 80;
const DEFAULT_MAX_DEPENDENCIES: usize = 150;
const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchitectureOptions {
    pub service_name: Option<String>,
    pub root_path: Option<PathBuf>,
    pub max_packages: usize,
    pub max_dependencies: usize,
    pub include_paths: Vec<String>,
    pub exclude_paths: Vec<String>,
}

impl Default for ArchitectureOptions {
    fn default() -> Self {
        Self {
            service_name: None,
            root_path: None,
            max_packages: DEFAULT_MAX_PACKAGES,
            max_dependencies: DEFAULT_MAX_DEPENDENCIES,
            include_paths: Vec::new(),
            exclude_paths: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArchitectureManifest {
    pub schema_version: u32,
    pub generator: String,
    pub service: ArchitectureService,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub capabilities: Vec<ArchitectureCapability>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub flows: Vec<ArchitectureCapabilityFlow>,
    pub packages: Vec<ArchitecturePackage>,
    pub dependencies: Vec<ArchitectureDependency>,
    pub external_dependencies: Vec<ArchitectureExternalDependency>,
    pub provided: ArchitectureInterfaces,
    pub consumed: ArchitectureInterfaces,
    #[serde(default)]
    pub contracts: ArchitectureContracts,
}

impl ArchitectureManifest {
    #[must_use]
    pub fn new(service_name: impl Into<String>) -> Self {
        let name = service_name.into();
        Self {
            schema_version: SCHEMA_VERSION,
            generator: "graphify-rs".to_string(),
            service: ArchitectureService {
                name,
                root: None,
                module_prefixes: Vec::new(),
            },
            capabilities: Vec::new(),
            flows: Vec::new(),
            packages: Vec::new(),
            dependencies: Vec::new(),
            external_dependencies: Vec::new(),
            provided: ArchitectureInterfaces::default(),
            consumed: ArchitectureInterfaces::default(),
            contracts: ArchitectureContracts::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArchitectureService {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub root: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub module_prefixes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArchitecturePackage {
    pub id: String,
    pub label: String,
    pub path: String,
    pub kind: String,
    pub source_files: Vec<String>,
    pub node_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub community: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArchitectureCapability {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub summary: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub packages: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub source_files: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub provided_apis: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub consumed_apis: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub events: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub operations: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub jobs: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub owns_data: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evidence: Vec<String>,
    pub node_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArchitectureCapabilityFlow {
    pub source: String,
    pub target: String,
    pub relation: String,
    pub weight: usize,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evidence: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArchitectureContracts {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub provided: Vec<ArchitectureContract>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub consumed: Vec<ArchitectureContract>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unresolved: Vec<ArchitectureContract>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArchitectureContract {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub source_kind: String,
    pub source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capability: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub operations: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evidence: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArchitectureDependency {
    pub source: String,
    pub target: String,
    pub relation: String,
    pub weight: usize,
    pub confidence: f64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sample_edges: Vec<ArchitectureSampleEdge>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArchitectureSampleEdge {
    pub source_node: String,
    pub target_node: String,
    pub source_file: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_location: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArchitectureExternalDependency {
    pub name: String,
    pub kind: String,
    pub weight: usize,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub imported_by: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArchitectureInterfaces {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub apis: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub events: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub protos: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArchitectureLandscape {
    pub schema_version: u32,
    pub generator: String,
    pub services: Vec<ArchitectureService>,
    pub dependencies: Vec<ArchitectureServiceDependency>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub contracts: Vec<ArchitectureLandscapeContract>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArchitectureLandscapeContract {
    pub service: String,
    pub id: String,
    pub name: String,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capability: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub operations: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArchitectureServiceDependency {
    pub source: String,
    pub target: String,
    pub relation: String,
    pub weight: usize,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evidence: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub contracts: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub operations: Vec<String>,
}

#[derive(Default)]
struct PackageAcc {
    source_files: BTreeSet<String>,
    node_count: usize,
    communities: HashMap<usize, usize>,
}

#[derive(Default)]
struct DependencyAcc {
    weight: usize,
    confidence_sum: f64,
    sample_edges: Vec<ArchitectureSampleEdge>,
}

#[derive(Default)]
struct ExternalAcc {
    kind: String,
    weight: usize,
    imported_by: BTreeSet<String>,
}

#[derive(Default)]
struct CapabilityFlowAcc {
    weight: usize,
    evidence: BTreeSet<String>,
}

#[derive(Default)]
struct CapabilityAcc {
    preferred_id: String,
    name: String,
    kind: String,
    packages: BTreeSet<String>,
    source_files: BTreeSet<String>,
    provided_apis: BTreeSet<String>,
    consumed_apis: BTreeSet<String>,
    events: BTreeSet<String>,
    operations: BTreeSet<String>,
    jobs: BTreeSet<String>,
    owns_data: BTreeSet<String>,
    evidence: BTreeSet<String>,
    node_count: usize,
}

#[derive(Debug, Clone)]
struct ProtoContract {
    capability_id: String,
    package: Option<String>,
    service: String,
    operations: Vec<String>,
    source_file: String,
}

#[must_use]
pub fn build_architecture_manifest(
    graph: &KnowledgeGraph,
    options: &ArchitectureOptions,
) -> ArchitectureManifest {
    let service_name = options.service_name.clone().unwrap_or_else(|| {
        options
            .root_path
            .as_deref()
            .and_then(Path::file_name)
            .and_then(|name| name.to_str())
            .filter(|name| !name.is_empty())
            .unwrap_or("service")
            .to_string()
    });
    let root = options.root_path.as_deref();
    let module_prefixes = detect_module_prefixes(root);

    let eligible = sorted_nodes(graph)
        .into_iter()
        .filter(|node| include_node_in_architecture(node, root, options))
        .map(|node| {
            let source_file = display_path(&node.source_file, root);
            let exact_owner = package_path_for_source(&source_file);
            (node, source_file, exact_owner)
        })
        .collect::<Vec<_>>();

    let exact_owners = eligible
        .iter()
        .map(|(_, _, owner)| owner.clone())
        .collect::<HashSet<_>>();
    let first_compaction = exact_owners.len() > options.max_packages;
    let compact_owners = eligible
        .iter()
        .map(|(_, _, owner)| {
            if first_compaction {
                compact_package_path(owner)
            } else {
                owner.clone()
            }
        })
        .collect::<HashSet<_>>();
    let second_compaction = compact_owners.len() > options.max_packages;

    let mut owner_by_node = HashMap::new();
    let mut packages: HashMap<String, PackageAcc> = HashMap::new();
    for (node, source_file, exact_owner) in eligible {
        let owner = if second_compaction {
            coarse_package_path(&exact_owner)
        } else if first_compaction {
            compact_package_path(&exact_owner)
        } else {
            exact_owner
        };
        owner_by_node.insert(node.id.clone(), owner.clone());
        let acc = packages.entry(owner).or_default();
        acc.source_files.insert(source_file);
        acc.node_count += 1;
        if let Some(community) = node.community {
            *acc.communities.entry(community).or_insert(0) += 1;
        }
    }

    let mut dependency_accs: HashMap<(String, String, String), DependencyAcc> = HashMap::new();
    for edge in sorted_edges(graph) {
        if !include_edge_in_architecture(edge) {
            continue;
        }
        let Some(source) = owner_by_node.get(&edge.source) else {
            continue;
        };
        let Some(target) = owner_by_node.get(&edge.target) else {
            continue;
        };
        if source == target {
            continue;
        }
        let relation = architecture_relation(&edge.relation);
        let acc = dependency_accs
            .entry((source.clone(), target.clone(), relation))
            .or_default();
        acc.weight += 1;
        acc.confidence_sum += safe_confidence(edge);
        if acc.sample_edges.len() < 5 {
            acc.sample_edges.push(ArchitectureSampleEdge {
                source_node: edge.source.clone(),
                target_node: edge.target.clone(),
                source_file: display_path(&edge.source_file, root),
                source_location: edge.source_location.clone(),
            });
        }
    }

    let mut dependencies = dependency_accs
        .into_iter()
        .map(|((source, target, relation), acc)| ArchitectureDependency {
            source,
            target,
            relation,
            weight: acc.weight,
            confidence: if acc.weight == 0 {
                0.0
            } else {
                (acc.confidence_sum / acc.weight as f64 * 1000.0).round() / 1000.0
            },
            sample_edges: acc.sample_edges,
        })
        .collect::<Vec<_>>();
    dependencies.sort_by(|a, b| {
        b.weight
            .cmp(&a.weight)
            .then_with(|| b.confidence.total_cmp(&a.confidence))
            .then_with(|| a.source.cmp(&b.source))
            .then_with(|| a.target.cmp(&b.target))
            .then_with(|| a.relation.cmp(&b.relation))
    });
    dependencies.truncate(options.max_dependencies);
    dependencies.sort_by(|a, b| {
        a.source
            .cmp(&b.source)
            .then_with(|| a.target.cmp(&b.target))
            .then_with(|| a.relation.cmp(&b.relation))
    });

    let mut package_nodes = packages
        .into_iter()
        .map(|(path, acc)| {
            let community = acc
                .communities
                .into_iter()
                .max_by_key(|(_, count)| *count)
                .map(|(community, _)| community);
            ArchitecturePackage {
                id: path.clone(),
                label: path.clone(),
                kind: package_kind(&path).to_string(),
                path,
                source_files: acc.source_files.into_iter().collect(),
                node_count: acc.node_count,
                community,
            }
        })
        .collect::<Vec<_>>();
    package_nodes.sort_by(|a, b| a.path.cmp(&b.path));

    let external_dependencies =
        collect_external_dependencies(graph, root, &owner_by_node, &module_prefixes);
    let provided = collect_provided_interfaces(&package_nodes, &module_prefixes);
    let consumed = collect_consumed_interfaces(&external_dependencies);
    let proto_contracts = collect_proto_contracts(root, options);
    let capabilities = build_capabilities(
        graph,
        root,
        &package_nodes,
        &external_dependencies,
        &proto_contracts,
        options,
    );
    let flows = build_capability_flows(&capabilities, &dependencies, options);
    let contracts = build_contracts(&proto_contracts, &external_dependencies, &capabilities);

    ArchitectureManifest {
        schema_version: SCHEMA_VERSION,
        generator: "graphify-rs".to_string(),
        service: ArchitectureService {
            name: service_name,
            // Keep manifests portable: source files are repo-relative already, and
            // absolute checkout paths must not leak into persisted architecture
            // manifests.
            root: None,
            module_prefixes,
        },
        capabilities,
        flows,
        packages: package_nodes,
        dependencies,
        external_dependencies,
        provided,
        consumed,
        contracts,
    }
}

pub fn export_architecture_manifest(
    graph: &KnowledgeGraph,
    output_dir: &Path,
    options: &ArchitectureOptions,
) -> anyhow::Result<PathBuf> {
    fs::create_dir_all(output_dir)?;
    let manifest = build_architecture_manifest(graph, options);
    let path = output_dir.join("architecture.json");
    fs::write(
        &path,
        format!("{}\n", serde_json::to_string_pretty(&manifest)?),
    )?;
    info!(path = %path.display(), "exported architecture manifest");
    Ok(path)
}

pub fn export_architecture_workspace(
    graph: &KnowledgeGraph,
    output_dir: &Path,
    options: &ArchitectureOptions,
) -> anyhow::Result<PathBuf> {
    fs::create_dir_all(output_dir)?;
    let manifest = build_architecture_manifest(graph, options);
    fs::write(
        output_dir.join("architecture.json"),
        format!("{}\n", serde_json::to_string_pretty(&manifest)?),
    )?;
    let workspace = output_dir.join("likec4");
    write_service_workspace(&manifest, &workspace)?;
    Ok(workspace)
}

fn build_capabilities(
    graph: &KnowledgeGraph,
    root: Option<&Path>,
    packages: &[ArchitecturePackage],
    external_dependencies: &[ArchitectureExternalDependency],
    proto_contracts: &[ProtoContract],
    options: &ArchitectureOptions,
) -> Vec<ArchitectureCapability> {
    let mut accs: HashMap<String, CapabilityAcc> = HashMap::new();

    for package in packages {
        let Some((capability_id, kind)) = capability_for_package_path(&package.path) else {
            continue;
        };
        let acc = capability_acc(&mut accs, &capability_id, kind);
        acc.packages.insert(package.path.clone());
        acc.source_files
            .extend(package.source_files.iter().cloned());
        acc.node_count += package.node_count;
        acc.evidence.insert(format!("package:{}", package.path));
        if matches!(kind, "data" | "repository") {
            acc.owns_data.insert(capability_label_seed(&capability_id));
        }
        if kind == "event" {
            acc.events.insert(capability_label_seed(&capability_id));
        }
        if kind == "job" {
            acc.jobs.insert(capability_label_seed(&capability_id));
        }
    }

    for contract in proto_contracts {
        let acc = capability_acc(&mut accs, &contract.capability_id, "contract");
        acc.source_files.insert(contract.source_file.clone());
        acc.evidence
            .insert(format!("proto:{}", contract.source_file));
        for operation in &contract.operations {
            acc.provided_apis
                .insert(format!("{}.{}", contract.service, operation));
        }
    }

    for node in sorted_nodes(graph) {
        let path = display_path(&node.source_file, root);
        if !matches_path_filters(&path, options) || is_transient_path(&path) {
            continue;
        }
        let Some((capability_id, kind)) = capability_for_source_path(&path) else {
            continue;
        };
        let acc = capability_acc(&mut accs, &capability_id, kind);
        acc.source_files.insert(path.clone());
        if matches!(node.node_type, NodeType::Function | NodeType::Method)
            && is_capability_operation(&node.label)
        {
            if kind == "job" || path.contains("/job") || path.contains("jobs") {
                acc.jobs.insert(clean_operation_name(&node.label));
            } else {
                acc.operations.insert(clean_operation_name(&node.label));
                acc.evidence.insert(format!(
                    "operation:{}@{}",
                    clean_operation_name(&node.label),
                    path
                ));
            }
        }
    }

    for dep in external_dependencies {
        if !is_informative_external_dependency(dep) {
            continue;
        }
        for package in &dep.imported_by {
            let Some((capability_id, _)) = capability_for_package_path(package) else {
                continue;
            };
            let acc = capability_acc(&mut accs, &capability_id, "integration");
            match dep.kind.as_str() {
                "event" => {
                    acc.events.insert(dep.name.clone());
                }
                "api" | "proto" | "module" => {
                    acc.consumed_apis.insert(dep.name.clone());
                }
                _ => {}
            }
            acc.evidence
                .insert(format!("external:{}:{}", dep.kind, dep.name));
        }
    }

    let mut capabilities = accs
        .into_values()
        .filter(capability_has_signal)
        .map(|acc| {
            let packages = acc.packages.into_iter().collect::<Vec<_>>();
            let source_files = acc.source_files.into_iter().take(25).collect::<Vec<_>>();
            let provided_apis = acc.provided_apis.into_iter().collect::<Vec<_>>();
            let consumed_apis = acc.consumed_apis.into_iter().take(25).collect::<Vec<_>>();
            let events = acc.events.into_iter().take(25).collect::<Vec<_>>();
            let operation_count = acc.operations.len();
            let operations = acc.operations.into_iter().take(25).collect::<Vec<_>>();
            let jobs = acc.jobs.into_iter().take(25).collect::<Vec<_>>();
            let owns_data = acc.owns_data.into_iter().collect::<Vec<_>>();
            let evidence = acc.evidence.into_iter().take(25).collect::<Vec<_>>();
            let summary = capability_summary(CapabilitySummaryInput {
                provided_apis: provided_apis.len(),
                consumed_apis: consumed_apis.len(),
                events: events.len(),
                operations: &operations,
                operation_count,
                jobs: jobs.len(),
                owns_data: owns_data.len(),
                packages: packages.len(),
            });
            ArchitectureCapability {
                id: acc.preferred_id,
                name: acc.name,
                kind: acc.kind,
                summary,
                packages,
                source_files,
                provided_apis,
                consumed_apis,
                events,
                operations,
                jobs,
                owns_data,
                evidence,
                node_count: acc.node_count,
            }
        })
        .collect::<Vec<_>>();

    capabilities.sort_by(|a, b| {
        capability_rank(&a.kind)
            .cmp(&capability_rank(&b.kind))
            .then_with(|| b.provided_apis.len().cmp(&a.provided_apis.len()))
            .then_with(|| b.node_count.cmp(&a.node_count))
            .then_with(|| a.name.cmp(&b.name))
    });
    capabilities
}

fn build_contracts(
    proto_contracts: &[ProtoContract],
    external_dependencies: &[ArchitectureExternalDependency],
    capabilities: &[ArchitectureCapability],
) -> ArchitectureContracts {
    let mut provided = proto_contracts
        .iter()
        .map(provided_contract_from_proto)
        .collect::<Vec<_>>();

    let provided_aliases = provided
        .iter()
        .flat_map(|contract| contract.aliases.iter().cloned())
        .collect::<HashSet<_>>();

    let mut consumed = external_dependencies
        .iter()
        .filter(|dep| matches!(dep.kind.as_str(), "api" | "proto"))
        .filter(|dep| is_informative_external_dependency(dep))
        .filter_map(|dep| consumed_contract_from_external(dep, capabilities))
        .collect::<Vec<_>>();

    provided = merge_contracts_by_id(provided);
    consumed = merge_contracts_by_id(consumed);

    let unresolved = consumed
        .iter()
        .filter(|contract| {
            contract
                .aliases
                .iter()
                .all(|alias| !provided_aliases.contains(alias))
        })
        .cloned()
        .collect::<Vec<_>>();

    ArchitectureContracts {
        provided,
        consumed,
        unresolved,
    }
}

fn merge_contracts_by_id(contracts: Vec<ArchitectureContract>) -> Vec<ArchitectureContract> {
    let mut merged = HashMap::<String, ArchitectureContract>::new();
    for mut contract in contracts {
        let entry = merged
            .entry(contract.id.clone())
            .or_insert_with(|| ArchitectureContract {
                id: contract.id.clone(),
                name: contract.name.clone(),
                kind: contract.kind.clone(),
                source_kind: contract.source_kind.clone(),
                source: contract.source.clone(),
                capability: contract.capability.clone(),
                operations: Vec::new(),
                aliases: Vec::new(),
                evidence: Vec::new(),
            });
        if entry.capability.is_none() {
            entry.capability = contract.capability.take();
        }
        if !entry.source.contains(&contract.source) {
            entry.source = format!("{};{}", entry.source, contract.source);
        }
        entry.operations.extend(contract.operations);
        entry.aliases.extend(contract.aliases);
        entry.evidence.extend(contract.evidence);
    }
    let mut contracts = merged.into_values().collect::<Vec<_>>();
    for contract in &mut contracts {
        contract.operations.sort();
        contract.operations.dedup();
        contract.aliases.sort();
        contract.aliases.dedup();
        contract.evidence.sort();
        contract.evidence.dedup();
    }
    contracts.sort_by(|a, b| a.id.cmp(&b.id));
    contracts
}

fn provided_contract_from_proto(contract: &ProtoContract) -> ArchitectureContract {
    let aliases = proto_contract_aliases(contract);
    ArchitectureContract {
        id: proto_contract_id(contract, &aliases),
        name: contract.service.clone(),
        kind: "proto".to_string(),
        source_kind: "proto".to_string(),
        source: contract.source_file.clone(),
        capability: Some(contract.capability_id.clone()),
        operations: contract
            .operations
            .iter()
            .map(|operation| format!("{}.{}", contract.service, operation))
            .collect(),
        aliases: aliases.into_iter().collect(),
        evidence: vec![format!("proto:{}", contract.source_file)],
    }
}

fn consumed_contract_from_external(
    dep: &ArchitectureExternalDependency,
    capabilities: &[ArchitectureCapability],
) -> Option<ArchitectureContract> {
    let aliases = external_contract_aliases(&dep.name, &dep.kind);
    if aliases.is_empty() {
        return None;
    }
    let source = dep.name.clone();
    let capability = capability_for_external(dep, capabilities);
    Some(ArchitectureContract {
        id: aliases
            .iter()
            .next()
            .cloned()
            .unwrap_or_else(|| contract_fallback_id(&dep.name, &dep.kind)),
        name: external_label(&dep.name),
        kind: dep.kind.clone(),
        source_kind: dep.kind.clone(),
        source,
        capability,
        operations: Vec::new(),
        aliases: aliases.into_iter().collect(),
        evidence: vec![format!("external:{}:{}", dep.kind, dep.name)],
    })
}

fn capability_for_external(
    dep: &ArchitectureExternalDependency,
    capabilities: &[ArchitectureCapability],
) -> Option<String> {
    for package in &dep.imported_by {
        for capability in capabilities {
            if capability.packages.iter().any(|path| path == package)
                || capability
                    .source_files
                    .iter()
                    .any(|path| path.starts_with(package))
            {
                return Some(capability.id.clone());
            }
        }
    }
    for package in &dep.imported_by {
        if let Some((capability, _)) = capability_for_package_path(package) {
            return Some(capability);
        }
    }
    None
}

fn proto_contract_id(contract: &ProtoContract, aliases: &BTreeSet<String>) -> String {
    let service = contract.service.to_ascii_lowercase();
    let service = service.strip_suffix("service").unwrap_or(&service);
    aliases
        .iter()
        .filter(|alias| alias.ends_with(&format!(":{service}")))
        .max_by_key(|alias| alias.len())
        .cloned()
        .or_else(|| aliases.iter().next().cloned())
        .unwrap_or_else(|| contract_fallback_id(&contract.source_file, "proto"))
}

fn proto_contract_aliases(contract: &ProtoContract) -> BTreeSet<String> {
    let mut aliases = BTreeSet::new();
    if let Some(package) = &contract.package {
        insert_contract_alias_prefixes(&mut aliases, "proto", &contract_segments(package), 2);
    }
    let source_segments = proto_path_contract_segments(&contract.source_file);
    insert_contract_alias_prefixes(&mut aliases, "proto", &source_segments, 2);

    let service = normalize_contract_segment(&contract.service);
    if !service.is_empty() {
        let service_base = service.strip_suffix("service").unwrap_or(&service);
        for base in aliases.clone() {
            aliases.insert(format!("{base}:{service}"));
            if service_base != service {
                aliases.insert(format!("{base}:{service_base}"));
            }
        }
    }
    aliases.insert(format!("capability:{}", contract.capability_id));
    aliases
}

fn external_contract_aliases(name: &str, kind: &str) -> BTreeSet<String> {
    let mut aliases = BTreeSet::new();
    let segments = contract_segments(name);
    if segments.is_empty() {
        return aliases;
    }

    let marker = match kind {
        "proto" => segments
            .iter()
            .position(|segment| matches!(segment.as_str(), "proto" | "protos" | "pb")),
        "api" => segments
            .iter()
            .position(|segment| matches!(segment.as_str(), "api" | "apis" | "client" | "clients")),
        _ => None,
    };
    let scoped = marker
        .and_then(|idx| {
            let rest = segments
                .iter()
                .skip(idx + 1)
                .filter(|segment| !is_generic_contract_segment(segment))
                .cloned()
                .collect::<Vec<_>>();
            (!rest.is_empty()).then_some(rest)
        })
        .unwrap_or_else(|| {
            segments
                .iter()
                .filter(|segment| !is_generic_contract_segment(segment))
                .cloned()
                .collect()
        });

    insert_contract_alias_prefixes(&mut aliases, kind, &scoped, 2);
    if scoped.len() == 1 && scoped[0].len() >= 4 {
        insert_contract_alias_prefixes(&mut aliases, kind, &scoped, 1);
    }
    if kind == "proto" {
        let dotted = scoped.join(".");
        if !dotted.is_empty() {
            aliases.insert(format!("proto:{dotted}"));
        }
    }
    aliases
}

fn proto_path_contract_segments(path: &str) -> Vec<String> {
    let mut segments = contract_segments(path);
    if let Some(idx) = segments
        .iter()
        .position(|segment| matches!(segment.as_str(), "proto" | "protos"))
    {
        segments = segments.into_iter().skip(idx + 1).collect();
    }
    if segments.last().is_some_and(|last| last == "proto") {
        segments.pop();
    }
    if segments.len() > 1 {
        let last = segments.last().cloned().unwrap_or_default();
        let before_last = segments
            .get(segments.len().saturating_sub(2))
            .cloned()
            .unwrap_or_default();
        if last == before_last || is_generic_contract_segment(&last) {
            segments.pop();
        }
    }
    segments
        .into_iter()
        .filter(|segment| !is_generic_contract_segment(segment))
        .collect()
}

fn insert_contract_alias_prefixes(
    aliases: &mut BTreeSet<String>,
    kind: &str,
    segments: &[String],
    min_len: usize,
) {
    if segments.len() < min_len {
        return;
    }
    aliases.insert(format!("{kind}:{}", segments.join(":")));
    for len in min_len..segments.len() {
        if segments
            .get(len.saturating_sub(1))
            .is_some_and(|segment| is_version_segment(segment))
        {
            aliases.insert(format!("{kind}:{}", segments[..len].join(":")));
        }
    }
}

fn is_version_segment(segment: &str) -> bool {
    segment.len() >= 2
        && segment.starts_with('v')
        && segment.chars().skip(1).all(|ch| ch.is_ascii_digit())
}

fn contract_segments(value: &str) -> Vec<String> {
    value
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .map(normalize_contract_segment)
        .filter(|segment| !segment.is_empty())
        .collect()
}

fn normalize_contract_segment(value: &str) -> String {
    value
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn contract_fallback_id(source: &str, kind: &str) -> String {
    let segments = contract_segments(source)
        .into_iter()
        .filter(|segment| !is_generic_contract_segment(segment))
        .collect::<Vec<_>>();
    if segments.is_empty() {
        kind.to_string()
    } else {
        format!("{kind}:{}", segments.join(":"))
    }
}

fn is_generic_contract_segment(segment: &str) -> bool {
    matches!(
        segment,
        "api"
            | "apis"
            | "client"
            | "clients"
            | "com"
            | "example"
            | "generated"
            | "github"
            | "gitlab"
            | "google"
            | "internal"
            | "local"
            | "net"
            | "org"
            | "pb"
            | "pkg"
            | "proto"
            | "protos"
            | "service"
            | "services"
            | "shared"
            | "src"
    )
}

fn build_capability_flows(
    capabilities: &[ArchitectureCapability],
    dependencies: &[ArchitectureDependency],
    options: &ArchitectureOptions,
) -> Vec<ArchitectureCapabilityFlow> {
    let mut capability_by_package = HashMap::<String, &ArchitectureCapability>::new();
    let mut kind_by_id = HashMap::<String, String>::new();
    for capability in capabilities {
        kind_by_id.insert(capability.id.clone(), capability.kind.clone());
        for package in &capability.packages {
            capability_by_package.insert(package.clone(), capability);
        }
    }

    let mut accs = HashMap::<(String, String, String), CapabilityFlowAcc>::new();
    for dep in dependencies {
        let Some(source_capability) = capability_by_package.get(&dep.source) else {
            continue;
        };
        let Some(target_capability) = capability_by_package.get(&dep.target) else {
            continue;
        };
        if source_capability.id == target_capability.id {
            continue;
        }
        let target_kind = kind_by_id
            .get(&target_capability.id)
            .map(String::as_str)
            .unwrap_or_default();
        let relation = capability_flow_relation(&dep.relation, target_kind);
        let acc = accs
            .entry((
                source_capability.id.clone(),
                target_capability.id.clone(),
                relation,
            ))
            .or_default();
        acc.weight += dep.weight.max(1);
        acc.evidence.insert(format!(
            "package_flow:{} -> {} ({})",
            dep.source, dep.target, dep.relation
        ));
        for sample in dep.sample_edges.iter().take(3) {
            acc.evidence.insert(format!(
                "edge:{} -> {} @ {}",
                sample.source_node, sample.target_node, sample.source_file
            ));
        }
    }

    let mut flows = accs
        .into_iter()
        .map(
            |((source, target, relation), acc)| ArchitectureCapabilityFlow {
                source,
                target,
                relation,
                weight: acc.weight,
                evidence: acc.evidence.into_iter().take(10).collect(),
            },
        )
        .collect::<Vec<_>>();
    flows.sort_by(|a, b| {
        b.weight
            .cmp(&a.weight)
            .then_with(|| a.source.cmp(&b.source))
            .then_with(|| a.target.cmp(&b.target))
            .then_with(|| a.relation.cmp(&b.relation))
    });
    flows.truncate(options.max_dependencies.min(80));
    flows.sort_by(|a, b| {
        a.source
            .cmp(&b.source)
            .then_with(|| a.target.cmp(&b.target))
            .then_with(|| a.relation.cmp(&b.relation))
    });
    flows
}

fn capability_flow_relation(package_relation: &str, target_kind: &str) -> String {
    match package_relation {
        "calls" => "calls",
        "reads_from" => "reads",
        "writes_to" | "alters" => "writes",
        "references" | "uses" if matches!(target_kind, "data" | "repository") => "reads",
        "references" | "uses" => "depends",
        relation => relation,
    }
    .to_string()
}

fn capability_acc<'a>(
    accs: &'a mut HashMap<String, CapabilityAcc>,
    raw_id: &str,
    kind: &str,
) -> &'a mut CapabilityAcc {
    let key = capability_key(raw_id);
    let preferred_id = capability_preferred_id(raw_id);
    let acc = accs.entry(key).or_insert_with(|| CapabilityAcc {
        preferred_id: preferred_id.clone(),
        name: capability_display_name(&preferred_id),
        kind: kind.to_string(),
        ..CapabilityAcc::default()
    });
    if capability_preferred_rank(&preferred_id) > capability_preferred_rank(&acc.preferred_id) {
        acc.preferred_id = preferred_id.clone();
        acc.name = capability_display_name(&preferred_id);
    }
    if capability_kind_rank(kind) < capability_kind_rank(&acc.kind) {
        acc.kind = kind.to_string();
    }
    acc
}

fn capability_for_source_path(path: &str) -> Option<(String, &'static str)> {
    capability_for_package_path(&package_path_for_source(path))
}

fn capability_for_package_path(path: &str) -> Option<(String, &'static str)> {
    let normalized = normalize_path(path);
    let segments = normalized
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>();
    if segments.is_empty() {
        return None;
    }
    if segments.iter().any(|segment| {
        matches!(
            *segment,
            "mock" | "mocks" | "test" | "tests" | "testdata" | "migrations"
        )
    }) {
        return None;
    }

    match segments.as_slice() {
        ["internal", "domain", "broadcaster", name, ..] => Some(((*name).to_string(), "event")),
        ["internal", "domain", "receiver", name, ..] => Some(((*name).to_string(), "event")),
        ["internal", "domain", name, ..] => Some(((*name).to_string(), "business")),
        ["internal", "service", name, ..] => Some(((*name).to_string(), "business")),
        ["internal", "broadcaster", name, ..] => Some(((*name).to_string(), "event")),
        ["internal", "broadcaster"] => Some(("broadcaster".to_string(), "event")),
        ["internal", "server", rest @ ..] => capability_after_marker(rest, "handler", "api"),
        ["internal", "api", name, ..] => Some(((*name).to_string(), "api")),
        ["internal", "job", name, ..] => Some(((*name).to_string(), "job")),
        ["internal", "jobs", name, ..] => Some(((*name).to_string(), "job")),
        ["internal", "syncs", name, ..] => Some(((*name).to_string(), "event")),
        ["internal", "kafka", name, ..] => Some(((*name).to_string(), "event")),
        ["internal", "kafka"] => Some(("kafka".to_string(), "event")),
        ["internal", "client", name, ..] => Some(((*name).to_string(), "integration")),
        ["internal", "clients", name, ..] => Some(((*name).to_string(), "integration")),
        ["internal", "infrastructure", "external", name, ..] => {
            Some(((*name).to_string(), "integration"))
        }
        ["internal", "infrastructure", name, ..] => Some(((*name).to_string(), "integration")),
        ["internal", "repository", name, ..] => Some(((*name).to_string(), "repository")),
        ["internal", "pg", name, ..] => Some(((*name).to_string(), "repository")),
        ["internal", name, ..] if is_feature_root(name) => {
            Some((normalize_capability_token(name), "business"))
        }
        ["pkg", "repository", name, ..] => Some(((*name).to_string(), "data")),
        ["database", name, ..] => Some(((*name).to_string(), "data")),
        ["crates", name, ..] => Some(((*name).to_string(), "business")),
        ["src", ..] => Some(("cli".to_string(), "business")),
        ["proto", ..] => proto_capability_from_segments(&segments).map(|id| (id, "contract")),
        _ => None,
    }
}

fn capability_after_marker(
    segments: &[&str],
    marker: &str,
    kind: &'static str,
) -> Option<(String, &'static str)> {
    let marker_idx = segments.iter().position(|segment| *segment == marker)?;
    let name = segments.get(marker_idx + 1)?;
    Some(((*name).to_string(), kind))
}

fn is_feature_root(name: &str) -> bool {
    !matches!(
        name,
        "" | "api"
            | "app"
            | "application"
            | "bootstrap"
            | "client"
            | "clients"
            | "cmd"
            | "config"
            | "configs"
            | "database"
            | "db"
            | "di"
            | "domain"
            | "infra"
            | "infrastructure"
            | "job"
            | "jobs"
            | "lib"
            | "libs"
            | "library"
            | "model"
            | "models"
            | "mock"
            | "mocks"
            | "pg"
            | "pkg"
            | "proto"
            | "protos"
            | "repository"
            | "server"
            | "servers"
            | "service"
            | "services"
            | "shared"
            | "storage"
            | "sync"
            | "syncs"
            | "test"
            | "tests"
            | "type"
            | "types"
            | "util"
            | "utils"
            | "wire"
    )
}

fn proto_capability_from_segments(segments: &[&str]) -> Option<String> {
    let file = segments.last().copied().unwrap_or_default();
    let stem = file.trim_end_matches(".proto");
    if !is_generic_capability_name(stem) {
        return Some(normalize_capability_token(stem));
    }
    for marker in ["service", "services"] {
        if let Some(marker_idx) = segments.iter().position(|segment| *segment == marker)
            && let Some(name) = segments
                .iter()
                .skip(marker_idx + 1)
                .map(|segment| segment.trim_end_matches(".proto"))
                .find(|segment| !is_generic_capability_name(segment))
        {
            return Some(normalize_capability_token(name));
        }
    }
    segments
        .iter()
        .rev()
        .map(|segment| segment.trim_end_matches(".proto"))
        .find(|segment| !is_generic_capability_name(segment) && *segment != "proto")
        .map(normalize_capability_token)
}

fn collect_proto_contracts(
    root: Option<&Path>,
    options: &ArchitectureOptions,
) -> Vec<ProtoContract> {
    let Some(root) = root else {
        return Vec::new();
    };
    let mut files = Vec::new();
    collect_proto_files(root, root, options, &mut files);
    files.sort();
    files
        .into_iter()
        .flat_map(|relative| parse_proto_contracts(root, &relative))
        .collect()
}

fn collect_proto_files(
    dir: &Path,
    root: &Path,
    options: &ArchitectureOptions,
    files: &mut Vec<String>,
) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if should_skip_architecture_dir(&path) {
                continue;
            }
            collect_proto_files(&path, root, options, files);
            continue;
        }
        if path.extension().and_then(|ext| ext.to_str()) != Some("proto") {
            continue;
        }
        let relative = display_path(&path.to_string_lossy(), Some(root));
        if normalize_path(&relative)
            .split('/')
            .any(|part| part == "proto")
            && matches_path_filters(&relative, options)
            && !is_transient_path(&relative)
        {
            files.push(relative);
        }
    }
}

fn should_skip_architecture_dir(path: &Path) -> bool {
    matches!(
        path.file_name().and_then(|name| name.to_str()),
        Some(".git" | ".graphify" | ".likec4" | "node_modules" | "target" | "dist" | "build")
    )
}

fn parse_proto_contracts(root: &Path, relative: &str) -> Vec<ProtoContract> {
    let Ok(content) = fs::read_to_string(root.join(relative)) else {
        return Vec::new();
    };
    let mut current_service = None::<String>;
    let mut package = None::<String>;
    let mut contracts = Vec::<ProtoContract>::new();
    let mut operations = Vec::<String>::new();

    for raw_line in content.lines() {
        let line = raw_line.trim();
        if let Some(rest) = line.strip_prefix("package ") {
            let parsed = rest
                .split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_' || ch == '.'))
                .next()
                .unwrap_or_default()
                .trim_end_matches(';')
                .to_string();
            if !parsed.is_empty() {
                package = Some(parsed);
            }
            continue;
        }
        if let Some(rest) = line.strip_prefix("service ") {
            if let Some(service) = current_service.take() {
                contracts.push(ProtoContract {
                    capability_id: proto_capability_from_path(relative, &service),
                    package: package.clone(),
                    service,
                    operations: std::mem::take(&mut operations),
                    source_file: relative.to_string(),
                });
            }
            let service = rest
                .split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_'))
                .next()
                .unwrap_or_default()
                .to_string();
            if !service.is_empty() {
                current_service = Some(service);
            }
            continue;
        }
        if current_service.is_some()
            && let Some(rest) = line.strip_prefix("rpc ")
        {
            let operation = rest
                .split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_'))
                .next()
                .unwrap_or_default();
            if !operation.is_empty() {
                operations.push(operation.to_string());
            }
        }
    }

    if let Some(service) = current_service {
        contracts.push(ProtoContract {
            capability_id: proto_capability_from_path(relative, &service),
            package,
            service,
            operations,
            source_file: relative.to_string(),
        });
    }

    contracts
}

fn proto_capability_from_path(path: &str, service: &str) -> String {
    let segments = normalize_path(path)
        .split('/')
        .filter(|segment| !segment.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
    let id = if let Some(id) =
        proto_capability_from_segments(&segments.iter().map(String::as_str).collect::<Vec<_>>())
    {
        id
    } else {
        service.trim_end_matches("Service").to_string()
    };
    capability_preferred_id(&id)
}

fn capability_has_signal(acc: &CapabilityAcc) -> bool {
    !acc.provided_apis.is_empty()
        || !acc.events.is_empty()
        || !acc.operations.is_empty()
        || !acc.jobs.is_empty()
        || !acc.owns_data.is_empty()
        || acc.kind == "business"
        || acc.kind == "api"
}

struct CapabilitySummaryInput<'a> {
    provided_apis: usize,
    consumed_apis: usize,
    events: usize,
    operations: &'a [String],
    operation_count: usize,
    jobs: usize,
    owns_data: usize,
    packages: usize,
}

fn capability_summary(input: CapabilitySummaryInput<'_>) -> String {
    let mut parts = Vec::new();
    if input.provided_apis > 0 {
        parts.push(format!("Exposes {} RPCs", input.provided_apis));
    }
    if input.consumed_apis > 0 {
        parts.push(format!(
            "Consumes {} external contracts",
            input.consumed_apis
        ));
    }
    if !input.operations.is_empty() {
        let mut sample = input.operations.iter().take(4).cloned().collect::<Vec<_>>();
        if input.operation_count > sample.len() {
            sample.push(format!("+{} more", input.operation_count - sample.len()));
        }
        parts.push(format!("Key operations: {}", sample.join(", ")));
    }
    if input.events > 0 {
        parts.push(format!("Handles {} event streams", input.events));
    }
    if input.jobs > 0 {
        parts.push(format!("Runs {} jobs", input.jobs));
    }
    if input.owns_data > 0 {
        parts.push(format!("Owns {} data areas", input.owns_data));
    }
    if input.packages > 0 {
        parts.push(format!("Implemented across {} packages", input.packages));
    }
    if parts.is_empty() {
        "Categorized from graph structure".to_string()
    } else {
        parts.join("; ")
    }
}

fn capability_key(raw: &str) -> String {
    let preferred = capability_preferred_id(raw);
    preferred
        .trim_end_matches('s')
        .trim_end_matches("_v2")
        .trim_end_matches("_service")
        .to_string()
}

fn capability_preferred_id(raw: &str) -> String {
    let normalized = normalize_capability_token(raw);
    normalized
        .strip_suffix("_service")
        .unwrap_or(&normalized)
        .to_string()
}

fn capability_preferred_rank(id: &str) -> usize {
    usize::from(id.ends_with('s')) + usize::from(id.contains('_'))
}

fn capability_kind_rank(kind: &str) -> usize {
    match kind {
        "contract" => 0,
        "business" => 1,
        "api" => 2,
        "event" => 3,
        "job" => 4,
        "repository" | "data" => 5,
        "integration" => 6,
        _ => 10,
    }
}

fn capability_rank(kind: &str) -> usize {
    capability_kind_rank(kind)
}

fn capability_display_name(id: &str) -> String {
    id.split('_')
        .filter(|part| !part.is_empty())
        .map(display_name_part)
        .collect::<Vec<_>>()
        .join(" ")
}

fn normalize_capability_token(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len().max(4));
    let mut prev_was_separator = true;
    let chars = raw.trim().chars().collect::<Vec<_>>();
    for (idx, ch) in chars.iter().copied().enumerate() {
        if ch.is_ascii_alphanumeric() {
            let prev = idx.checked_sub(1).and_then(|prev| chars.get(prev)).copied();
            let next = chars.get(idx + 1).copied();
            if ch.is_ascii_uppercase()
                && !prev_was_separator
                && (prev.is_some_and(|prev| prev.is_ascii_lowercase() || prev.is_ascii_digit())
                    || next.is_some_and(|next| next.is_ascii_lowercase()))
            {
                out.push('_');
            }
            out.push(ch.to_ascii_lowercase());
            prev_was_separator = false;
        } else if !prev_was_separator {
            out.push('_');
            prev_was_separator = true;
        }
    }
    while out.ends_with('_') {
        out.pop();
    }
    if out.is_empty() {
        return "capability".to_string();
    }
    if let Some(prefix) = out.strip_suffix("api").map(str::to_string)
        && !prefix.is_empty()
        && !prefix.ends_with('_')
    {
        out = format!("{prefix}_api");
    }
    sanitize_identifier(&out)
}

fn display_name_part(part: &str) -> String {
    match part {
        "api" => "API".to_string(),
        "id" => "ID".to_string(),
        "http" => "HTTP".to_string(),
        "rpc" => "RPC".to_string(),
        "url" => "URL".to_string(),
        _ => {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => first.to_ascii_uppercase().to_string() + chars.as_str(),
                None => String::new(),
            }
        }
    }
}

fn capability_label_seed(id: &str) -> String {
    id.replace('_', " ")
}

fn external_label(name: &str) -> String {
    let trimmed = name.trim_matches('/');
    trimmed
        .rsplit('/')
        .find(|part| !part.is_empty())
        .unwrap_or(trimmed)
        .trim_end_matches(".proto")
        .trim_end_matches("connect")
        .trim_matches('.')
        .replace(['_', '-'], " ")
}

fn is_generic_capability_name(name: &str) -> bool {
    matches!(
        name,
        "" | "api"
            | "common"
            | "connect"
            | "model"
            | "models"
            | "service"
            | "services"
            | "v1"
            | "v2"
    ) || name.ends_with(".pb")
}

fn is_capability_operation(label: &str) -> bool {
    let name = clean_operation_name(label);
    name.chars().next().is_some_and(char::is_uppercase)
        && !matches!(
            name.as_str(),
            "String"
                | "Enum"
                | "Descriptor"
                | "Type"
                | "Number"
                | "Reset"
                | "ProtoReflect"
                | "AddError"
                | "AddErrorf"
                | "Close"
                | "Debug"
                | "Debugf"
                | "Debugln"
                | "Error"
                | "Errorf"
                | "Errorln"
                | "Fatal"
                | "Fatalf"
                | "Fatalln"
                | "Info"
                | "Infof"
                | "Infoln"
                | "Panic"
                | "Panicf"
                | "Panicln"
                | "Ping"
                | "Print"
                | "Printf"
                | "Println"
                | "RegisterMetrics"
                | "Shutdown"
                | "Start"
                | "Stop"
                | "Warn"
                | "Warnf"
        )
        && !name.starts_with("New")
        && !name.starts_with("Option")
        && !name.starts_with("Test")
        && !name.starts_with("Benchmark")
        && !name.starts_with("Example")
        && !name.starts_with("Fuzz")
        && !name.contains("TestData")
        && !name.starts_with("With")
}

fn clean_operation_name(label: &str) -> String {
    label
        .trim()
        .trim_end_matches("()")
        .split('(')
        .next()
        .unwrap_or(label)
        .to_string()
}

fn is_informative_external_dependency(dep: &ArchitectureExternalDependency) -> bool {
    let name = dep.name.to_ascii_lowercase();
    if is_runtime_import(&name)
        || name.starts_with("github.com/stretchr/")
        || name.starts_with("github.com/prometheus/")
        || name.starts_with("go.uber.org/mock")
        || name.starts_with("google.golang.org/protobuf")
        || name.starts_with("google.golang.org/genproto")
        || name.starts_with("buf.build/gen/go/bufbuild/")
        || name.starts_with("buf.build/gen/go/googleapis/")
        || name.starts_with("connectrpc.com/")
        || name.contains("/mock")
        || name.contains("/test")
    {
        return false;
    }
    matches!(dep.kind.as_str(), "api" | "event" | "proto")
        || name.contains("service")
        || name.contains("client")
        || name.contains("broker")
        || name.contains("kafka")
        || name.contains("analytics")
}

#[must_use]
pub fn compose_architecture_manifests(manifests: &[ArchitectureManifest]) -> ArchitectureLandscape {
    let mut services = manifests
        .iter()
        .map(|manifest| manifest.service.clone())
        .collect::<Vec<_>>();
    services.sort_by(|a, b| a.name.cmp(&b.name));

    let mut deps: HashMap<(String, String), ArchitectureServiceDependency> = HashMap::new();
    for source in manifests {
        for target in manifests {
            if source.service.name == target.service.name {
                continue;
            }
            let mut weight = 0usize;
            let mut evidence = BTreeSet::new();
            let mut contracts = BTreeSet::new();
            let mut operations = BTreeSet::new();
            let target_tokens = service_contract_tokens(target);
            let contract_registry_available =
                !source.contracts.consumed.is_empty() || !target.contracts.provided.is_empty();

            for consumed in &source.contracts.consumed {
                for provided in &target.contracts.provided {
                    if let Some(alias) = contract_alias_overlap(consumed, provided) {
                        weight += provided.operations.len().max(1);
                        evidence.insert(format!(
                            "contract_id:{alias} consumed:{} provided:{}",
                            consumed.id, provided.id
                        ));
                        contracts.insert(provided.id.clone());
                        operations.extend(provided.operations.iter().cloned());
                    }
                }
            }

            for external in &source.external_dependencies {
                for prefix in &target.service.module_prefixes {
                    if !prefix.is_empty() && external.name.starts_with(prefix) {
                        weight += external.weight.max(1);
                        evidence.insert(format!("module_prefix:{prefix} via {}", external.name));
                    }
                }
                if !contract_registry_available && matches!(external.kind.as_str(), "api" | "proto")
                {
                    let external_tokens = meaningful_tokens(&external.name);
                    if let Some(token) = first_token_overlap(&external_tokens, &target_tokens) {
                        weight += external.weight.max(1);
                        evidence.insert(format!("contract_token:{token} via {}", external.name));
                    }
                }
            }

            for consumed in interface_names(&source.consumed) {
                for provided in interface_names(&target.provided) {
                    if interfaces_match(&consumed, &provided, &target.service.name) {
                        weight += 1;
                        evidence.insert(format!("interface:{consumed} -> {provided}"));
                    }
                }
            }

            let source_tokens = meaningful_tokens(&source.service.name);
            for capability in &target.capabilities {
                if !matches!(capability.kind.as_str(), "contract" | "api") {
                    continue;
                }
                let capability_tokens = capability_contract_tokens(capability);
                if let Some(token) = first_token_overlap(&source_tokens, &capability_tokens) {
                    weight += capability.provided_apis.len().max(1);
                    evidence.insert(format!(
                        "target_contract_for_source:{token} via capability:{}",
                        capability.id
                    ));
                    for contract in &target.contracts.provided {
                        if contract.capability.as_deref() == Some(capability.id.as_str()) {
                            contracts.insert(contract.id.clone());
                            operations.extend(contract.operations.iter().cloned());
                        }
                    }
                    operations.extend(capability.provided_apis.iter().cloned());
                }
            }

            let target_token = target.service.name.to_ascii_lowercase();
            for package in &source.packages {
                if package_mentions_service(package, &target_token)
                    && !matches!(package.kind.as_str(), "api" | "proto")
                {
                    weight += package.node_count.max(1);
                    evidence.insert(format!("source_package_mentions_target:{}", package.path));
                }
            }

            if weight == 0 {
                continue;
            }
            deps.insert(
                (source.service.name.clone(), target.service.name.clone()),
                ArchitectureServiceDependency {
                    source: source.service.name.clone(),
                    target: target.service.name.clone(),
                    relation: if service_is_contract_catalog(target) {
                        "depends".to_string()
                    } else {
                        "calls".to_string()
                    },
                    weight,
                    evidence: evidence.into_iter().take(10).collect(),
                    contracts: contracts.into_iter().take(10).collect(),
                    operations: operations.into_iter().take(20).collect(),
                },
            );
        }
    }

    let mut dependencies = deps.into_values().collect::<Vec<_>>();
    dependencies.sort_by(|a, b| {
        a.source
            .cmp(&b.source)
            .then_with(|| a.target.cmp(&b.target))
    });

    ArchitectureLandscape {
        schema_version: SCHEMA_VERSION,
        generator: "graphify-rs".to_string(),
        services,
        dependencies,
        contracts: landscape_contracts(manifests),
    }
}

fn landscape_contracts(manifests: &[ArchitectureManifest]) -> Vec<ArchitectureLandscapeContract> {
    let mut contracts = Vec::new();
    for manifest in manifests {
        for contract in &manifest.contracts.provided {
            contracts.push(ArchitectureLandscapeContract {
                service: manifest.service.name.clone(),
                id: contract.id.clone(),
                name: contract.name.clone(),
                kind: contract.kind.clone(),
                capability: contract.capability.clone(),
                operations: contract.operations.clone(),
                aliases: contract.aliases.clone(),
            });
        }
    }
    contracts.sort_by(|a, b| {
        a.service
            .cmp(&b.service)
            .then_with(|| a.id.cmp(&b.id))
            .then_with(|| a.name.cmp(&b.name))
    });
    contracts
}

pub fn export_landscape_workspace(
    manifests: &[ArchitectureManifest],
    output_dir: &Path,
) -> anyhow::Result<PathBuf> {
    fs::create_dir_all(output_dir)?;
    let landscape = compose_architecture_manifests(manifests);
    fs::write(
        output_dir.join("landscape.json"),
        format!("{}\n", serde_json::to_string_pretty(&landscape)?),
    )?;
    let workspace = output_dir.join("likec4");
    write_landscape_workspace(&landscape, &workspace)?;
    Ok(workspace)
}

fn write_service_workspace(
    manifest: &ArchitectureManifest,
    workspace: &Path,
) -> anyhow::Result<()> {
    fs::create_dir_all(workspace)?;
    fs::write(
        workspace.join("likec4.config.json"),
        config_json(&service_root_id(&manifest.service.name))?,
    )?;
    fs::write(
        workspace.join("specification.c4"),
        service_specification(manifest),
    )?;
    fs::write(workspace.join("model.c4"), service_model(manifest))?;
    fs::write(workspace.join("views.c4"), service_views(manifest))?;
    fs::write(workspace.join("README.md"), service_readme())?;
    info!(path = %workspace.display(), "exported architecture LikeC4 workspace");
    Ok(())
}

fn write_landscape_workspace(
    landscape: &ArchitectureLandscape,
    workspace: &Path,
) -> anyhow::Result<()> {
    fs::create_dir_all(workspace)?;
    fs::write(
        workspace.join("likec4.config.json"),
        config_json("landscape")?,
    )?;
    fs::write(
        workspace.join("specification.c4"),
        landscape_specification(landscape),
    )?;
    fs::write(workspace.join("model.c4"), landscape_model(landscape))?;
    fs::write(workspace.join("views.c4"), landscape_views())?;
    fs::write(workspace.join("README.md"), landscape_readme())?;
    Ok(())
}

fn service_specification(manifest: &ArchitectureManifest) -> String {
    if !manifest.capabilities.is_empty() {
        return [
            "specification {",
            "  element system",
            "  element container",
            "  element component",
            "  element api",
            "  element externalApi",
            "  element operation",
            "",
            "  relationship uses",
            "  relationship implements",
            "  relationship consumes",
            "  relationship calls",
            "  relationship depends",
            "  relationship reads",
            "  relationship writes",
            "  relationship provides",
            "  relationship stores",
            "  relationship schedules",
            "  relationship publishes",
            "  relationship subscribes",
            "}",
            "",
        ]
        .join("\n");
    }

    let mut relations = BTreeSet::new();
    for dep in &manifest.dependencies {
        relations.insert(dep.relation.as_str());
    }
    if relations.is_empty() {
        relations.insert("uses");
    }

    let mut out = String::from("specification {\n");
    out.push_str("  element system\n");
    out.push_str("  element container\n\n");
    for relation in relations {
        writeln!(out, "  relationship {relation}").expect("write to string");
    }
    out.push_str("}\n");
    out
}

fn landscape_specification(landscape: &ArchitectureLandscape) -> String {
    let mut relations = BTreeSet::new();
    for dep in &landscape.dependencies {
        relations.insert(dep.relation.as_str());
    }
    if relations.is_empty() {
        relations.insert("calls");
    }
    let mut out = String::from("specification {\n");
    out.push_str("  element system\n\n");
    out.push_str("  element api\n");
    out.push_str("  element operation\n\n");
    for relation in relations {
        writeln!(out, "  relationship {relation}").expect("write to string");
    }
    out.push_str("}\n");
    out
}

fn service_model(manifest: &ArchitectureManifest) -> String {
    if !manifest.capabilities.is_empty() {
        return capability_service_model(manifest);
    }

    let mut ids = UniqueIds::default();
    for package in &manifest.packages {
        ids.insert(&package.id, &format!("pkg_{}", package.path));
    }

    let root_id = service_root_id(&manifest.service.name);
    let mut out = String::from("model {\n");
    writeln!(
        out,
        "  {root_id} = system \"{}\" {{",
        escape_likec4_string(&manifest.service.name)
    )
    .expect("write to string");
    out.push_str("    metadata {\n");
    out.push_str("      generator \"graphify-rs\"\n");
    out.push_str("      source_model \"architecture_manifest\"\n");
    for prefix in &manifest.service.module_prefixes {
        write_metadata_string(&mut out, 6, "module_prefix", prefix);
    }
    out.push_str("    }\n");

    for package in &manifest.packages {
        let Some(id) = ids.get(&package.id) else {
            continue;
        };
        writeln!(
            out,
            "    {id} = container \"{}\" {{",
            escape_likec4_string(&package.label)
        )
        .expect("write to string");
        out.push_str("      metadata {\n");
        write_metadata_string(&mut out, 8, "graphify_package_path", &package.path);
        write_metadata_string(&mut out, 8, "graphify_package_kind", &package.kind);
        write_metadata_string(
            &mut out,
            8,
            "source_files",
            &package.source_files.len().to_string(),
        );
        write_metadata_string(&mut out, 8, "node_count", &package.node_count.to_string());
        if let Some(community) = package.community {
            write_metadata_string(&mut out, 8, "community", &community.to_string());
        }
        out.push_str("      }\n");
        out.push_str("    }\n");
    }

    if !manifest.dependencies.is_empty() && !manifest.packages.is_empty() {
        out.push('\n');
    }
    for dep in &manifest.dependencies {
        let Some(source) = ids.get(&dep.source) else {
            continue;
        };
        let Some(target) = ids.get(&dep.target) else {
            continue;
        };
        writeln!(
            out,
            "    {source} -[{}]-> {target} \"{} ({})\" {{",
            dep.relation,
            escape_likec4_string(&dep.relation),
            dep.weight
        )
        .expect("write to string");
        out.push_str("      metadata {\n");
        write_metadata_string(&mut out, 8, "weight", &dep.weight.to_string());
        write_metadata_string(&mut out, 8, "confidence", &format!("{:.3}", dep.confidence));
        for sample in &dep.sample_edges {
            write_metadata_string(
                &mut out,
                8,
                "sample_edge",
                &format!(
                    "{} -> {} @ {}",
                    sample.source_node, sample.target_node, sample.source_file
                ),
            );
        }
        out.push_str("      }\n");
        out.push_str("    }\n");
    }

    out.push_str("  }\n");
    out.push_str("}\n");
    out
}

fn capability_service_model(manifest: &ArchitectureManifest) -> String {
    let root_id = service_root_id(&manifest.service.name);
    let mut capability_ids = UniqueIds::default();
    for capability in &manifest.capabilities {
        capability_ids.insert(&capability.id, &format!("cap_{}", capability.id));
    }

    let mut provided_contract_ids = UniqueIds::default();
    for contract in &manifest.contracts.provided {
        provided_contract_ids.insert(&contract.id, &format!("api_{}", contract.id));
    }

    let mut external_contract_ids = UniqueIds::default();
    for contract in &manifest.contracts.unresolved {
        external_contract_ids.insert(&contract.id, &format!("ext_{}", contract.id));
    }

    let mut package_ids = UniqueIds::default();
    for package in &manifest.packages {
        package_ids.insert(&package.path, &format!("mod_{}", package.path));
    }

    let package_by_path = manifest
        .packages
        .iter()
        .map(|package| (package.path.as_str(), package))
        .collect::<HashMap<_, _>>();
    let package_owner = package_owner_map(manifest);
    let contract_implementations = contract_implementation_packages(manifest);
    let duplicate_contract_names = duplicate_contract_names(&manifest.contracts.provided);

    let mut out = String::from("model {\n");
    for contract in &manifest.contracts.unresolved {
        let Some(id) = external_contract_ids.get(&contract.id) else {
            continue;
        };
        writeln!(
            out,
            "  {id} = externalApi \"{}\" {{",
            escape_likec4_string(&contract.name)
        )
        .expect("write to string");
        write_contract_metadata(&mut out, contract, 4);
        out.push_str("  }\n");
    }
    if !manifest.contracts.unresolved.is_empty() {
        out.push('\n');
    }

    writeln!(
        out,
        "  {root_id} = system \"{}\" {{",
        escape_likec4_string(&manifest.service.name)
    )
    .expect("write to string");
    out.push_str("    metadata {\n");
    out.push_str("      generator \"graphify-rs\"\n");
    out.push_str("      source_model \"architecture_capabilities\"\n");
    for prefix in &manifest.service.module_prefixes {
        write_metadata_string(&mut out, 6, "module_prefix", prefix);
    }
    out.push_str("    }\n");

    for capability in &manifest.capabilities {
        let Some(id) = capability_ids.get(&capability.id) else {
            continue;
        };
        writeln!(
            out,
            "    {id} = container \"{}\" {{",
            escape_likec4_string(&capability.name)
        )
        .expect("write to string");
        write_metadata_string(&mut out, 6, "summary", &capability.summary);
        out.push_str("      metadata {\n");
        write_metadata_string(&mut out, 8, "graphify_capability_id", &capability.id);
        write_metadata_string(&mut out, 8, "graphify_capability_kind", &capability.kind);
        write_metadata_string(&mut out, 8, "graphify_summary", &capability.summary);
        write_metadata_string(
            &mut out,
            8,
            "graphify_packages",
            &capability.packages.len().to_string(),
        );
        write_metadata_string(
            &mut out,
            8,
            "node_count",
            &capability.node_count.to_string(),
        );
        for api in capability.provided_apis.iter().take(20) {
            write_metadata_string(&mut out, 8, "provides_api", api);
        }
        for api in capability.consumed_apis.iter().take(20) {
            write_metadata_string(&mut out, 8, "consumes_api", api);
        }
        for data in capability.owns_data.iter().take(20) {
            write_metadata_string(&mut out, 8, "owns_data", data);
        }
        for operation in capability.operations.iter().take(20) {
            write_metadata_string(&mut out, 8, "operation", operation);
        }
        for job in capability.jobs.iter().take(20) {
            write_metadata_string(&mut out, 8, "job", job);
        }
        for evidence in capability.evidence.iter().take(20) {
            write_metadata_string(&mut out, 8, "evidence", evidence);
        }
        out.push_str("      }\n");

        let operations_by_package = capability_operations_by_package(capability);
        for package_path in &capability.packages {
            let Some(package) = package_by_path.get(package_path.as_str()) else {
                continue;
            };
            let operations = operations_by_package
                .get(package_path)
                .cloned()
                .unwrap_or_default();
            if let Some(package_id) = package_ids.get(&package.path) {
                write_package_component(&mut out, package_id, package, &operations, 6);
            }
        }

        let nested_operation_count = operations_by_package.values().map(Vec::len).sum::<usize>();
        if nested_operation_count == 0 {
            write_operation_elements(&mut out, &capability.operations, 6);
        }

        for contract in manifest
            .contracts
            .provided
            .iter()
            .filter(|contract| contract.capability.as_deref() == Some(capability.id.as_str()))
        {
            let Some(contract_id) = provided_contract_ids.get(&contract.id) else {
                continue;
            };
            writeln!(
                out,
                "      {contract_id} = api \"{}\" {{",
                escape_likec4_string(&contract_display_name(contract, &duplicate_contract_names))
            )
            .expect("write to string");
            write_contract_metadata(&mut out, contract, 8);
            write_operation_elements(&mut out, &contract.operations, 8);
            out.push_str("      }\n");
        }

        out.push_str("    }\n");
    }

    for package in manifest
        .packages
        .iter()
        .filter(|package| !package_owner.contains_key(&package.path))
    {
        if let Some(package_id) = package_ids.get(&package.path) {
            write_package_component(&mut out, package_id, package, &[], 4);
        }
    }

    for contract in manifest.contracts.provided.iter().filter(|contract| {
        contract_ref(contract, &capability_ids, &provided_contract_ids).is_none()
    }) {
        let Some(id) = provided_contract_ids.get(&contract.id) else {
            continue;
        };
        writeln!(
            out,
            "    {id} = api \"{}\" {{",
            escape_likec4_string(&contract_display_name(contract, &duplicate_contract_names))
        )
        .expect("write to string");
        write_contract_metadata(&mut out, contract, 6);
        write_operation_elements(&mut out, &contract.operations, 6);
        out.push_str("    }\n");
    }

    for contract in &manifest.contracts.provided {
        let Some(target) = contract_ref(contract, &capability_ids, &provided_contract_ids) else {
            continue;
        };
        if let Some(packages) = contract_implementations.get(&contract.id) {
            for package in packages {
                let Some(source) =
                    package_ref(package, &package_owner, &capability_ids, &package_ids)
                else {
                    continue;
                };
                if source == target {
                    continue;
                }
                writeln!(out, "    {source} -[implements]-> {target} \"implements\"")
                    .expect("write to string");
            }
        }
    }

    for contract in &manifest.contracts.unresolved {
        let Some(target) = external_contract_ids.get(&contract.id) else {
            continue;
        };
        let source = contract
            .capability
            .as_deref()
            .and_then(|capability| capability_ids.get(capability))
            .unwrap_or(root_id.as_str());
        writeln!(out, "    {source} -[consumes]-> {target} \"consumes\"").expect("write to string");
    }

    for dep in &manifest.dependencies {
        let Some(source) = package_ref(&dep.source, &package_owner, &capability_ids, &package_ids)
        else {
            continue;
        };
        let Some(target) = package_ref(&dep.target, &package_owner, &capability_ids, &package_ids)
        else {
            continue;
        };
        if source == target {
            continue;
        }
        writeln!(
            out,
            "    {source} -[{}]-> {target} \"{} ({})\" {{",
            dep.relation,
            escape_likec4_string(&dep.relation),
            dep.weight
        )
        .expect("write to string");
        out.push_str("      metadata {\n");
        write_metadata_string(&mut out, 8, "weight", &dep.weight.to_string());
        write_metadata_string(&mut out, 8, "confidence", &format!("{:.3}", dep.confidence));
        for sample in dep.sample_edges.iter().take(5) {
            write_metadata_string(
                &mut out,
                8,
                "sample_edge",
                &format!(
                    "{} -> {} @ {}",
                    sample.source_node, sample.target_node, sample.source_file
                ),
            );
        }
        out.push_str("      }\n");
        out.push_str("    }\n");
    }

    for flow in &manifest.flows {
        let Some(source) = capability_ids.get(&flow.source) else {
            continue;
        };
        let Some(target) = capability_ids.get(&flow.target) else {
            continue;
        };
        writeln!(
            out,
            "    {source} -[{}]-> {target} \"{} ({})\" {{",
            flow.relation,
            escape_likec4_string(&flow.relation),
            flow.weight
        )
        .expect("write to string");
        out.push_str("      metadata {\n");
        write_metadata_string(&mut out, 8, "weight", &flow.weight.to_string());
        for evidence in &flow.evidence {
            write_metadata_string(&mut out, 8, "evidence", evidence);
        }
        out.push_str("      }\n");
        out.push_str("    }\n");
    }

    out.push_str("  }\n");
    out.push_str("}\n");
    out
}

fn landscape_model(landscape: &ArchitectureLandscape) -> String {
    let mut ids = UniqueIds::default();
    for service in &landscape.services {
        ids.insert(&service.name, &format!("svc_{}", service.name));
    }
    let visible_contracts = visible_landscape_contracts(landscape);
    let show_all_contracts = visible_contracts.is_empty();
    let mut contract_ids = UniqueIds::default();
    for contract in &landscape.contracts {
        let key = landscape_contract_key(&contract.service, &contract.id);
        if show_all_contracts || visible_contracts.contains(&key) {
            contract_ids.insert(&key, &format!("api_{}", contract.id));
        }
    }

    let mut out = String::from("model {\n");
    out.push_str("  landscape = system \"Service landscape\" {\n");
    out.push_str("    metadata {\n");
    out.push_str("      generator \"graphify-rs\"\n");
    out.push_str("      source_model \"architecture_landscape\"\n");
    out.push_str("    }\n");
    for service in &landscape.services {
        let Some(id) = ids.get(&service.name) else {
            continue;
        };
        writeln!(
            out,
            "    {id} = system \"{}\" {{",
            escape_likec4_string(&service.name)
        )
        .expect("write to string");
        out.push_str("      metadata {\n");
        for prefix in &service.module_prefixes {
            write_metadata_string(&mut out, 8, "module_prefix", prefix);
        }
        out.push_str("      }\n");

        for contract in landscape.contracts.iter().filter(|contract| {
            let key = landscape_contract_key(&contract.service, &contract.id);
            contract.service == service.name
                && (show_all_contracts || visible_contracts.contains(&key))
        }) {
            let key = landscape_contract_key(&contract.service, &contract.id);
            let Some(contract_id) = contract_ids.get(&key) else {
                continue;
            };
            writeln!(
                out,
                "      {contract_id} = api \"{}\" {{",
                escape_likec4_string(&contract.name)
            )
            .expect("write to string");
            write_landscape_contract_metadata(&mut out, contract, 8);
            write_operation_elements(&mut out, &contract.operations, 8);
            out.push_str("      }\n");
        }
        out.push_str("    }\n");
    }

    if !landscape.dependencies.is_empty() && !landscape.services.is_empty() {
        out.push('\n');
    }
    for dep in &landscape.dependencies {
        if !landscape_dependency_has_visual_signal(dep) {
            continue;
        }
        let Some(source) = ids.get(&dep.source) else {
            continue;
        };
        let Some(target) = ids.get(&dep.target) else {
            continue;
        };
        if dep.contracts.is_empty() {
            write_landscape_dependency_edge(&mut out, dep, source, target, None, &dep.operations);
            continue;
        }

        for contract in &dep.contracts {
            let target_ref = {
                let key = landscape_contract_key(&dep.target, contract);
                contract_ids
                    .get(&key)
                    .map(|contract_id| format!("{target}.{contract_id}"))
                    .unwrap_or_else(|| target.to_string())
            };
            let operations = landscape_dependency_contract_operations(landscape, dep, contract);
            write_landscape_dependency_edge(
                &mut out,
                dep,
                source,
                &target_ref,
                Some(contract),
                &operations,
            );
        }
    }
    out.push_str("  }\n");
    out.push_str("}\n");
    out
}

fn service_views(manifest: &ArchitectureManifest) -> String {
    let root_id = service_root_id(&manifest.service.name);
    let mut out = String::from("views {\n");
    out.push_str("  view index {\n");
    writeln!(
        out,
        "    title \"{} architecture\"",
        escape_likec4_string(&manifest.service.name)
    )
    .expect("write to string");
    out.push_str("    include *\n");
    out.push_str("    autoLayout TopBottom\n");
    out.push_str("  }\n");

    writeln!(out, "  view service of {root_id} {{").expect("write to string");
    writeln!(
        out,
        "    title \"{} capabilities\"",
        escape_likec4_string(&manifest.service.name)
    )
    .expect("write to string");
    out.push_str("    include element.kind = container\n");
    out.push_str("    include element.kind = api\n");
    out.push_str("    include element.kind = externalApi\n");
    out.push_str("    autoLayout TopBottom\n");
    out.push_str("  }\n");

    if !manifest.contracts.provided.is_empty() || !manifest.contracts.unresolved.is_empty() {
        writeln!(out, "  view contracts of {root_id} {{").expect("write to string");
        writeln!(
            out,
            "    title \"{} contracts and operations\"",
            escape_likec4_string(&manifest.service.name)
        )
        .expect("write to string");
        out.push_str("    include element.kind = container\n");
        out.push_str("    include element.kind = api\n");
        out.push_str("    include element.kind = operation\n");
        out.push_str("    include element.kind = externalApi\n");
        out.push_str("    autoLayout TopBottom\n");
        out.push_str("  }\n");
    }
    if !manifest.packages.is_empty() {
        writeln!(out, "  view modules of {root_id} {{").expect("write to string");
        writeln!(
            out,
            "    title \"{} implementation modules\"",
            escape_likec4_string(&manifest.service.name)
        )
        .expect("write to string");
        out.push_str("    include element.kind = component\n");
        out.push_str("    include element.kind = api\n");
        out.push_str("    autoLayout LeftRight\n");
        out.push_str("  }\n");
    }
    out.push_str("}\n");
    out
}

fn landscape_views() -> String {
    [
        "views {",
        "  view index {",
        "    title \"Service landscape\"",
        "    include landscape.*",
        "    autoLayout LeftRight",
        "  }",
        "}",
        "",
    ]
    .join("\n")
}

fn service_readme() -> String {
    [
        "# graphify service architecture",
        "",
        "Generated from `.graphify/architecture.json` by graphify-rs.",
        "",
        "Default view is service capabilities/contracts plus inferred capability flows; package dependencies are evidence, not diagram noise.",
        "LikeC4 manual layout snapshots persist under `.likec4/`.",
        "",
    ]
    .join("\n")
}

fn landscape_readme() -> String {
    [
        "# graphify service landscape",
        "",
        "Generated from multiple graphify architecture manifests.",
        "LikeC4 manual layout snapshots persist under `.likec4/`.",
        "",
    ]
    .join("\n")
}

fn config_json(name: &str) -> anyhow::Result<String> {
    let config = serde_json::json!({
        "$schema": "https://likec4.dev/schemas/config.json",
        "name": name,
        "title": "Graphify architecture",
        "exclude": [
            "**/.likec4/**",
            "**/node_modules/**",
            "**/dist/**",
            "**/build/**",
            "**/target/**"
        ],
        "landingPage": {
            "redirect": true
        },
        "implicitViews": true,
        "manualLayouts": {
            "outDir": ".likec4"
        }
    });
    Ok(format!("{}\n", serde_json::to_string_pretty(&config)?))
}

fn service_root_id(service_name: &str) -> String {
    let id = sanitize_identifier(service_name);
    if id.is_empty() {
        ROOT_ID.to_string()
    } else {
        id
    }
}

fn sorted_nodes(graph: &KnowledgeGraph) -> Vec<&GraphNode> {
    let mut nodes = graph.nodes();
    nodes.sort_by(|a, b| a.id.cmp(&b.id));
    nodes
}

fn sorted_edges(graph: &KnowledgeGraph) -> Vec<&GraphEdge> {
    let mut edges = graph.edges();
    edges.sort_by(|a, b| {
        a.source
            .cmp(&b.source)
            .then_with(|| a.target.cmp(&b.target))
            .then_with(|| a.relation.cmp(&b.relation))
    });
    edges
}

fn include_node_in_architecture(
    node: &GraphNode,
    root: Option<&Path>,
    options: &ArchitectureOptions,
) -> bool {
    let path = display_path(&node.source_file, root);
    if !matches_path_filters(&path, options) {
        return false;
    }
    if !is_architecture_source_path(&path)
        || is_transient_path(&path)
        || quality::is_low_signal_node(node)
        || is_import_node(node)
    {
        return false;
    }
    matches!(
        node.node_type,
        NodeType::File
            | NodeType::Module
            | NodeType::Package
            | NodeType::Namespace
            | NodeType::Struct
            | NodeType::Class
            | NodeType::Interface
            | NodeType::Trait
            | NodeType::Enum
            | NodeType::Function
            | NodeType::Method
    )
}

fn include_edge_in_architecture(edge: &GraphEdge) -> bool {
    !matches!(
        edge.relation.as_str(),
        "defines" | "imports" | "contains" | "next_section"
    ) && !is_transient_path(&edge.source_file)
}

fn is_import_node(node: &GraphNode) -> bool {
    node.id.contains("_import_")
        || node
            .source_location
            .as_deref()
            .is_some_and(|location| location.eq_ignore_ascii_case("import"))
}

fn is_architecture_source_path(path: &str) -> bool {
    let lower = normalize_path(path).to_ascii_lowercase();
    let Some(ext) = Path::new(&lower).extension().and_then(|ext| ext.to_str()) else {
        return false;
    };
    matches!(
        ext,
        "go" | "rs"
            | "ts"
            | "tsx"
            | "js"
            | "jsx"
            | "py"
            | "java"
            | "kt"
            | "kts"
            | "cs"
            | "c"
            | "h"
            | "cpp"
            | "hpp"
            | "cc"
            | "cxx"
            | "php"
            | "rb"
            | "swift"
            | "scala"
            | "dart"
            | "ex"
            | "exs"
            | "lua"
            | "jl"
            | "zig"
            | "sql"
            | "proto"
    )
}

fn architecture_relation(relation: &str) -> String {
    match relation {
        "calls" | "uses" | "reads_from" | "writes_to" | "references" | "alters" => {
            sanitize_identifier(relation)
        }
        _ => "uses".to_string(),
    }
}

fn safe_confidence(edge: &GraphEdge) -> f64 {
    if edge.confidence_score.is_finite() {
        edge.confidence_score.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

fn display_path(path: &str, root: Option<&Path>) -> String {
    if let Some(root) = root {
        let source = Path::new(path);
        if source.is_absolute()
            && let Ok(relative) = source.strip_prefix(root)
        {
            return normalize_path(&relative.to_string_lossy());
        }
    }
    normalize_path(path)
}

fn normalize_path(path: &str) -> String {
    path.trim_start_matches("./").replace('\\', "/")
}

fn package_path_for_source(source_file: &str) -> String {
    let normalized = normalize_path(source_file);
    let path = Path::new(&normalized);
    path.parent()
        .and_then(|parent| parent.to_str())
        .map(normalize_path)
        .filter(|parent| !parent.is_empty())
        .unwrap_or_else(|| {
            path.file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or("root")
                .to_string()
        })
}

fn compact_package_path(path: &str) -> String {
    let parts = path_parts(path);
    if parts.len() <= 3 {
        return path.to_string();
    }
    let keep = match parts.first().copied().unwrap_or_default() {
        "cmd" => 2,
        "internal" | "pkg" | "proto" | "api" | "database" | "db" => 3,
        _ => 2,
    };
    parts.into_iter().take(keep).collect::<Vec<_>>().join("/")
}

fn coarse_package_path(path: &str) -> String {
    let parts = path_parts(path);
    if parts.len() <= 2 {
        return path.to_string();
    }
    parts.into_iter().take(2).collect::<Vec<_>>().join("/")
}

fn path_parts(path: &str) -> Vec<&str> {
    path.split('/').filter(|part| !part.is_empty()).collect()
}

fn package_kind(path: &str) -> &'static str {
    let lower = path.to_ascii_lowercase();
    if lower.starts_with("cmd/") {
        "entrypoint"
    } else if lower.contains("/api") || lower.starts_with("api") {
        "api"
    } else if lower.contains("domain") {
        "domain"
    } else if lower.contains("repository") || lower.contains("storage") {
        "repository"
    } else if lower.contains("infrastructure") || lower.contains("infra") {
        "infrastructure"
    } else if lower.contains("proto") {
        "proto"
    } else if lower.contains("event") || lower.contains("kafka") || lower.contains("broker") {
        "event"
    } else {
        "package"
    }
}

fn collect_external_dependencies(
    graph: &KnowledgeGraph,
    root: Option<&Path>,
    owner_by_node: &HashMap<String, String>,
    module_prefixes: &[String],
) -> Vec<ArchitectureExternalDependency> {
    let mut accs: HashMap<String, ExternalAcc> = HashMap::new();

    for edge in sorted_edges(graph) {
        if edge.relation != "imports" {
            continue;
        }
        let Some(target) = graph.get_node(&edge.target) else {
            continue;
        };
        let name = target.label.trim();
        if name.is_empty()
            || module_prefixes
                .iter()
                .any(|prefix| name.starts_with(prefix))
            || is_runtime_import(name)
        {
            continue;
        }
        let source_owner = owner_by_node
            .get(&edge.source)
            .cloned()
            .unwrap_or_else(|| package_path_for_source(&display_path(&edge.source_file, root)));
        let acc = accs.entry(name.to_string()).or_insert_with(|| ExternalAcc {
            kind: classify_interface_name(name).to_string(),
            ..ExternalAcc::default()
        });
        acc.weight += 1;
        acc.imported_by.insert(source_owner);
    }

    let mut deps = accs
        .into_iter()
        .map(|(name, acc)| ArchitectureExternalDependency {
            name,
            kind: acc.kind,
            weight: acc.weight,
            imported_by: acc.imported_by.into_iter().collect(),
        })
        .collect::<Vec<_>>();
    deps.sort_by(|a, b| b.weight.cmp(&a.weight).then_with(|| a.name.cmp(&b.name)));
    deps.truncate(100);
    deps.sort_by(|a, b| a.name.cmp(&b.name));
    deps
}

fn collect_provided_interfaces(
    packages: &[ArchitecturePackage],
    module_prefixes: &[String],
) -> ArchitectureInterfaces {
    let mut apis = BTreeSet::new();
    let mut events = BTreeSet::new();
    let mut protos = BTreeSet::new();

    for package in packages {
        match package.kind.as_str() {
            "api" => {
                apis.insert(package.path.clone());
            }
            "event" => {
                events.insert(package.path.clone());
            }
            "proto" => {
                protos.insert(package.path.clone());
            }
            _ => {}
        }
    }
    for prefix in module_prefixes {
        protos.insert(format!("{prefix}/proto"));
    }

    ArchitectureInterfaces {
        apis: apis.into_iter().collect(),
        events: events.into_iter().collect(),
        protos: protos.into_iter().collect(),
    }
}

fn collect_consumed_interfaces(
    external_dependencies: &[ArchitectureExternalDependency],
) -> ArchitectureInterfaces {
    let mut apis = BTreeSet::new();
    let mut events = BTreeSet::new();
    let mut protos = BTreeSet::new();

    for dep in external_dependencies {
        match dep.kind.as_str() {
            "api" => {
                apis.insert(dep.name.clone());
            }
            "event" => {
                events.insert(dep.name.clone());
            }
            "proto" => {
                protos.insert(dep.name.clone());
            }
            _ => {}
        }
    }

    ArchitectureInterfaces {
        apis: apis.into_iter().collect(),
        events: events.into_iter().collect(),
        protos: protos.into_iter().collect(),
    }
}

fn classify_interface_name(name: &str) -> &'static str {
    let lower = name.to_ascii_lowercase();
    if lower.contains("proto") || lower.contains("/pb") || lower.ends_with(".proto") {
        "proto"
    } else if lower.contains("event")
        || lower.contains("kafka")
        || lower.contains("broker")
        || lower.contains("stream")
    {
        "event"
    } else if lower.contains("api") || lower.contains("grpc") || lower.contains("client") {
        "api"
    } else {
        "module"
    }
}

fn interface_names(interfaces: &ArchitectureInterfaces) -> Vec<String> {
    interfaces
        .apis
        .iter()
        .chain(interfaces.events.iter())
        .chain(interfaces.protos.iter())
        .cloned()
        .collect()
}

fn interfaces_match(consumed: &str, provided: &str, target_service: &str) -> bool {
    if consumed == provided || consumed.starts_with(provided) || provided.starts_with(consumed) {
        return true;
    }
    let consumed_tokens = meaningful_tokens(consumed);
    let provided_tokens = meaningful_tokens(provided);
    !target_service.is_empty()
        && consumed_tokens.contains(&target_service.to_ascii_lowercase())
        && !consumed_tokens.is_disjoint(&provided_tokens)
}

fn service_contract_tokens(manifest: &ArchitectureManifest) -> HashSet<String> {
    let mut tokens = HashSet::new();
    tokens.extend(meaningful_tokens(&manifest.service.name));
    for capability in &manifest.capabilities {
        if !matches!(capability.kind.as_str(), "contract" | "api") {
            continue;
        }
        tokens.extend(capability_contract_tokens(capability));
    }
    tokens
}

fn capability_contract_tokens(capability: &ArchitectureCapability) -> HashSet<String> {
    let mut tokens = HashSet::new();
    tokens.extend(meaningful_tokens(&capability.id));
    tokens.extend(meaningful_tokens(&capability.name));
    for api in &capability.provided_apis {
        let service = api.split('.').next().unwrap_or(api);
        tokens.extend(meaningful_tokens(service));
    }
    tokens
}

fn contract_alias_overlap<'a>(
    consumed: &'a ArchitectureContract,
    provided: &'a ArchitectureContract,
) -> Option<&'a String> {
    let provided_aliases = provided.aliases.iter().collect::<HashSet<_>>();
    consumed
        .aliases
        .iter()
        .filter(|alias| provided_aliases.contains(alias))
        .filter(|alias| !is_generic_contract_alias(alias))
        .min()
}

fn service_is_contract_catalog(manifest: &ArchitectureManifest) -> bool {
    !manifest.contracts.provided.is_empty()
        && manifest
            .capabilities
            .iter()
            .all(|capability| capability.kind == "contract")
        && manifest
            .packages
            .iter()
            .all(|package| package.kind == "proto" || package.kind == "package")
}

fn is_generic_contract_alias(alias: &str) -> bool {
    let parts = alias.split(':').collect::<Vec<_>>();
    parts.len() < 3
        || parts
            .iter()
            .skip(1)
            .all(|part| is_generic_contract_segment(part))
}

fn first_token_overlap<'a>(
    source_tokens: &'a HashSet<String>,
    target_tokens: &'a HashSet<String>,
) -> Option<&'a String> {
    source_tokens
        .intersection(target_tokens)
        .filter(|token| !is_generic_service_token(token))
        .min()
}

fn package_mentions_service(package: &ArchitecturePackage, service_token: &str) -> bool {
    if service_token.len() <= 2 {
        return false;
    }
    let service_token = service_token.to_ascii_lowercase();
    meaningful_tokens(&package.path).contains(&service_token)
        || (package.kind != "package"
            && package
                .source_files
                .iter()
                .any(|source| meaningful_tokens(source).contains(&service_token)))
}

fn meaningful_tokens(value: &str) -> HashSet<String> {
    value
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .map(str::to_ascii_lowercase)
        .filter(|token| token.len() > 2)
        .filter(|token| {
            !matches!(
                token.as_str(),
                "com"
                    | "org"
                    | "net"
                    | "src"
                    | "pkg"
                    | "api"
                    | "proto"
                    | "internal"
                    | "service"
                    | "services"
                    | "shared"
            )
        })
        .collect()
}

fn is_generic_service_token(token: &str) -> bool {
    matches!(
        token,
        "api"
            | "client"
            | "common"
            | "connect"
            | "event"
            | "events"
            | "generated"
            | "internal"
            | "model"
            | "models"
            | "proto"
            | "public"
            | "service"
            | "services"
            | "shared"
            | "stream"
            | "streams"
    )
}

fn detect_module_prefixes(root: Option<&Path>) -> Vec<String> {
    let Some(root) = root else {
        return Vec::new();
    };
    let mut prefixes = BTreeSet::new();

    let go_mod = root.join("go.mod");
    if let Ok(content) = fs::read_to_string(go_mod) {
        for line in content.lines() {
            let line = line.trim();
            if let Some(module) = line.strip_prefix("module ") {
                let module = module.trim();
                if !module.is_empty() {
                    prefixes.insert(module.to_string());
                }
            }
        }
    }

    let cargo_toml = root.join("Cargo.toml");
    if let Ok(content) = fs::read_to_string(cargo_toml)
        && let Ok(value) = content.parse::<toml::Value>()
        && let Some(name) = value
            .get("package")
            .and_then(|package| package.get("name"))
            .and_then(|name| name.as_str())
    {
        prefixes.insert(name.to_string());
    }

    let package_json = root.join("package.json");
    if let Ok(content) = fs::read_to_string(package_json)
        && let Ok(value) = serde_json::from_str::<serde_json::Value>(&content)
        && let Some(name) = value.get("name").and_then(|name| name.as_str())
    {
        prefixes.insert(name.to_string());
    }

    prefixes.into_iter().collect()
}

fn is_runtime_import(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    if lower.starts_with("std::")
        || lower.starts_with("core::")
        || lower.starts_with("alloc::")
        || lower.starts_with("crate::")
        || lower.starts_with("super::")
        || lower.starts_with("self::")
    {
        return true;
    }
    if !lower.contains('.') && !lower.contains("://") {
        return true;
    }
    matches!(
        lower.as_str(),
        "context" | "fmt" | "errors" | "time" | "strings" | "sync" | "encoding/json"
    )
}

fn matches_path_filters(path: &str, options: &ArchitectureOptions) -> bool {
    let path = normalize_path(path);
    if !options.include_paths.is_empty()
        && !options
            .include_paths
            .iter()
            .any(|pattern| path_matches_pattern(&path, pattern))
    {
        return false;
    }
    !options
        .exclude_paths
        .iter()
        .any(|pattern| path_matches_pattern(&path, pattern))
}

fn path_matches_pattern(path: &str, pattern: &str) -> bool {
    let pattern = normalize_path(pattern);
    if pattern.is_empty() {
        return false;
    }
    if pattern.contains('*') {
        return wildcard_match(path, &pattern);
    }
    if pattern.ends_with('/') {
        return path.starts_with(&pattern);
    }
    path == pattern || path.starts_with(&format!("{pattern}/"))
}

fn wildcard_match(value: &str, pattern: &str) -> bool {
    if let Some(stripped) = pattern.strip_prefix("**/") {
        return wildcard_match_inner(value, stripped) || wildcard_match_inner(value, pattern);
    }
    wildcard_match_inner(value, pattern)
}

fn wildcard_match_inner(value: &str, pattern: &str) -> bool {
    if pattern == "*" || pattern == "**" {
        return true;
    }

    let value = value.as_bytes();
    let pattern = pattern.as_bytes();
    let mut value_idx = 0usize;
    let mut pattern_idx = 0usize;
    let mut star_idx = None;
    let mut star_match_idx = 0usize;

    while value_idx < value.len() {
        if pattern_idx < pattern.len() && pattern[pattern_idx] == b'*' {
            star_idx = Some(pattern_idx);
            star_match_idx = value_idx;
            pattern_idx += 1;
        } else if pattern_idx < pattern.len() && pattern[pattern_idx] == value[value_idx] {
            value_idx += 1;
            pattern_idx += 1;
        } else if let Some(star) = star_idx {
            pattern_idx = star + 1;
            star_match_idx += 1;
            value_idx = star_match_idx;
        } else {
            return false;
        }
    }

    while pattern_idx < pattern.len() && pattern[pattern_idx] == b'*' {
        pattern_idx += 1;
    }

    pattern_idx == pattern.len()
}

fn is_transient_path(path: &str) -> bool {
    let lower = normalize_path(path).to_ascii_lowercase();
    lower.starts_with(".graphify/")
        || lower.contains("/.graphify/")
        || lower.starts_with(".likec4/")
        || lower.contains("/.likec4/")
        || lower.contains("/node_modules/")
        || lower.contains("/target/")
        || lower.starts_with("docs/")
        || lower.contains("/docs/")
        || lower.starts_with("doc/")
        || lower.contains("/doc/")
        || lower.starts_with("documentation/")
        || lower.contains("/documentation/")
        || lower.starts_with("scratch/")
        || lower.contains("/scratch/")
        || lower.ends_with(".user.js")
        || lower.ends_with(".tmp")
        || lower.ends_with(".temp")
        || lower.ends_with(".bak")
        || lower.ends_with(".patch")
        || is_test_source_path(&lower)
}

fn is_test_source_path(lower_path: &str) -> bool {
    let file = lower_path.rsplit('/').next().unwrap_or(lower_path);
    file.starts_with("test_")
        || file.contains("_test.")
        || file.contains(".test.")
        || file.contains("_spec.")
        || file.contains(".spec.")
}

#[derive(Default)]
struct UniqueIds {
    ids: HashMap<String, String>,
    used: HashSet<String>,
}

impl UniqueIds {
    fn insert(&mut self, raw: &str, preferred: &str) {
        let base = sanitize_identifier(preferred);
        let mut candidate = base.clone();
        let mut suffix = 2;
        while self.used.contains(&candidate) {
            candidate = format!("{base}_{suffix}");
            suffix += 1;
        }
        self.used.insert(candidate.clone());
        self.ids.insert(raw.to_string(), candidate);
    }

    fn get(&self, raw: &str) -> Option<&str> {
        self.ids.get(raw).map(String::as_str)
    }
}

fn write_metadata_string(out: &mut String, indent: usize, key: &str, value: &str) {
    let padding = " ".repeat(indent);
    let escaped = escape_likec4_string(value);
    writeln!(out, "{padding}{key} \"{escaped}\"").expect("write to string");
}

fn write_contract_metadata(out: &mut String, contract: &ArchitectureContract, indent: usize) {
    let metadata_indent = indent;
    let value_indent = indent + 2;
    let padding = " ".repeat(metadata_indent);
    writeln!(out, "{padding}metadata {{").expect("write to string");
    write_metadata_string(out, value_indent, "contract_id", &contract.id);
    write_metadata_string(out, value_indent, "contract_kind", &contract.kind);
    write_metadata_string(
        out,
        value_indent,
        "contract_source_kind",
        &contract.source_kind,
    );
    write_metadata_string(out, value_indent, "contract_source", &contract.source);
    if let Some(capability) = &contract.capability {
        write_metadata_string(out, value_indent, "capability", capability);
    }
    for alias in contract.aliases.iter().take(20) {
        write_metadata_string(out, value_indent, "contract_alias", alias);
    }
    for operation in contract.operations.iter().take(20) {
        write_metadata_string(out, value_indent, "operation", operation);
    }
    for evidence in contract.evidence.iter().take(20) {
        write_metadata_string(out, value_indent, "evidence", evidence);
    }
    writeln!(out, "{padding}}}").expect("write to string");
}

fn write_landscape_contract_metadata(
    out: &mut String,
    contract: &ArchitectureLandscapeContract,
    indent: usize,
) {
    let metadata_indent = indent;
    let value_indent = indent + 2;
    let padding = " ".repeat(metadata_indent);
    writeln!(out, "{padding}metadata {{").expect("write to string");
    write_metadata_string(out, value_indent, "service", &contract.service);
    write_metadata_string(out, value_indent, "contract_id", &contract.id);
    write_metadata_string(out, value_indent, "contract_kind", &contract.kind);
    if let Some(capability) = &contract.capability {
        write_metadata_string(out, value_indent, "capability", capability);
    }
    for alias in contract.aliases.iter().take(20) {
        write_metadata_string(out, value_indent, "contract_alias", alias);
    }
    for operation in contract.operations.iter().take(20) {
        write_metadata_string(out, value_indent, "operation", operation);
    }
    writeln!(out, "{padding}}}").expect("write to string");
}

fn write_operation_elements(out: &mut String, operations: &[String], indent: usize) {
    let operations = operations
        .iter()
        .filter(|operation| !operation.trim().is_empty())
        .take(50)
        .collect::<Vec<_>>();
    let mut ids = UniqueIds::default();
    for operation in &operations {
        ids.insert(operation, &format!("op_{operation}"));
    }

    let padding = " ".repeat(indent);
    for operation in operations {
        let Some(id) = ids.get(operation) else {
            continue;
        };
        writeln!(
            out,
            "{padding}{id} = operation \"{}\"",
            escape_likec4_string(operation)
        )
        .expect("write to string");
    }
}

fn write_package_component(
    out: &mut String,
    id: &str,
    package: &ArchitecturePackage,
    operations: &[String],
    indent: usize,
) {
    let padding = " ".repeat(indent);
    writeln!(
        out,
        "{padding}{id} = component \"{}\" {{",
        escape_likec4_string(&package.label)
    )
    .expect("write to string");
    let metadata_indent = indent + 2;
    let value_indent = indent + 4;
    let metadata_padding = " ".repeat(metadata_indent);
    writeln!(out, "{metadata_padding}metadata {{").expect("write to string");
    write_metadata_string(out, value_indent, "graphify_package_path", &package.path);
    write_metadata_string(out, value_indent, "graphify_package_kind", &package.kind);
    write_metadata_string(
        out,
        value_indent,
        "source_files",
        &package.source_files.len().to_string(),
    );
    write_metadata_string(
        out,
        value_indent,
        "node_count",
        &package.node_count.to_string(),
    );
    if let Some(community) = package.community {
        write_metadata_string(out, value_indent, "community", &community.to_string());
    }
    writeln!(out, "{metadata_padding}}}").expect("write to string");
    write_operation_elements(out, operations, indent + 2);
    writeln!(out, "{padding}}}").expect("write to string");
}

fn package_owner_map(manifest: &ArchitectureManifest) -> HashMap<String, String> {
    let mut owners = HashMap::new();
    for capability in &manifest.capabilities {
        for package in &capability.packages {
            owners
                .entry(package.clone())
                .or_insert_with(|| capability.id.clone());
        }
    }
    owners
}

fn package_ref(
    package: &str,
    package_owner: &HashMap<String, String>,
    capability_ids: &UniqueIds,
    package_ids: &UniqueIds,
) -> Option<String> {
    let package_id = package_ids.get(package)?;
    if let Some(capability) = package_owner.get(package)
        && let Some(capability_id) = capability_ids.get(capability)
    {
        return Some(format!("{capability_id}.{package_id}"));
    }
    Some(package_id.to_string())
}

fn contract_ref(
    contract: &ArchitectureContract,
    capability_ids: &UniqueIds,
    contract_ids: &UniqueIds,
) -> Option<String> {
    let contract_id = contract_ids.get(&contract.id)?;
    if let Some(capability) = contract.capability.as_deref()
        && let Some(capability_id) = capability_ids.get(capability)
    {
        return Some(format!("{capability_id}.{contract_id}"));
    }
    Some(contract_id.to_string())
}

fn capability_operations_by_package(
    capability: &ArchitectureCapability,
) -> BTreeMap<String, Vec<String>> {
    let mut operations = BTreeMap::<String, BTreeSet<String>>::new();
    let capability_packages = capability.packages.iter().collect::<HashSet<_>>();
    for operation in &capability.operations {
        for evidence in &capability.evidence {
            let Some(source) = evidence.strip_prefix(&format!("operation:{operation}@")) else {
                continue;
            };
            let package = package_path_for_source(source);
            if capability_packages.contains(&package) {
                operations
                    .entry(package)
                    .or_default()
                    .insert(operation.clone());
            }
        }
    }
    operations
        .into_iter()
        .map(|(package, operations)| (package, operations.into_iter().collect()))
        .collect()
}

fn contract_implementation_packages(
    manifest: &ArchitectureManifest,
) -> HashMap<String, Vec<String>> {
    let known_packages = manifest
        .packages
        .iter()
        .map(|package| package.path.as_str())
        .collect::<HashSet<_>>();
    let capability_by_id = manifest
        .capabilities
        .iter()
        .map(|capability| (capability.id.as_str(), capability))
        .collect::<HashMap<_, _>>();

    let mut implementations = HashMap::new();
    for contract in &manifest.contracts.provided {
        let mut packages = BTreeSet::new();
        if let Some(capability) = contract
            .capability
            .as_deref()
            .and_then(|capability| capability_by_id.get(capability))
        {
            packages.extend(
                capability
                    .packages
                    .iter()
                    .filter(|package| known_packages.contains(package.as_str()))
                    .cloned(),
            );
        }
        for dep in &manifest.dependencies {
            if known_packages.contains(dep.source.as_str())
                && generated_package_matches_contract(&dep.target, contract)
            {
                packages.insert(dep.source.clone());
            }
        }
        if !packages.is_empty() {
            implementations.insert(contract.id.clone(), packages.into_iter().collect());
        }
    }
    implementations
}

fn generated_package_matches_contract(package: &str, contract: &ArchitectureContract) -> bool {
    if contract.source_kind != "proto" {
        return false;
    }
    let source = normalize_path(&contract.source);
    let Some(parent) = Path::new(&source).parent().and_then(Path::to_str) else {
        return false;
    };
    let parent = normalize_path(parent)
        .strip_prefix("proto/")
        .map(str::to_string)
        .unwrap_or_else(|| normalize_path(parent));
    let package = normalize_path(package)
        .strip_prefix("internal/proto/")
        .map(str::to_string)
        .or_else(|| {
            normalize_path(package)
                .strip_prefix("pkg/proto/")
                .map(str::to_string)
        })
        .unwrap_or_else(|| normalize_path(package));
    let Some(rest) = package.strip_prefix(&format!("{parent}/")) else {
        return false;
    };
    !rest.is_empty() && rest.split('/').count() == 1
}

fn duplicate_contract_names(contracts: &[ArchitectureContract]) -> HashSet<String> {
    let mut counts = HashMap::<String, usize>::new();
    for contract in contracts {
        *counts.entry(contract.name.clone()).or_default() += 1;
    }
    counts
        .into_iter()
        .filter_map(|(name, count)| (count > 1).then_some(name))
        .collect()
}

fn contract_display_name(
    contract: &ArchitectureContract,
    duplicate_names: &HashSet<String>,
) -> String {
    if !duplicate_names.contains(&contract.name) {
        return contract.name.clone();
    }
    match contract_source_qualifier(&contract.source) {
        Some(qualifier) => format!("{} ({qualifier})", contract.name),
        None => contract.name.clone(),
    }
}

fn contract_source_qualifier(source: &str) -> Option<String> {
    let source = normalize_path(source);
    let parent = Path::new(&source).parent()?.to_str()?;
    let parent = normalize_path(parent);
    let parent = parent.strip_prefix("proto/").unwrap_or(&parent);
    let parts = path_parts(parent);
    let last = parts.last().copied()?;
    if is_version_segment(last) && parts.len() >= 2 {
        return Some(parts[parts.len() - 2..].join("/"));
    }
    Some(last.to_string())
}

fn landscape_contract_key(service: &str, contract: &str) -> String {
    format!("{service}::{contract}")
}

fn visible_landscape_contracts(landscape: &ArchitectureLandscape) -> HashSet<String> {
    landscape
        .dependencies
        .iter()
        .flat_map(|dependency| {
            dependency
                .contracts
                .iter()
                .map(|contract| landscape_contract_key(&dependency.target, contract))
        })
        .collect()
}

fn landscape_dependency_has_visual_signal(dep: &ArchitectureServiceDependency) -> bool {
    !dep.contracts.is_empty()
        || dep.evidence.iter().any(|evidence| {
            evidence.starts_with("module_prefix:") || evidence.starts_with("interface:")
        })
}

fn landscape_dependency_contract_operations(
    landscape: &ArchitectureLandscape,
    dep: &ArchitectureServiceDependency,
    contract_id: &str,
) -> Vec<String> {
    let dep_operations = dep.operations.iter().collect::<HashSet<_>>();
    let mut operations = landscape
        .contracts
        .iter()
        .find(|contract| contract.service == dep.target && contract.id == contract_id)
        .map(|contract| {
            contract
                .operations
                .iter()
                .filter(|operation| dep_operations.contains(operation))
                .cloned()
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if operations.is_empty() && dep.contracts.len() == 1 {
        operations = dep.operations.clone();
    }
    operations
}

fn write_landscape_dependency_edge(
    out: &mut String,
    dep: &ArchitectureServiceDependency,
    source: &str,
    target_ref: &str,
    contract: Option<&str>,
    operations: &[String],
) {
    writeln!(
        out,
        "    {source} -[{}]-> {target_ref} \"{}\" {{",
        dep.relation,
        escape_likec4_string(&landscape_dependency_label(dep, contract, operations))
    )
    .expect("write to string");
    out.push_str("      metadata {\n");
    write_metadata_string(out, 8, "weight", &dep.weight.to_string());
    if let Some(contract) = contract {
        write_metadata_string(out, 8, "contract", contract);
    }
    for operation in operations {
        write_metadata_string(out, 8, "operation", operation);
    }
    for evidence in &dep.evidence {
        write_metadata_string(out, 8, "evidence", evidence);
    }
    out.push_str("      }\n");
    out.push_str("    }\n");
}

fn landscape_dependency_label(
    dep: &ArchitectureServiceDependency,
    contract: Option<&str>,
    operations: &[String],
) -> String {
    let mut label = dep.relation.clone();
    if !operations.is_empty() {
        let mut operation_labels = operations.iter().take(3).cloned().collect::<Vec<_>>();
        if operations.len() > operation_labels.len() {
            operation_labels.push(format!(
                "+{} more",
                operations.len() - operation_labels.len()
            ));
        }
        label = format!("{} {}", dep.relation, operation_labels.join(", "));
    } else if let Some(contract) = contract.or_else(|| dep.contracts.first().map(String::as_str)) {
        label = format!("{} {contract}", dep.relation);
    }
    format!("{label} ({})", dep.weight)
}

fn escape_likec4_string(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            _ => escaped.push(ch),
        }
    }
    escaped
}

fn sanitize_identifier(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len().max(4));
    let mut last_was_underscore = false;
    for ch in raw.trim().chars() {
        let next = if ch.is_ascii_alphanumeric() || ch == '_' {
            ch
        } else {
            '_'
        };
        if next == '_' {
            if !last_was_underscore {
                out.push(next);
            }
            last_was_underscore = true;
        } else {
            out.push(next);
            last_was_underscore = false;
        }
    }
    while out.ends_with('_') {
        out.pop();
    }
    if out.is_empty() {
        out.push_str("node");
    }
    if out.starts_with(|ch: char| ch.is_ascii_digit()) {
        out.insert(0, '_');
    }
    out
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use graphify_core::confidence::Confidence;
    use graphify_core::graph::KnowledgeGraph;
    use graphify_core::model::{GraphEdge, GraphNode, NodeType};

    use super::*;

    #[test]
    fn manifest_groups_files_by_package_and_folds_edges() {
        let mut graph = KnowledgeGraph::new();
        for (id, label, source_file, node_type) in [
            (
                "internal/api/public/handler.go",
                "handler",
                "internal/api/public/handler.go",
                NodeType::File,
            ),
            (
                "internal/domain/catalog/service.go",
                "service",
                "internal/domain/catalog/service.go",
                NodeType::File,
            ),
            (
                "handler_fn",
                "Handle()",
                "internal/api/public/handler.go",
                NodeType::Function,
            ),
            (
                "domain_type",
                "CatalogItem",
                "internal/domain/catalog/service.go",
                NodeType::Struct,
            ),
        ] {
            graph
                .add_node(GraphNode {
                    id: id.into(),
                    label: label.into(),
                    source_file: source_file.into(),
                    source_location: None,
                    node_type,
                    community: None,
                    extra: HashMap::new(),
                })
                .unwrap();
        }
        for (source, target, relation, source_file) in [
            (
                "internal/api/public/handler.go",
                "handler_fn",
                "defines",
                "internal/api/public/handler.go",
            ),
            (
                "internal/domain/catalog/service.go",
                "domain_type",
                "defines",
                "internal/domain/catalog/service.go",
            ),
            (
                "handler_fn",
                "domain_type",
                "uses",
                "internal/api/public/handler.go",
            ),
        ] {
            graph
                .add_edge(GraphEdge {
                    source: source.into(),
                    target: target.into(),
                    relation: relation.into(),
                    confidence: Confidence::Extracted,
                    confidence_score: 1.0,
                    source_file: source_file.into(),
                    source_location: None,
                    weight: 1.0,
                    extra: HashMap::new(),
                })
                .unwrap();
        }

        let manifest = build_architecture_manifest(
            &graph,
            &ArchitectureOptions {
                service_name: Some("consumer".into()),
                ..ArchitectureOptions::default()
            },
        );

        assert_eq!(manifest.service.name, "consumer");
        assert!(
            manifest
                .packages
                .iter()
                .any(|p| p.path == "internal/api/public")
        );
        assert!(
            manifest
                .packages
                .iter()
                .any(|p| p.path == "internal/domain/catalog")
        );
        assert_eq!(manifest.dependencies.len(), 1);
        let dep = &manifest.dependencies[0];
        assert_eq!(dep.source, "internal/api/public");
        assert_eq!(dep.target, "internal/domain/catalog");
        assert_eq!(dep.relation, "uses");
        assert_eq!(dep.weight, 1);
        assert_eq!(dep.sample_edges.len(), 1);
    }

    #[test]
    fn manifest_reads_module_prefix_and_collects_external_imports() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("go.mod"),
            "module example.com/services/consumer\n",
        )
        .unwrap();

        let mut graph = KnowledgeGraph::new();
        graph
            .add_node(GraphNode {
                id: "internal/app/app.go".into(),
                label: "app".into(),
                source_file: dir
                    .path()
                    .join("internal/app/app.go")
                    .to_string_lossy()
                    .to_string(),
                source_location: None,
                node_type: NodeType::File,
                community: None,
                extra: HashMap::new(),
            })
            .unwrap();
        graph
            .add_node(GraphNode {
                id: "import_provider".into(),
                label: "example.com/services/provider/proto".into(),
                source_file: dir
                    .path()
                    .join("internal/app/app.go")
                    .to_string_lossy()
                    .to_string(),
                source_location: Some("import".into()),
                node_type: NodeType::Package,
                community: None,
                extra: HashMap::new(),
            })
            .unwrap();
        graph
            .add_edge(GraphEdge {
                source: "internal/app/app.go".into(),
                target: "import_provider".into(),
                relation: "imports".into(),
                confidence: Confidence::Extracted,
                confidence_score: 1.0,
                source_file: dir
                    .path()
                    .join("internal/app/app.go")
                    .to_string_lossy()
                    .to_string(),
                source_location: Some("import".into()),
                weight: 1.0,
                extra: HashMap::new(),
            })
            .unwrap();

        let manifest = build_architecture_manifest(
            &graph,
            &ArchitectureOptions {
                service_name: Some("consumer".into()),
                root_path: Some(dir.path().to_path_buf()),
                ..ArchitectureOptions::default()
            },
        );

        assert_eq!(
            manifest.service.module_prefixes,
            vec!["example.com/services/consumer".to_string()]
        );
        assert!(
            manifest
                .external_dependencies
                .iter()
                .any(|dep| dep.name == "example.com/services/provider/proto")
        );
        assert!(
            manifest
                .consumed
                .protos
                .contains(&"example.com/services/provider/proto".to_string())
        );
    }

    #[test]
    fn manifest_ignores_docs_and_scratch_files() {
        let mut graph = KnowledgeGraph::new();
        for (id, source_file, node_type) in [
            ("readme", "README.md", NodeType::File),
            ("doc_sql", "docs/example.sql", NodeType::File),
            ("scratch_go", "scratch/main.go", NodeType::File),
            ("mr", "MR_DESCRIPTION.md", NodeType::File),
            ("api", "internal/api/handler.go", NodeType::File),
        ] {
            graph
                .add_node(GraphNode {
                    id: id.into(),
                    label: id.into(),
                    source_file: source_file.into(),
                    source_location: None,
                    node_type,
                    community: None,
                    extra: HashMap::new(),
                })
                .unwrap();
        }

        let manifest = build_architecture_manifest(
            &graph,
            &ArchitectureOptions {
                service_name: Some("service".into()),
                ..ArchitectureOptions::default()
            },
        );

        assert_eq!(manifest.packages.len(), 1);
        assert_eq!(manifest.packages[0].path, "internal/api");
    }

    #[test]
    fn manifest_does_not_leak_absolute_root_path() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("internal/service/handler.go");
        std::fs::create_dir_all(src.parent().unwrap()).unwrap();
        std::fs::write(&src, "package service\n").unwrap();

        let mut graph = KnowledgeGraph::new();
        graph
            .add_node(GraphNode {
                id: "handler".into(),
                label: "handler".into(),
                source_file: src.to_string_lossy().to_string(),
                source_location: None,
                node_type: NodeType::Function,
                community: None,
                extra: HashMap::new(),
            })
            .unwrap();

        let manifest = build_architecture_manifest(
            &graph,
            &ArchitectureOptions {
                service_name: Some("svc".into()),
                root_path: Some(dir.path().to_path_buf()),
                ..ArchitectureOptions::default()
            },
        );
        let json = serde_json::to_string(&manifest).unwrap();

        assert!(manifest.service.root.is_none());
        assert!(!json.contains(&dir.path().to_string_lossy().to_string()));
        assert_eq!(
            manifest.packages[0].source_files,
            vec!["internal/service/handler.go"]
        );
    }

    #[test]
    fn manifest_extracts_business_capabilities_from_contracts_and_packages() {
        let dir = tempfile::tempdir().unwrap();
        let proto = dir.path().join("proto/services/catalog/catalog.proto");
        std::fs::create_dir_all(proto.parent().unwrap()).unwrap();
        std::fs::write(
            &proto,
            r#"
syntax = "proto3";
package example.catalog;
service Catalog {
  rpc Save(SaveRequest) returns (SaveResponse);
  rpc List(ListRequest) returns (ListResponse);
}
"#,
        )
        .unwrap();

        let mut graph = KnowledgeGraph::new();
        for (id, source_file) in [
            ("proto", "proto/services/catalog/catalog.proto"),
            ("handler", "internal/server/v2/handler/catalog/list.go"),
            ("service", "internal/service/catalog/create.go"),
            ("domain", "internal/domain/catalog/changer.go"),
            ("repo", "internal/repository/catalog/store.go"),
        ] {
            graph
                .add_node(GraphNode {
                    id: id.into(),
                    label: id.into(),
                    source_file: dir.path().join(source_file).to_string_lossy().to_string(),
                    source_location: None,
                    node_type: NodeType::File,
                    community: None,
                    extra: HashMap::new(),
                })
                .unwrap();
        }

        let manifest = build_architecture_manifest(
            &graph,
            &ArchitectureOptions {
                service_name: Some("service".into()),
                root_path: Some(dir.path().to_path_buf()),
                ..ArchitectureOptions::default()
            },
        );

        let capability = manifest
            .capabilities
            .iter()
            .find(|capability| capability.id == "catalog")
            .expect("catalog capability");
        assert_eq!(capability.name, "Catalog");
        assert!(
            capability
                .packages
                .contains(&"internal/domain/catalog".to_string())
        );
        assert!(
            capability
                .packages
                .contains(&"internal/service/catalog".to_string())
        );
        assert!(
            capability
                .provided_apis
                .contains(&"Catalog.Save".to_string())
        );
        assert!(
            capability
                .provided_apis
                .contains(&"Catalog.List".to_string())
        );
        assert!(
            capability.summary.contains("Exposes 2 RPCs"),
            "{}",
            capability.summary
        );
    }

    #[test]
    fn manifest_infers_capability_flows_from_package_dependencies() {
        let mut graph = KnowledgeGraph::new();
        for (id, label, source_file, node_type) in [
            (
                "api_file",
                "handler",
                "internal/api/public/handler.go",
                NodeType::File,
            ),
            (
                "domain_file",
                "service",
                "internal/domain/catalog/service.go",
                NodeType::File,
            ),
            (
                "api_handler",
                "ListCatalog()",
                "internal/api/public/handler.go",
                NodeType::Function,
            ),
            (
                "catalog_service",
                "ListCatalog()",
                "internal/domain/catalog/service.go",
                NodeType::Function,
            ),
        ] {
            graph
                .add_node(GraphNode {
                    id: id.into(),
                    label: label.into(),
                    source_file: source_file.into(),
                    source_location: None,
                    node_type,
                    community: None,
                    extra: HashMap::new(),
                })
                .unwrap();
        }
        graph
            .add_edge(GraphEdge {
                source: "api_handler".into(),
                target: "catalog_service".into(),
                relation: "calls".into(),
                confidence: Confidence::Extracted,
                confidence_score: 1.0,
                source_file: "internal/api/public/handler.go".into(),
                source_location: None,
                weight: 1.0,
                extra: HashMap::new(),
            })
            .unwrap();

        let manifest = build_architecture_manifest(
            &graph,
            &ArchitectureOptions {
                service_name: Some("service".into()),
                ..ArchitectureOptions::default()
            },
        );

        assert!(
            manifest.flows.iter().any(|flow| {
                flow.source == "public" && flow.target == "catalog" && flow.relation == "calls"
            }),
            "{:#?}",
            manifest.flows
        );
    }

    #[test]
    fn manifest_capabilities_ignore_test_and_benchmark_operations() {
        let mut graph = KnowledgeGraph::new();
        for (id, label, source_file, node_type) in [
            (
                "prod",
                "SaveRecord()",
                "internal/domain/catalog/service.go",
                NodeType::Function,
            ),
            (
                "test",
                "TestSaveRecord()",
                "internal/domain/catalog/service_test.go",
                NodeType::Function,
            ),
            (
                "bench",
                "BenchmarkSaveRecord()",
                "internal/domain/catalog/service_test.go",
                NodeType::Function,
            ),
            (
                "debug",
                "Debugf()",
                "internal/domain/catalog/service.go",
                NodeType::Function,
            ),
            (
                "constructor",
                "NewCatalog()",
                "internal/domain/catalog/service.go",
                NodeType::Function,
            ),
            (
                "option",
                "WithClock()",
                "internal/domain/catalog/service.go",
                NodeType::Function,
            ),
            (
                "logger",
                "Errorf()",
                "internal/domain/catalog/service.go",
                NodeType::Function,
            ),
            (
                "test_data",
                "AddCatalogTestData()",
                "internal/domain/catalog/service.go",
                NodeType::Function,
            ),
        ] {
            graph
                .add_node(GraphNode {
                    id: id.into(),
                    label: label.into(),
                    source_file: source_file.into(),
                    source_location: None,
                    node_type,
                    community: None,
                    extra: HashMap::new(),
                })
                .unwrap();
        }

        let manifest = build_architecture_manifest(
            &graph,
            &ArchitectureOptions {
                service_name: Some("service".into()),
                ..ArchitectureOptions::default()
            },
        );

        let capability = manifest
            .capabilities
            .iter()
            .find(|capability| capability.id == "catalog")
            .expect("catalog capability");
        assert!(
            capability
                .evidence
                .iter()
                .any(|evidence| evidence.starts_with("operation:SaveRecord@"))
        );
        assert!(
            capability
                .evidence
                .iter()
                .all(|evidence| !evidence.contains("TestSaveRecord")
                    && !evidence.contains("BenchmarkSaveRecord")
                    && !evidence.contains("Debugf")
                    && !evidence.contains("Errorf")
                    && !evidence.contains("NewCatalog")
                    && !evidence.contains("AddCatalogTestData")
                    && !evidence.contains("WithClock"))
        );
        assert!(
            capability
                .source_files
                .iter()
                .all(|path| !path.ends_with("_test.go"))
        );
    }

    #[test]
    fn manifest_extracts_all_proto_services_from_one_file() {
        let dir = tempfile::tempdir().unwrap();
        let proto = dir.path().join("proto/services/reporting.proto");
        std::fs::create_dir_all(proto.parent().unwrap()).unwrap();
        std::fs::write(
            &proto,
            r#"
syntax = "proto3";
service Reports {
  rpc Download(DownloadRequest) returns (DownloadResponse);
}
service AsyncRequests {
  rpc List(ListRequest) returns (ListResponse);
}
"#,
        )
        .unwrap();

        let graph = KnowledgeGraph::new();
        let manifest = build_architecture_manifest(
            &graph,
            &ArchitectureOptions {
                service_name: Some("service".into()),
                root_path: Some(dir.path().to_path_buf()),
                ..ArchitectureOptions::default()
            },
        );

        let provided = manifest
            .capabilities
            .iter()
            .flat_map(|capability| capability.provided_apis.iter())
            .cloned()
            .collect::<BTreeSet<_>>();
        assert!(provided.contains("Reports.Download"));
        assert!(provided.contains("AsyncRequests.List"));
    }

    #[test]
    fn compose_links_services_by_module_prefix_imports() {
        let consumer = ArchitectureManifest {
            service: ArchitectureService {
                name: "consumer".into(),
                root: None,
                module_prefixes: vec!["example.com/services/consumer".into()],
            },
            external_dependencies: vec![ArchitectureExternalDependency {
                name: "example.com/services/provider/proto".into(),
                kind: "proto".into(),
                weight: 3,
                imported_by: vec!["internal/app".into()],
            }],
            ..ArchitectureManifest::new("consumer")
        };
        let provider = ArchitectureManifest {
            service: ArchitectureService {
                name: "provider".into(),
                root: None,
                module_prefixes: vec!["example.com/services/provider".into()],
            },
            provided: ArchitectureInterfaces {
                protos: vec!["example.com/services/provider/proto".into()],
                ..ArchitectureInterfaces::default()
            },
            ..ArchitectureManifest::new("provider")
        };

        let landscape = compose_architecture_manifests(&[consumer, provider]);

        assert_eq!(landscape.dependencies.len(), 1);
        assert_eq!(landscape.dependencies[0].source, "consumer");
        assert_eq!(landscape.dependencies[0].target, "provider");
        assert!(
            landscape.dependencies[0]
                .evidence
                .iter()
                .any(|evidence| evidence.contains("module_prefix"))
        );
    }

    #[test]
    fn compose_links_services_by_shared_proto_contract_tokens() {
        let consumer = ArchitectureManifest {
            service: ArchitectureService {
                name: "consumer".into(),
                root: None,
                module_prefixes: vec!["example.com/services/consumer".into()],
            },
            external_dependencies: vec![ArchitectureExternalDependency {
                name: "example.com/shared/proto/catalog/v1".into(),
                kind: "proto".into(),
                weight: 2,
                imported_by: vec!["internal/app".into()],
            }],
            ..ArchitectureManifest::new("consumer")
        };
        let provider = ArchitectureManifest {
            service: ArchitectureService {
                name: "catalog-service".into(),
                root: None,
                module_prefixes: Vec::new(),
            },
            capabilities: vec![ArchitectureCapability {
                id: "catalog".into(),
                name: "Catalog".into(),
                kind: "contract".into(),
                summary: "Exposes 1 RPCs".into(),
                packages: Vec::new(),
                source_files: vec!["proto/services/catalog.proto".into()],
                provided_apis: vec!["Catalog.List".into()],
                consumed_apis: Vec::new(),
                events: Vec::new(),
                operations: Vec::new(),
                jobs: Vec::new(),
                owns_data: Vec::new(),
                evidence: Vec::new(),
                node_count: 0,
            }],
            ..ArchitectureManifest::new("catalog-service")
        };

        let landscape = compose_architecture_manifests(&[consumer, provider]);

        assert!(
            landscape.dependencies.iter().any(|dep| {
                dep.source == "consumer"
                    && dep.target == "catalog-service"
                    && dep.evidence.iter().any(|e| e.contains("contract_token"))
            }),
            "{:#?}",
            landscape.dependencies
        );
    }

    #[test]
    fn manifest_extracts_contract_registry_from_proto_files() {
        let dir = tempfile::tempdir().expect("tempdir");
        let proto_dir = dir.path().join("proto/catalog/v1");
        std::fs::create_dir_all(&proto_dir).expect("proto dir");
        std::fs::write(
            proto_dir.join("catalog.proto"),
            r#"
                syntax = "proto3";
                package catalog.v1;

                service CatalogService {
                  rpc ListItems (ListItemsRequest) returns (ListItemsResponse);
                }
            "#,
        )
        .expect("write proto");

        let graph = KnowledgeGraph::new();
        let manifest = build_architecture_manifest(
            &graph,
            &ArchitectureOptions {
                service_name: Some("provider".into()),
                root_path: Some(dir.path().to_path_buf()),
                ..ArchitectureOptions::default()
            },
        );

        assert!(
            manifest.contracts.provided.iter().any(|contract| contract
                .aliases
                .iter()
                .any(|alias| alias == "proto:catalog:v1")
                && contract
                    .operations
                    .iter()
                    .any(|op| op == "CatalogService.ListItems")),
            "{:#?}",
            manifest.contracts
        );
    }

    #[test]
    fn compose_links_services_by_exact_contract_aliases() {
        let consumer = ArchitectureManifest {
            service: ArchitectureService {
                name: "consumer".into(),
                root: None,
                module_prefixes: Vec::new(),
            },
            contracts: ArchitectureContracts {
                consumed: vec![ArchitectureContract {
                    id: "proto:catalog:v1".into(),
                    name: "catalog/v1".into(),
                    kind: "proto".into(),
                    source_kind: "proto".into(),
                    source: "example.com/shared/proto/catalog/v1".into(),
                    capability: None,
                    operations: Vec::new(),
                    aliases: vec!["proto:catalog:v1".into()],
                    evidence: vec!["external:proto:example.com/shared/proto/catalog/v1".into()],
                }],
                unresolved: vec![ArchitectureContract {
                    id: "proto:catalog:v1".into(),
                    name: "catalog/v1".into(),
                    kind: "proto".into(),
                    source_kind: "proto".into(),
                    source: "example.com/shared/proto/catalog/v1".into(),
                    capability: None,
                    operations: Vec::new(),
                    aliases: vec!["proto:catalog:v1".into()],
                    evidence: vec!["external:proto:example.com/shared/proto/catalog/v1".into()],
                }],
                ..ArchitectureContracts::default()
            },
            ..ArchitectureManifest::new("consumer")
        };
        let provider = ArchitectureManifest {
            service: ArchitectureService {
                name: "provider".into(),
                root: None,
                module_prefixes: Vec::new(),
            },
            contracts: ArchitectureContracts {
                provided: vec![
                    ArchitectureContract {
                        id: "proto:catalog:v1:catalogservice".into(),
                        name: "CatalogService".into(),
                        kind: "proto".into(),
                        source_kind: "proto".into(),
                        source: "proto/catalog/v1/catalog.proto".into(),
                        capability: Some("catalog".into()),
                        operations: vec!["CatalogService.ListItems".into()],
                        aliases: vec![
                            "proto:catalog:v1:catalogservice".into(),
                            "proto:catalog:v1".into(),
                        ],
                        evidence: vec!["proto:proto/catalog/v1/catalog.proto".into()],
                    },
                    ArchitectureContract {
                        id: "proto:unused:v1:unusedservice".into(),
                        name: "UnusedService".into(),
                        kind: "proto".into(),
                        source_kind: "proto".into(),
                        source: "proto/unused/v1/unused.proto".into(),
                        capability: Some("unused".into()),
                        operations: vec!["UnusedService.Get".into()],
                        aliases: vec!["proto:unused:v1".into()],
                        evidence: vec!["proto:proto/unused/v1/unused.proto".into()],
                    },
                ],
                ..ArchitectureContracts::default()
            },
            packages: vec![ArchitecturePackage {
                id: "internal/app".into(),
                label: "internal/app".into(),
                path: "internal/app".into(),
                kind: "domain".into(),
                source_files: vec!["internal/app/service.go".into()],
                node_count: 1,
                community: None,
            }],
            ..ArchitectureManifest::new("provider")
        };

        let landscape = compose_architecture_manifests(&[consumer, provider]);

        let dep = landscape
            .dependencies
            .iter()
            .find(|dep| dep.source == "consumer" && dep.target == "provider")
            .expect("consumer depends on provider");
        assert!(
            dep.evidence
                .iter()
                .any(|e| e.contains("contract_id:proto:catalog:v1")),
            "{:#?}",
            landscape.dependencies
        );
        assert!(
            dep.contracts
                .iter()
                .any(|contract| contract == "proto:catalog:v1:catalogservice"),
            "{dep:#?}"
        );
        assert!(
            dep.operations
                .iter()
                .any(|operation| operation == "CatalogService.ListItems"),
            "{dep:#?}"
        );
        assert!(
            landscape.contracts.iter().any(|contract| {
                contract.service == "provider"
                    && contract.id == "proto:catalog:v1:catalogservice"
                    && contract
                        .operations
                        .iter()
                        .any(|operation| operation == "CatalogService.ListItems")
            }),
            "{:#?}",
            landscape.contracts
        );
        assert!(
            landscape
                .contracts
                .iter()
                .any(|contract| contract.id == "proto:unused:v1:unusedservice"),
            "{:#?}",
            landscape.contracts
        );

        let spec = landscape_specification(&landscape);
        let model = landscape_model(&landscape);
        assert!(spec.contains("element api"));
        assert!(spec.contains("element operation"));
        assert!(model.contains("= api \"CatalogService\""));
        assert!(model.contains("= operation \"CatalogService.ListItems\""));
        assert!(
            model.contains(
                "svc_consumer -[calls]-> svc_provider.api_proto_catalog_v1_catalogservice"
            )
        );
        assert!(model.contains("calls CatalogService.ListItems"));
        assert!(!model.contains("= api \"UnusedService\""));
    }

    #[test]
    fn service_workspace_emits_api_and_external_api_elements() {
        let manifest = ArchitectureManifest {
            service: ArchitectureService {
                name: "consumer".into(),
                root: None,
                module_prefixes: Vec::new(),
            },
            capabilities: vec![ArchitectureCapability {
                id: "catalog".into(),
                name: "Catalog".into(),
                kind: "business".into(),
                summary: "Catalog behavior".into(),
                packages: Vec::new(),
                source_files: Vec::new(),
                provided_apis: Vec::new(),
                consumed_apis: vec!["example.com/shared/proto/catalog/v1".into()],
                events: Vec::new(),
                operations: Vec::new(),
                jobs: Vec::new(),
                owns_data: Vec::new(),
                evidence: Vec::new(),
                node_count: 1,
            }],
            contracts: ArchitectureContracts {
                provided: vec![ArchitectureContract {
                    id: "proto:consumer:v1:consumerservice".into(),
                    name: "ConsumerService".into(),
                    kind: "proto".into(),
                    source_kind: "proto".into(),
                    source: "proto/consumer/v1/consumer.proto".into(),
                    capability: Some("catalog".into()),
                    operations: vec!["ConsumerService.Get".into()],
                    aliases: vec!["proto:consumer:v1".into()],
                    evidence: Vec::new(),
                }],
                consumed: vec![ArchitectureContract {
                    id: "proto:catalog:v1".into(),
                    name: "catalog/v1".into(),
                    kind: "proto".into(),
                    source_kind: "proto".into(),
                    source: "example.com/shared/proto/catalog/v1".into(),
                    capability: Some("catalog".into()),
                    operations: Vec::new(),
                    aliases: vec!["proto:catalog:v1".into()],
                    evidence: Vec::new(),
                }],
                unresolved: vec![ArchitectureContract {
                    id: "proto:catalog:v1".into(),
                    name: "catalog/v1".into(),
                    kind: "proto".into(),
                    source_kind: "proto".into(),
                    source: "example.com/shared/proto/catalog/v1".into(),
                    capability: Some("catalog".into()),
                    operations: Vec::new(),
                    aliases: vec!["proto:catalog:v1".into()],
                    evidence: Vec::new(),
                }],
            },
            ..ArchitectureManifest::new("consumer")
        };

        let spec = service_specification(&manifest);
        let model = service_model(&manifest);

        assert!(spec.contains("element api"));
        assert!(spec.contains("element externalApi"));
        assert!(spec.contains("element operation"));
        assert!(model.contains("= api "));
        assert!(model.contains("= operation "));
        assert!(model.contains("= externalApi "));
        assert!(model.contains("-[implements]->") || model.contains("= api "));
        assert!(model.contains("-[consumes]->"));

        let views = service_views(&manifest);
        assert!(views.contains("view service of consumer"));
        assert!(views.contains("view contracts of consumer"));
        assert!(views.contains("include element.kind = api"));
        assert!(views.contains("include element.kind = operation"));
    }

    #[test]
    fn service_model_nests_contracts_under_capabilities_and_shows_module_implementations() {
        let manifest = ArchitectureManifest {
            service: ArchitectureService {
                name: "example_service".into(),
                root: None,
                module_prefixes: Vec::new(),
            },
            capabilities: vec![
                ArchitectureCapability {
                    id: "public_api".into(),
                    name: "Public API".into(),
                    kind: "api".into(),
                    summary: "Public handlers".into(),
                    packages: vec!["internal/api/publicapi".into()],
                    source_files: Vec::new(),
                    provided_apis: Vec::new(),
                    consumed_apis: Vec::new(),
                    events: Vec::new(),
                    operations: vec!["ListWidgets".into()],
                    jobs: Vec::new(),
                    owns_data: Vec::new(),
                    evidence: vec![
                        "operation:ListWidgets@internal/api/publicapi/public_api.go".into(),
                    ],
                    node_count: 10,
                },
                ArchitectureCapability {
                    id: "widget".into(),
                    name: "Widget".into(),
                    kind: "contract".into(),
                    summary: "Widget contract".into(),
                    packages: Vec::new(),
                    source_files: Vec::new(),
                    provided_apis: Vec::new(),
                    consumed_apis: Vec::new(),
                    events: Vec::new(),
                    operations: Vec::new(),
                    jobs: Vec::new(),
                    owns_data: Vec::new(),
                    evidence: Vec::new(),
                    node_count: 0,
                },
                ArchitectureCapability {
                    id: "private_api".into(),
                    name: "Private API".into(),
                    kind: "api".into(),
                    summary: "Private handlers".into(),
                    packages: vec!["internal/api/privateapi".into()],
                    source_files: Vec::new(),
                    provided_apis: Vec::new(),
                    consumed_apis: Vec::new(),
                    events: Vec::new(),
                    operations: vec!["CreateSession".into()],
                    jobs: Vec::new(),
                    owns_data: Vec::new(),
                    evidence: vec![
                        "operation:CreateSession@internal/api/privateapi/sessions.go".into(),
                    ],
                    node_count: 3,
                },
            ],
            packages: vec![
                ArchitecturePackage {
                    id: "internal/api/publicapi".into(),
                    label: "internal/api/publicapi".into(),
                    path: "internal/api/publicapi".into(),
                    kind: "api".into(),
                    source_files: vec!["internal/api/publicapi/public_api.go".into()],
                    node_count: 10,
                    community: None,
                },
                ArchitecturePackage {
                    id: "internal/api/privateapi".into(),
                    label: "internal/api/privateapi".into(),
                    path: "internal/api/privateapi".into(),
                    kind: "api".into(),
                    source_files: vec!["internal/api/privateapi/sessions.go".into()],
                    node_count: 3,
                    community: None,
                },
                ArchitecturePackage {
                    id: "internal/proto/example/publicapi/publicapiconnect".into(),
                    label: "internal/proto/example/publicapi/publicapiconnect".into(),
                    path: "internal/proto/example/publicapi/publicapiconnect".into(),
                    kind: "proto".into(),
                    source_files: vec![
                        "internal/proto/example/publicapi/publicapi.connect.go".into(),
                    ],
                    node_count: 2,
                    community: None,
                },
                ArchitecturePackage {
                    id: "internal/proto/example/publicapi/v2/publicapiv2connect".into(),
                    label: "internal/proto/example/publicapi/v2/publicapiv2connect".into(),
                    path: "internal/proto/example/publicapi/v2/publicapiv2connect".into(),
                    kind: "proto".into(),
                    source_files: vec![
                        "internal/proto/example/publicapi/v2/publicapi.connect.go".into(),
                    ],
                    node_count: 2,
                    community: None,
                },
                ArchitecturePackage {
                    id: "internal/proto/example/widget/widgetconnect".into(),
                    label: "internal/proto/example/widget/widgetconnect".into(),
                    path: "internal/proto/example/widget/widgetconnect".into(),
                    kind: "proto".into(),
                    source_files: vec!["internal/proto/example/widget/widget.connect.go".into()],
                    node_count: 2,
                    community: None,
                },
            ],
            dependencies: vec![
                ArchitectureDependency {
                    source: "internal/api/publicapi".into(),
                    target: "internal/proto/example/publicapi/publicapiconnect".into(),
                    relation: "uses".into(),
                    weight: 3,
                    confidence: 0.8,
                    sample_edges: Vec::new(),
                },
                ArchitectureDependency {
                    source: "internal/api/publicapi".into(),
                    target: "internal/proto/example/publicapi/v2/publicapiv2connect".into(),
                    relation: "uses".into(),
                    weight: 3,
                    confidence: 0.8,
                    sample_edges: Vec::new(),
                },
                ArchitectureDependency {
                    source: "internal/api/publicapi".into(),
                    target: "internal/proto/example/widget/widgetconnect".into(),
                    relation: "uses".into(),
                    weight: 2,
                    confidence: 0.8,
                    sample_edges: Vec::new(),
                },
            ],
            contracts: ArchitectureContracts {
                provided: vec![
                    ArchitectureContract {
                        id: "proto:example:publicapi:publicapiservice".into(),
                        name: "PublicAPIService".into(),
                        kind: "proto".into(),
                        source_kind: "proto".into(),
                        source: "proto/example/publicapi/publicapi.proto".into(),
                        capability: Some("public_api".into()),
                        operations: vec!["PublicAPIService.ListWidgets".into()],
                        aliases: Vec::new(),
                        evidence: Vec::new(),
                    },
                    ArchitectureContract {
                        id: "proto:example:publicapi:v2:publicapiservice".into(),
                        name: "PublicAPIService".into(),
                        kind: "proto".into(),
                        source_kind: "proto".into(),
                        source: "proto/example/publicapi/v2/publicapi.proto".into(),
                        capability: Some("public_api".into()),
                        operations: vec!["PublicAPIService.ListWidgets".into()],
                        aliases: Vec::new(),
                        evidence: Vec::new(),
                    },
                    ArchitectureContract {
                        id: "proto:example:widget:widgetservice".into(),
                        name: "WidgetService".into(),
                        kind: "proto".into(),
                        source_kind: "proto".into(),
                        source: "proto/example/widget/widget.proto".into(),
                        capability: Some("widget".into()),
                        operations: vec!["WidgetService.GetWidget".into()],
                        aliases: Vec::new(),
                        evidence: Vec::new(),
                    },
                ],
                ..ArchitectureContracts::default()
            },
            ..ArchitectureManifest::new("example_service")
        };

        let model = service_model(&manifest);
        assert!(model.contains("cap_widget = container \"Widget\""));
        assert!(model.contains("api_proto_example_widget_widgetservice = api \"WidgetService\""));
        assert!(model.contains("cap_public_api.mod_internal_api_publicapi -[implements]-> cap_widget.api_proto_example_widget_widgetservice"));
        assert!(model.contains("api \"PublicAPIService (publicapi)\""));
        assert!(model.contains("api \"PublicAPIService (publicapi/v2)\""));
        assert!(
            model.contains("mod_internal_api_privateapi = component \"internal/api/privateapi\"")
        );
        assert!(model.contains("op_CreateSession = operation \"CreateSession\""));

        let views = service_views(&manifest);
        assert!(views.contains("view modules of example_service"));
    }

    #[test]
    fn landscape_renders_multi_contract_dependencies_as_separate_api_edges() {
        let consumer = ArchitectureManifest {
            service: ArchitectureService {
                name: "consumer".into(),
                root: None,
                module_prefixes: Vec::new(),
            },
            contracts: ArchitectureContracts {
                consumed: vec![
                    ArchitectureContract {
                        id: "proto:alpha:v1".into(),
                        name: "alpha/v1".into(),
                        kind: "proto".into(),
                        source_kind: "proto".into(),
                        source: "example.com/shared/proto/alpha/v1".into(),
                        capability: None,
                        operations: Vec::new(),
                        aliases: vec!["proto:alpha:v1".into()],
                        evidence: Vec::new(),
                    },
                    ArchitectureContract {
                        id: "proto:beta:v1".into(),
                        name: "beta/v1".into(),
                        kind: "proto".into(),
                        source_kind: "proto".into(),
                        source: "example.com/shared/proto/beta/v1".into(),
                        capability: None,
                        operations: Vec::new(),
                        aliases: vec!["proto:beta:v1".into()],
                        evidence: Vec::new(),
                    },
                ],
                ..ArchitectureContracts::default()
            },
            ..ArchitectureManifest::new("consumer")
        };
        let provider = ArchitectureManifest {
            service: ArchitectureService {
                name: "provider".into(),
                root: None,
                module_prefixes: Vec::new(),
            },
            contracts: ArchitectureContracts {
                provided: vec![
                    ArchitectureContract {
                        id: "proto:alpha:v1:alphaservice".into(),
                        name: "AlphaService".into(),
                        kind: "proto".into(),
                        source_kind: "proto".into(),
                        source: "proto/alpha/v1/alpha.proto".into(),
                        capability: Some("alpha".into()),
                        operations: vec!["AlphaService.Get".into()],
                        aliases: vec![
                            "proto:alpha:v1".into(),
                            "proto:alpha:v1:alphaservice".into(),
                        ],
                        evidence: Vec::new(),
                    },
                    ArchitectureContract {
                        id: "proto:beta:v1:betaservice".into(),
                        name: "BetaService".into(),
                        kind: "proto".into(),
                        source_kind: "proto".into(),
                        source: "proto/beta/v1/beta.proto".into(),
                        capability: Some("beta".into()),
                        operations: vec!["BetaService.List".into()],
                        aliases: vec!["proto:beta:v1".into(), "proto:beta:v1:betaservice".into()],
                        evidence: Vec::new(),
                    },
                ],
                ..ArchitectureContracts::default()
            },
            packages: vec![ArchitecturePackage {
                id: "internal/app".into(),
                label: "internal/app".into(),
                path: "internal/app".into(),
                kind: "domain".into(),
                source_files: vec!["internal/app/service.go".into()],
                node_count: 1,
                community: None,
            }],
            ..ArchitectureManifest::new("provider")
        };

        let landscape = compose_architecture_manifests(&[consumer, provider]);
        let model = landscape_model(&landscape);

        assert!(model.contains(
            "svc_consumer -[calls]-> svc_provider.api_proto_alpha_v1_alphaservice \"calls AlphaService.Get"
        ));
        assert!(model.contains(
            "svc_consumer -[calls]-> svc_provider.api_proto_beta_v1_betaservice \"calls BetaService.List"
        ));
        assert!(!model.contains(
            "svc_consumer -[calls]-> svc_provider.api_proto_alpha_v1_alphaservice \"calls AlphaService.Get, BetaService.List"
        ));
    }

    #[test]
    fn compose_links_source_service_to_target_contract_named_for_source() {
        let consumer = ArchitectureManifest {
            service: ArchitectureService {
                name: "consumer".into(),
                root: None,
                module_prefixes: vec!["example.com/services/consumer".into()],
            },
            ..ArchitectureManifest::new("consumer")
        };
        let provider = ArchitectureManifest {
            service: ArchitectureService {
                name: "catalog".into(),
                root: None,
                module_prefixes: vec!["example.com/services/catalog".into()],
            },
            capabilities: vec![ArchitectureCapability {
                id: "consumer".into(),
                name: "Consumer API".into(),
                kind: "contract".into(),
                summary: "Exposes 2 RPCs".into(),
                packages: Vec::new(),
                source_files: vec!["proto/services/catalog/consumer.proto".into()],
                provided_apis: vec!["Consumer.List".into(), "Consumer.Get".into()],
                consumed_apis: Vec::new(),
                events: Vec::new(),
                operations: Vec::new(),
                jobs: Vec::new(),
                owns_data: Vec::new(),
                evidence: Vec::new(),
                node_count: 0,
            }],
            ..ArchitectureManifest::new("catalog")
        };

        let landscape = compose_architecture_manifests(&[consumer, provider]);

        assert!(
            landscape.dependencies.iter().any(|dep| {
                dep.source == "consumer"
                    && dep.target == "catalog"
                    && dep
                        .evidence
                        .iter()
                        .any(|e| e.contains("target_contract_for_source"))
            }),
            "{:#?}",
            landscape.dependencies
        );
    }

    #[test]
    fn compose_does_not_link_by_generic_rpc_method_tokens() {
        let consumer = ArchitectureManifest {
            service: ArchitectureService {
                name: "consumer".into(),
                root: None,
                module_prefixes: Vec::new(),
            },
            external_dependencies: vec![ArchitectureExternalDependency {
                name: "example.com/shared/repository/itemquery".into(),
                kind: "module".into(),
                weight: 2,
                imported_by: vec!["internal/app".into()],
            }],
            ..ArchitectureManifest::new("consumer")
        };
        let provider = ArchitectureManifest {
            service: ArchitectureService {
                name: "provider".into(),
                root: None,
                module_prefixes: Vec::new(),
            },
            capabilities: vec![ArchitectureCapability {
                id: "external_contract".into(),
                name: "External Contract".into(),
                kind: "contract".into(),
                summary: "Exposes 1 RPCs".into(),
                packages: Vec::new(),
                source_files: vec!["proto/services/external.proto".into()],
                provided_apis: vec!["ExternalService.FetchItem".into()],
                consumed_apis: Vec::new(),
                events: Vec::new(),
                operations: Vec::new(),
                jobs: Vec::new(),
                owns_data: Vec::new(),
                evidence: Vec::new(),
                node_count: 0,
            }],
            ..ArchitectureManifest::new("provider")
        };

        let landscape = compose_architecture_manifests(&[consumer, provider]);

        assert!(
            landscape.dependencies.is_empty(),
            "{:#?}",
            landscape.dependencies
        );
    }

    #[test]
    fn compose_links_consumer_to_provider_by_service_named_api_packages() {
        let consumer = ArchitectureManifest {
            service: ArchitectureService {
                name: "consumer".into(),
                root: None,
                module_prefixes: vec!["example.com/services/consumer".into()],
            },
            packages: vec![ArchitecturePackage {
                id: "internal/domain/provider".into(),
                label: "internal/domain/provider".into(),
                path: "internal/domain/provider".into(),
                kind: "domain".into(),
                source_files: vec!["internal/domain/provider/manager.go".into()],
                node_count: 3,
                community: None,
            }],
            ..ArchitectureManifest::new("consumer")
        };
        let provider = ArchitectureManifest {
            service: ArchitectureService {
                name: "provider".into(),
                root: None,
                module_prefixes: vec!["example.com/services/provider".into()],
            },
            packages: vec![ArchitecturePackage {
                id: "internal/api".into(),
                label: "internal/api".into(),
                path: "internal/api".into(),
                kind: "api".into(),
                source_files: vec!["internal/api/consumer.go".into()],
                node_count: 2,
                community: None,
            }],
            ..ArchitectureManifest::new("provider")
        };

        let landscape = compose_architecture_manifests(&[consumer, provider]);

        assert!(
            landscape
                .dependencies
                .iter()
                .any(|dep| dep.source == "consumer" && dep.target == "provider")
        );
        assert!(
            !landscape
                .dependencies
                .iter()
                .any(|dep| dep.source == "provider" && dep.target == "consumer")
        );
        let model = landscape_model(&landscape);
        assert!(
            !model.contains("svc_consumer -[calls]-> svc_provider"),
            "{model}"
        );
    }

    #[test]
    fn architecture_workspace_defaults_to_capabilities_not_package_edges() {
        let mut graph = KnowledgeGraph::new();
        for (id, source_file) in [
            ("api_file", "internal/api/public/handler.go"),
            ("domain_file", "internal/domain/catalog/service.go"),
        ] {
            graph
                .add_node(GraphNode {
                    id: id.into(),
                    label: id.into(),
                    source_file: source_file.into(),
                    source_location: None,
                    node_type: NodeType::File,
                    community: None,
                    extra: HashMap::new(),
                })
                .unwrap();
        }
        graph
            .add_edge(GraphEdge {
                source: "api_file".into(),
                target: "domain_file".into(),
                relation: "uses".into(),
                confidence: Confidence::Extracted,
                confidence_score: 1.0,
                source_file: "internal/api/public/handler.go".into(),
                source_location: None,
                weight: 1.0,
                extra: HashMap::new(),
            })
            .unwrap();

        let dir = tempfile::tempdir().unwrap();
        let workspace = export_architecture_workspace(
            &graph,
            dir.path(),
            &ArchitectureOptions {
                service_name: Some("consumer".into()),
                ..ArchitectureOptions::default()
            },
        )
        .unwrap();
        let model = std::fs::read_to_string(workspace.join("model.c4")).unwrap();

        assert!(model.contains("consumer = system \"consumer\""));
        assert!(model.contains("cap_catalog = container \"Catalog\""));
        assert!(!model.contains("pkg_internal_api_public"));
        assert!(!model.contains("pkg_internal_domain_catalog"));
        assert!(model.contains("mod_internal_api_public = component \"internal/api/public\""));
        assert!(model.contains("-[uses]->"));
        assert!(!model.contains("container \"handler.go\""));
        assert!(!model.contains("container \"service.go\""));
    }
}
