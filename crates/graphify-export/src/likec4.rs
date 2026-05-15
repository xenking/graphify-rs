//! LikeC4 workspace export.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use graphify_core::graph::KnowledgeGraph;
use graphify_core::model::{GraphEdge, GraphNode, NodeType};
use graphify_core::quality;
use tracing::info;

use crate::architecture::{ArchitectureOptions, export_architecture_workspace};

const ROOT_ID: &str = "graphify";
const ROOT_TITLE: &str = "graphify";
pub const DEFAULT_LIKEC4_MAX_NODES: usize = 250;
pub const DEFAULT_LIKEC4_MAX_RELATIONS: usize = 150;

/// Amount of graph detail emitted to LikeC4.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum LikeC4Detail {
    /// Capability/contract service architecture via architecture manifest.
    Architecture,
    /// Default signal-balanced view: architecture entities plus non-temporary concepts.
    #[default]
    Balanced,
    /// Keep code-level nodes too, still subject to caps and noise filters.
    Full,
}

/// LikeC4 export tuning.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LikeC4Options {
    pub service_name: Option<String>,
    pub root_path: Option<PathBuf>,
    pub max_nodes: usize,
    pub max_relations: usize,
    pub detail: LikeC4Detail,
    pub include_paths: Vec<String>,
    pub exclude_paths: Vec<String>,
}

impl Default for LikeC4Options {
    fn default() -> Self {
        Self {
            service_name: None,
            root_path: None,
            max_nodes: DEFAULT_LIKEC4_MAX_NODES,
            max_relations: DEFAULT_LIKEC4_MAX_RELATIONS,
            detail: LikeC4Detail::Balanced,
            include_paths: Vec::new(),
            exclude_paths: Vec::new(),
        }
    }
}

/// Export a LikeC4 workspace under `output_dir/likec4`.
///
/// The generated workspace contains `likec4.config.json`, `specification.c4`,
/// `model.c4`, `views.c4`, and `README.md`. It is Rust-only generation: users
/// can run the LikeC4 CLI separately to validate or build a static site.
pub fn export_likec4(
    graph: &KnowledgeGraph,
    communities: &HashMap<usize, Vec<String>>,
    community_labels: &HashMap<usize, String>,
    output_dir: &Path,
) -> anyhow::Result<PathBuf> {
    export_likec4_with_options(
        graph,
        communities,
        community_labels,
        output_dir,
        &LikeC4Options::default(),
    )
}

/// Export a LikeC4 workspace using explicit options.
pub fn export_likec4_with_options(
    graph: &KnowledgeGraph,
    communities: &HashMap<usize, Vec<String>>,
    community_labels: &HashMap<usize, String>,
    output_dir: &Path,
    options: &LikeC4Options,
) -> anyhow::Result<PathBuf> {
    if matches!(options.detail, LikeC4Detail::Architecture) {
        let architecture_defaults = ArchitectureOptions::default();
        return export_architecture_workspace(
            graph,
            output_dir,
            &ArchitectureOptions {
                service_name: options.service_name.clone(),
                root_path: options.root_path.clone(),
                max_packages: if options.max_nodes == DEFAULT_LIKEC4_MAX_NODES {
                    architecture_defaults.max_packages
                } else {
                    options.max_nodes
                },
                max_dependencies: options.max_relations,
                include_paths: options.include_paths.clone(),
                exclude_paths: options.exclude_paths.clone(),
            },
        );
    }

    let workspace = output_dir.join("likec4");
    fs::create_dir_all(&workspace)?;

    let source_paths = SourcePathResolver::from_graph(graph);
    let nodes = select_likec4_nodes(graph, options);
    let ids = LikeC4IdMap::from_nodes(&nodes, &source_paths);
    let selected_ids = nodes
        .iter()
        .map(|node| node.id.clone())
        .collect::<HashSet<_>>();
    let relations = select_likec4_relations(graph, &selected_ids, options);
    let relationship_kinds = relationship_kinds(&relations);

    fs::write(workspace.join("likec4.config.json"), config_json()?)?;
    fs::write(
        workspace.join("specification.c4"),
        write_specification(&relationship_kinds),
    )?;
    fs::write(
        workspace.join("model.c4"),
        write_model(&nodes, &relations, community_labels, &ids, &source_paths),
    )?;
    fs::write(
        workspace.join("views.c4"),
        write_views(communities, community_labels, &ids),
    )?;
    fs::write(workspace.join("README.md"), readme())?;

    info!(path = %workspace.display(), "exported LikeC4 workspace");
    Ok(workspace)
}

fn sorted_nodes(graph: &KnowledgeGraph) -> Vec<&GraphNode> {
    let mut nodes = graph.nodes();
    nodes.sort_by(|a, b| a.id.cmp(&b.id));
    nodes
}

fn select_likec4_nodes<'a>(
    graph: &'a KnowledgeGraph,
    options: &LikeC4Options,
) -> Vec<&'a GraphNode> {
    let mut nodes = sorted_nodes(graph)
        .into_iter()
        .filter(|node| include_node_in_likec4(node, options))
        .collect::<Vec<_>>();

    nodes.sort_by(|a, b| {
        likec4_node_score(graph, b)
            .cmp(&likec4_node_score(graph, a))
            .then_with(|| a.id.cmp(&b.id))
    });
    nodes.truncate(options.max_nodes);
    nodes.sort_by(|a, b| a.id.cmp(&b.id));
    nodes
}

fn likec4_node_score(graph: &KnowledgeGraph, node: &GraphNode) -> u64 {
    let type_score = match node.node_type {
        NodeType::Struct
        | NodeType::Class
        | NodeType::Interface
        | NodeType::Trait
        | NodeType::Enum => 8_000,
        NodeType::Module | NodeType::File => 6_000,
        NodeType::Package | NodeType::Namespace => 5_000,
        NodeType::Concept | NodeType::Paper | NodeType::Image => 4_000,
        NodeType::Function | NodeType::Method | NodeType::Constant | NodeType::Variable => 0,
    };
    let degree_score = graph.degree(&node.id).min(500) as u64 * 10;
    let quality_score = (quality::node_priority(node).clamp(0.0, 2.0) * 100.0).round() as u64;
    type_score + degree_score + quality_score
}

#[derive(Debug, Clone)]
struct LikeC4Relation<'a> {
    source: String,
    target: String,
    kind: String,
    title: String,
    edge: &'a GraphEdge,
    lifted: bool,
}

fn select_likec4_relations<'a>(
    graph: &'a KnowledgeGraph,
    selected_ids: &HashSet<String>,
    options: &LikeC4Options,
) -> Vec<LikeC4Relation<'a>> {
    let node_scores = selected_ids
        .iter()
        .filter_map(|id| {
            graph
                .get_node(id)
                .map(|node| (id.as_str(), likec4_node_score(graph, node)))
        })
        .collect::<HashMap<_, _>>();
    let owner_map = architecture_owner_map(
        graph,
        selected_ids,
        matches!(options.detail, LikeC4Detail::Architecture),
    );

    let mut seen = HashSet::new();
    let mut edges = sorted_edges(graph)
        .into_iter()
        .filter(|edge| include_edge_in_likec4(edge))
        .filter_map(|edge| {
            let source = owner_map.get(&edge.source)?;
            let target = owner_map.get(&edge.target)?;
            if source == target {
                return None;
            }
            let kind = relationship_kind_for_edge(&edge.relation);
            if is_same_file_noise_relation(graph, source, target, &kind) {
                return None;
            }
            let lifted = source != &edge.source || target != &edge.target;
            Some(LikeC4Relation {
                source: source.clone(),
                target: target.clone(),
                kind,
                title: edge.relation.clone(),
                edge,
                lifted,
            })
        })
        .filter(|relation| {
            seen.insert((
                relation.source.clone(),
                relation.target.clone(),
                relation.kind.clone(),
            ))
        })
        .collect::<Vec<_>>();

    edges.sort_by(|a, b| {
        likec4_edge_score(b, &node_scores)
            .cmp(&likec4_edge_score(a, &node_scores))
            .then_with(|| a.source.cmp(&b.source))
            .then_with(|| a.target.cmp(&b.target))
            .then_with(|| a.kind.cmp(&b.kind))
    });
    edges.truncate(options.max_relations);
    edges.sort_by(|a, b| {
        a.source
            .cmp(&b.source)
            .then_with(|| a.target.cmp(&b.target))
            .then_with(|| a.kind.cmp(&b.kind))
    });
    edges
}

fn likec4_edge_score(relation: &LikeC4Relation<'_>, node_scores: &HashMap<&str, u64>) -> u64 {
    let source_score = node_scores
        .get(relation.source.as_str())
        .copied()
        .unwrap_or(0);
    let target_score = node_scores
        .get(relation.target.as_str())
        .copied()
        .unwrap_or(0);
    let confidence_score = if relation.edge.confidence_score.is_finite() {
        (relation.edge.confidence_score.clamp(0.0, 1.0) * 100.0).round() as u64
    } else {
        0
    };
    let relation_score = match relation.kind.as_str() {
        "depends_on" | "implements" | "extends" => 300,
        "uses" | "calls" | "imports" => 200,
        "mentions" | "relates_to" => 50,
        _ => 100,
    };
    source_score + target_score + confidence_score + relation_score
}

fn architecture_owner_map(
    graph: &KnowledgeGraph,
    selected_ids: &HashSet<String>,
    prefer_file_owners: bool,
) -> HashMap<String, String> {
    let defines_parent = graph
        .edges()
        .into_iter()
        .filter(|edge| edge.relation.eq_ignore_ascii_case("defines"))
        .map(|edge| (edge.target.clone(), edge.source.clone()))
        .collect::<HashMap<_, _>>();

    let mut selected_by_file: HashMap<String, Vec<String>> = HashMap::new();
    for id in selected_ids {
        let Some(node) = graph.get_node(id) else {
            continue;
        };
        selected_by_file
            .entry(normalize_source_path(&node.source_file))
            .or_default()
            .push(id.clone());
    }
    for owners in selected_by_file.values_mut() {
        owners.sort_by(|a, b| {
            let a = graph.get_node(a);
            let b = graph.get_node(b);
            owner_node_rank(b).cmp(&owner_node_rank(a)).then_with(|| {
                a.map(|n| n.id.as_str())
                    .unwrap_or_default()
                    .cmp(b.map(|n| n.id.as_str()).unwrap_or_default())
            })
        });
    }

    let mut owners = HashMap::new();
    for id in graph.node_ids() {
        if prefer_file_owners {
            let Some(node) = graph.get_node(&id) else {
                continue;
            };
            let file = normalize_source_path(&node.source_file);
            if let Some(owner) = selected_by_file.get(&file).and_then(|ids| ids.first()) {
                owners.insert(id, owner.clone());
                continue;
            }
        }
        if let Some(owner) = architecture_owner_for(&id, graph, selected_ids, &defines_parent) {
            owners.insert(id, owner);
            continue;
        }
        let Some(node) = graph.get_node(&id) else {
            continue;
        };
        let file = normalize_source_path(&node.source_file);
        if let Some(owner) = selected_by_file.get(&file).and_then(|ids| ids.first()) {
            owners.insert(id, owner.clone());
        }
    }
    owners
}

fn architecture_owner_for(
    id: &str,
    graph: &KnowledgeGraph,
    selected_ids: &HashSet<String>,
    defines_parent: &HashMap<String, String>,
) -> Option<String> {
    if selected_ids.contains(id) {
        return Some(id.to_string());
    }

    let mut current = id;
    let mut seen = HashSet::new();
    while seen.insert(current.to_string()) {
        let parent = defines_parent.get(current)?;
        if selected_ids.contains(parent) {
            return Some(parent.clone());
        }
        graph.get_node(parent)?;
        current = parent;
    }
    None
}

fn owner_node_rank(node: Option<&GraphNode>) -> u8 {
    let Some(node) = node else {
        return 0;
    };
    match node.node_type {
        NodeType::File => 9,
        NodeType::Module => 8,
        NodeType::Package | NodeType::Namespace => 7,
        NodeType::Class
        | NodeType::Struct
        | NodeType::Interface
        | NodeType::Trait
        | NodeType::Enum => 6,
        NodeType::Concept | NodeType::Paper | NodeType::Image => 4,
        NodeType::Function | NodeType::Method | NodeType::Constant | NodeType::Variable => 1,
    }
}

fn relationship_kinds(edges: &[LikeC4Relation<'_>]) -> BTreeSet<String> {
    let mut kinds: BTreeSet<String> = edges.iter().map(|edge| edge.kind.clone()).collect();
    if kinds.is_empty() {
        kinds.insert("relates_to".to_string());
    }
    kinds
}

fn is_same_file_noise_relation(
    graph: &KnowledgeGraph,
    source: &str,
    target: &str,
    kind: &str,
) -> bool {
    if !matches!(kind, "calls" | "uses" | "imports" | "contains" | "mentions") {
        return false;
    }
    let Some(source) = graph.get_node(source) else {
        return false;
    };
    let Some(target) = graph.get_node(target) else {
        return false;
    };
    normalize_source_path(&source.source_file) == normalize_source_path(&target.source_file)
}

fn config_json() -> anyhow::Result<String> {
    let config = serde_json::json!({
        "$schema": "https://likec4.dev/schemas/config.json",
        "name": ROOT_ID,
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

fn write_specification(relationship_kinds: &BTreeSet<String>) -> String {
    let mut out = String::from("specification {\n");
    out.push_str("  element system\n");
    out.push_str("  element container\n");
    out.push_str("  element component\n");
    out.push_str("  element artifact\n\n");

    for kind in relationship_kinds {
        writeln!(out, "  relationship {kind}").expect("write to string");
    }

    out.push_str("}\n");
    out
}

fn write_model(
    nodes: &[&GraphNode],
    relations: &[LikeC4Relation<'_>],
    community_labels: &HashMap<usize, String>,
    ids: &LikeC4IdMap,
    source_paths: &SourcePathResolver,
) -> String {
    let mut out = String::from("model {\n");
    writeln!(out, "  {ROOT_ID} = system \"{ROOT_TITLE}\" {{").expect("write to string");
    out.push_str("    metadata {\n");
    out.push_str("      generator \"graphify-rs\"\n");
    out.push_str("      source_model \"graphify\"\n");
    out.push_str("    }\n");

    let mut ungrouped = Vec::new();
    let mut grouped: BTreeMap<usize, Vec<&GraphNode>> = BTreeMap::new();
    for node in nodes {
        if let Some(community) = node.community {
            grouped.entry(community).or_default().push(*node);
        } else {
            ungrouped.push(*node);
        }
    }

    for node in ungrouped {
        let Some(id) = ids.get(&node.id) else {
            continue;
        };
        write_element(&mut out, node, id, 4, source_paths);
    }

    for (community, members) in grouped {
        let label = likec4_community_label(community, &members, community_labels);
        let label = escape_likec4_string(&label);
        writeln!(out, "    community_{community} = container \"{label}\" {{")
            .expect("write to string");
        out.push_str("      metadata {\n");
        write_metadata_string(&mut out, 8, "graphify_community", &community.to_string());
        out.push_str("      }\n");
        for node in members {
            let Some(id) = ids.get(&node.id) else {
                continue;
            };
            write_element(&mut out, node, id, 6, source_paths);
        }
        out.push_str("    }\n");
    }

    if !relations.is_empty() && !nodes.is_empty() {
        out.push('\n');
    }

    for relation in relations {
        let Some(source) = ids.reference(&relation.source) else {
            continue;
        };
        let Some(target) = ids.reference(&relation.target) else {
            continue;
        };
        let kind = &relation.kind;
        let title = escape_likec4_string(&relation.title);
        writeln!(out, "    {source} -[{kind}]-> {target} \"{title}\" {{").expect("write to string");
        write_edge_metadata(&mut out, relation, source_paths);
        out.push_str("    }\n");
    }

    out.push_str("  }\n");
    out.push_str("}\n");
    out
}

fn likec4_community_label(
    community: usize,
    members: &[&GraphNode],
    community_labels: &HashMap<usize, String>,
) -> String {
    community_labels
        .get(&community)
        .filter(|label| !is_code_level_label(label))
        .cloned()
        .or_else(|| {
            members
                .iter()
                .filter(|node| !is_code_level_label(&node.label))
                .max_by_key(|node| {
                    (
                        owner_node_rank(Some(node)),
                        (quality::node_priority(node) * 100.0).round() as i32,
                    )
                })
                .map(|node| node.label.clone())
        })
        .unwrap_or_else(|| format!("Community {community}"))
}

fn is_code_level_label(label: &str) -> bool {
    let trimmed = label.trim();
    trimmed.ends_with("()")
        || trimmed.starts_with('.')
        || trimmed == "main"
        || trimmed == "lib"
        || trimmed == "mod"
}

fn write_element(
    out: &mut String,
    node: &GraphNode,
    id: &str,
    indent: usize,
    source_paths: &SourcePathResolver,
) {
    let padding = " ".repeat(indent);
    let kind = element_kind_for_node(node);
    let label = escape_likec4_string(&node.label);
    writeln!(out, "{padding}{id} = {kind} \"{label}\" {{").expect("write to string");
    write_metadata_string(
        out,
        indent + 2,
        "technology",
        node_type_name(&node.node_type),
    );
    write_node_metadata(out, node, source_paths, indent + 2);
    writeln!(out, "{padding}}}").expect("write to string");
}

fn include_edge_in_likec4(edge: &GraphEdge) -> bool {
    !edge.relation.eq_ignore_ascii_case("defines") && !is_transient_likec4_path(&edge.source_file)
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

fn include_node_in_likec4(node: &GraphNode, options: &LikeC4Options) -> bool {
    if !matches_path_filters(&node.source_file, options) {
        return false;
    }
    if is_transient_likec4_path(&node.source_file) {
        return false;
    }
    if quality::is_low_signal_node(node) {
        return false;
    }
    if is_import_node(node) {
        return false;
    }

    if is_sql_path(&node.source_file) {
        return is_canonical_sql_schema_path(&node.source_file)
            && !is_noise_sql_label(&node.label)
            && matches!(
                node.node_type,
                NodeType::Struct | NodeType::Module | NodeType::Concept
            );
    }

    match options.detail {
        LikeC4Detail::Architecture => matches!(
            node.node_type,
            NodeType::File | NodeType::Module | NodeType::Namespace | NodeType::Package
        ),
        LikeC4Detail::Balanced => !matches!(
            node.node_type,
            NodeType::Function | NodeType::Method | NodeType::Constant | NodeType::Variable
        ),
        LikeC4Detail::Full => true,
    }
}

fn write_node_metadata(
    out: &mut String,
    node: &GraphNode,
    source_paths: &SourcePathResolver,
    indent: usize,
) {
    let padding = " ".repeat(indent);
    writeln!(out, "{padding}metadata {{").expect("write to string");
    write_metadata_string(out, indent + 2, "graphify_id", &source_paths.node_key(node));
    write_metadata_string(
        out,
        indent + 2,
        "graphify_type",
        node_type_name(&node.node_type),
    );
    write_metadata_string(
        out,
        indent + 2,
        "source_file",
        &source_paths.display(&node.source_file),
    );
    if let Some(location) = node.source_location.as_deref() {
        write_metadata_string(out, indent + 2, "source_location", location);
    }
    if let Some(community) = node.community {
        write_metadata_string(out, indent + 2, "community", &community.to_string());
    }
    let source_kind = quality::node_source_kind(node);
    write_metadata_string(out, indent + 2, "source_kind", &source_kind);
    let flags = quality::node_flags(node);
    if !flags.is_empty() {
        write_metadata_string(out, indent + 2, "source_flags", &flags.join(","));
    }
    writeln!(out, "{padding}}}").expect("write to string");
}

fn write_edge_metadata(
    out: &mut String,
    relation: &LikeC4Relation<'_>,
    source_paths: &SourcePathResolver,
) {
    let edge = relation.edge;
    out.push_str("      metadata {\n");
    write_metadata_string(out, 8, "graphify_relation", &edge.relation);
    if relation.lifted {
        write_metadata_string(out, 8, "graphify_lifted", "true");
        write_metadata_string(out, 8, "graphify_source_node", &edge.source);
        write_metadata_string(out, 8, "graphify_target_node", &edge.target);
    }
    write_metadata_string(
        out,
        8,
        "source_file",
        &source_paths.display(&edge.source_file),
    );
    if let Some(location) = edge.source_location.as_deref() {
        write_metadata_string(out, 8, "source_location", location);
    }
    write_metadata_string(
        out,
        8,
        "confidence",
        &format!("{:?}", edge.confidence).to_ascii_lowercase(),
    );
    write_metadata_string(
        out,
        8,
        "confidence_score",
        &format!("{:.3}", edge.confidence_score),
    );
    out.push_str("      }\n");
}

fn write_metadata_string(out: &mut String, indent: usize, key: &str, value: &str) {
    let padding = " ".repeat(indent);
    let escaped = escape_likec4_string(value);
    writeln!(out, "{padding}{key} \"{escaped}\"").expect("write to string");
}

fn matches_path_filters(path: &str, options: &LikeC4Options) -> bool {
    let path = normalize_source_path(path);
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
    let pattern = normalize_source_path(pattern);
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

fn normalize_source_path(path: &str) -> String {
    path.trim_start_matches("./").replace('\\', "/")
}

fn is_transient_likec4_path(path: &str) -> bool {
    let normalized = normalize_source_path(path);
    let lower = normalized.to_ascii_lowercase();
    if lower.starts_with(".graphify/")
        || lower.contains("/.graphify/")
        || lower.starts_with(".likec4/")
        || lower.contains("/.likec4/")
        || lower.starts_with("likec4-dist/")
        || lower.contains("/likec4-dist/")
    {
        return true;
    }

    let file_name = Path::new(&lower)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(lower.as_str());
    if file_name.ends_with('~')
        || file_name.ends_with(".tmp")
        || file_name.ends_with(".temp")
        || file_name.ends_with(".bak")
        || file_name.ends_with(".orig")
        || file_name.ends_with(".rej")
        || file_name.ends_with(".patch")
        || file_name.ends_with(".log")
        || file_name.ends_with(".user.js")
    {
        return true;
    }

    is_root_scratch_doc_path(&lower) || is_root_scratch_doc_path(file_name)
}

fn is_root_scratch_doc_path(path: &str) -> bool {
    if path.contains('/') {
        return false;
    }
    let Some((stem, ext)) = path.rsplit_once('.') else {
        return false;
    };
    if !matches!(ext, "md" | "txt" | "rst") {
        return false;
    }
    let canonical = [
        "agents",
        "architecture",
        "changelog",
        "claude",
        "contributing",
        "license",
        "product",
        "readme",
        "roadmap",
    ];
    if canonical.contains(&stem) {
        return false;
    }
    stem.starts_with("dd")
        || stem.starts_with("mr")
        || [
            "confluence",
            "decomposition",
            "draft",
            "migration",
            "response",
            "review",
            "sync",
            "template",
        ]
        .iter()
        .any(|needle| stem.contains(needle))
}

fn is_import_node(node: &GraphNode) -> bool {
    node.id.contains("_import_")
        || node
            .source_location
            .as_deref()
            .is_some_and(|location| location.eq_ignore_ascii_case("import"))
}

fn is_sql_path(path: &str) -> bool {
    path.to_ascii_lowercase().ends_with(".sql")
}

fn is_canonical_sql_schema_path(path: &str) -> bool {
    let path = path.replace('\\', "/").to_ascii_lowercase();
    path.ends_with("/schema.sql") || path == "schema.sql"
}

fn is_noise_sql_label(label: &str) -> bool {
    let label = label.trim().to_ascii_lowercase();
    label.len() < 3
        || matches!(
            label.as_str(),
            "all"
                | "an"
                | "and"
                | "array"
                | "changed"
                | "data"
                | "date"
                | "dates"
                | "get"
                | "join"
                | "leave"
                | "materialized"
                | "previous"
                | "process"
                | "round"
                | "same"
                | "second"
                | "select"
                | "series"
                | "table"
                | "the"
                | "trigger"
                | "view"
                | "volume"
        )
}

fn write_views(
    communities: &HashMap<usize, Vec<String>>,
    community_labels: &HashMap<usize, String>,
    ids: &LikeC4IdMap,
) -> String {
    let mut out = String::from("views {\n");
    out.push_str("  view index {\n");
    out.push_str("    title \"Graphify overview\"\n");
    out.push_str("    include graphify.*\n");
    out.push_str("    autoLayout TopBottom\n");
    out.push_str("  }\n");

    let ordered: BTreeMap<usize, Vec<String>> = communities
        .iter()
        .map(|(cid, members)| (*cid, members.clone()))
        .collect();

    for (cid, members) in ordered {
        let label = community_labels
            .get(&cid)
            .cloned()
            .unwrap_or_else(|| format!("Community {cid}"));
        let label = escape_likec4_string(&label);
        writeln!(out, "\n  view community_{cid} {{").expect("write to string");
        writeln!(out, "    title \"{label}\"").expect("write to string");

        let mut refs: Vec<String> = members
            .iter()
            .filter_map(|member| ids.reference(member).map(|id| format!("{ROOT_ID}.{id}")))
            .collect();
        refs.sort();
        refs.dedup();
        if refs.is_empty() {
            out.push_str("    include graphify.*\n");
        } else {
            for reference in refs {
                writeln!(out, "    include {reference}").expect("write to string");
            }
        }
        out.push_str("    autoLayout TopBottom\n");
        out.push_str("  }\n");
    }

    out.push_str("}\n");
    out
}

fn readme() -> String {
    [
        "# graphify LikeC4 workspace",
        "",
        "Generated by `graphify-rs build --format likec4`.",
        "",
        "Generated files may be refreshed by graphify. LikeC4 manual layout snapshots are persisted under `.likec4/` and are not deleted or rewritten by graphify.",
        "",
        "Use LikeC4 CLI separately:",
        "",
        "```bash",
        "npx likec4 start",
        "npx likec4 build -o ../likec4-dist",
        "npx likec4 validate",
        "```",
        "",
        "LikeC4 CLI may require Node.js 22.22.0 or newer.",
        "",
    ]
    .join("\n")
}

fn element_kind_for_node(node: &GraphNode) -> &'static str {
    match node.node_type {
        NodeType::Package | NodeType::Namespace => "system",
        NodeType::Module | NodeType::File => "container",
        NodeType::Paper | NodeType::Image => "artifact",
        NodeType::Class
        | NodeType::Function
        | NodeType::Concept
        | NodeType::Method
        | NodeType::Interface
        | NodeType::Enum
        | NodeType::Struct
        | NodeType::Trait
        | NodeType::Constant
        | NodeType::Variable => "component",
    }
}

fn node_type_name(node_type: &NodeType) -> &'static str {
    match node_type {
        NodeType::Class => "class",
        NodeType::Function => "function",
        NodeType::Module => "module",
        NodeType::Concept => "concept",
        NodeType::Paper => "paper",
        NodeType::Image => "image",
        NodeType::File => "file",
        NodeType::Method => "method",
        NodeType::Interface => "interface",
        NodeType::Enum => "enum",
        NodeType::Struct => "struct",
        NodeType::Trait => "trait",
        NodeType::Constant => "constant",
        NodeType::Variable => "variable",
        NodeType::Package => "package",
        NodeType::Namespace => "namespace",
    }
}

fn relationship_kind_for_edge(relation: &str) -> String {
    let trimmed = relation.trim();
    if trimmed.is_empty() {
        return "relates_to".to_string();
    }
    let sanitized = sanitize_identifier(trimmed);
    if sanitized == "node" {
        "relates_to".to_string()
    } else {
        sanitized
    }
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
    if is_likec4_keyword(&out) {
        out.insert(0, '_');
    }
    out
}

fn is_likec4_keyword(value: &str) -> bool {
    matches!(
        value,
        "autoLayout"
            | "deployment"
            | "deploymentNode"
            | "element"
            | "global"
            | "include"
            | "model"
            | "relationship"
            | "specification"
            | "style"
            | "view"
            | "views"
    )
}

#[derive(Debug, Clone)]
struct SourcePathResolver {
    root: Option<PathBuf>,
}

impl SourcePathResolver {
    fn from_graph(graph: &KnowledgeGraph) -> Self {
        let mut common = None::<Vec<String>>;

        for path in graph
            .nodes()
            .into_iter()
            .map(|node| node.source_file.as_str())
            .chain(
                graph
                    .edges()
                    .into_iter()
                    .map(|edge| edge.source_file.as_str()),
            )
        {
            let Some(components) = absolute_parent_components(path) else {
                continue;
            };
            match &mut common {
                Some(prefix) => truncate_to_common_prefix(prefix, &components),
                None => common = Some(components),
            }
        }

        let root = common
            .filter(|components| components.len() > 1)
            .map(path_from_components);
        Self { root }
    }

    fn display(&self, path: &str) -> String {
        if let Some(root) = &self.root {
            let source = Path::new(path);
            if source.is_absolute()
                && let Ok(relative) = source.strip_prefix(root)
            {
                return normalize_source_path(&relative.to_string_lossy());
            }
        }
        normalize_source_path(path)
    }

    fn node_key(&self, node: &GraphNode) -> String {
        format!("{}:{}", self.display(&node.source_file), node.label)
    }
}

fn absolute_parent_components(path: &str) -> Option<Vec<String>> {
    let path = Path::new(path);
    if !path.is_absolute() {
        return None;
    }
    Some(
        path.parent()?
            .components()
            .map(|component| component.as_os_str().to_string_lossy().to_string())
            .collect(),
    )
}

fn truncate_to_common_prefix(prefix: &mut Vec<String>, components: &[String]) {
    let keep = prefix
        .iter()
        .zip(components)
        .take_while(|(a, b)| a == b)
        .count();
    prefix.truncate(keep);
}

fn path_from_components(components: Vec<String>) -> PathBuf {
    let mut path = PathBuf::new();
    for component in components {
        path.push(component);
    }
    path
}

#[derive(Debug)]
struct LikeC4IdMap {
    ids: HashMap<String, String>,
    references: HashMap<String, String>,
}

impl LikeC4IdMap {
    fn from_nodes(nodes: &[&GraphNode], source_paths: &SourcePathResolver) -> Self {
        let mut used = HashSet::new();
        let mut ids = HashMap::new();
        let mut references = HashMap::new();

        for node in nodes {
            let base = sanitize_identifier(&source_paths.node_key(node));
            let mut candidate = base.clone();
            let mut suffix = 2;
            while used.contains(&candidate) {
                candidate = format!("{base}_{suffix}");
                suffix += 1;
            }
            used.insert(candidate.clone());
            let reference = if let Some(community) = node.community {
                format!("community_{community}.{candidate}")
            } else {
                candidate.clone()
            };
            ids.insert(node.id.clone(), candidate);
            references.insert(node.id.clone(), reference);
        }

        Self { ids, references }
    }

    #[cfg(test)]
    fn from_raw_ids<I, S>(raw_ids: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut used = HashSet::new();
        let mut ids = HashMap::new();
        let mut references = HashMap::new();

        for raw in raw_ids {
            let raw = raw.as_ref();
            let base = sanitize_identifier(raw);
            let mut candidate = base.clone();
            let mut suffix = 2;
            while used.contains(&candidate) {
                candidate = format!("{base}_{suffix}");
                suffix += 1;
            }
            used.insert(candidate.clone());
            ids.insert(raw.to_string(), candidate);
            references.insert(raw.to_string(), ids.get(raw).cloned().unwrap_or_default());
        }

        Self { ids, references }
    }

    fn get(&self, raw: &str) -> Option<&str> {
        self.ids.get(raw).map(String::as_str)
    }

    fn reference(&self, raw: &str) -> Option<&str> {
        self.references.get(raw).map(String::as_str)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use graphify_core::confidence::Confidence;
    use graphify_core::graph::KnowledgeGraph;
    use graphify_core::model::{GraphEdge, GraphNode, NodeType};
    use std::collections::HashMap;

    #[test]
    fn sanitize_identifier_makes_likec4_safe_segments() {
        assert_eq!(sanitize_identifier("src/main.rs"), "src_main_rs");
        assert_eq!(sanitize_identifier("123 abc"), "_123_abc");
        assert_eq!(sanitize_identifier(""), "node");
        assert_eq!(sanitize_identifier("hello-world"), "hello_world");
    }

    #[test]
    fn id_map_is_stable_and_collision_safe() {
        let ids = LikeC4IdMap::from_raw_ids(["foo-bar", "foo_bar", "123 abc"]);

        assert_eq!(ids.get("foo-bar"), Some("foo_bar"));
        assert_eq!(ids.get("foo_bar"), Some("foo_bar_2"));
        assert_eq!(ids.get("123 abc"), Some("_123_abc"));
    }

    #[test]
    fn escape_likec4_string_quotes_backslashes_and_newlines() {
        assert_eq!(
            escape_likec4_string("say \"hi\"\\there\nnow"),
            "say \\\"hi\\\"\\\\there\\nnow"
        );
    }

    #[test]
    fn relationship_kind_is_valid_identifier() {
        assert_eq!(relationship_kind_for_edge("depends-on"), "depends_on");
        assert_eq!(relationship_kind_for_edge(""), "relates_to");
        assert_eq!(relationship_kind_for_edge("123"), "_123");
    }

    #[test]
    fn export_likec4_creates_workspace_files() {
        let dir = tempfile::tempdir().unwrap();
        let graph = sample_graph();
        let communities = HashMap::from([(0, vec!["foo-bar".to_string(), "foo_bar".to_string()])]);
        let community_labels = HashMap::from([(0, "Core APIs".to_string())]);

        let workspace = export_likec4(&graph, &communities, &community_labels, dir.path()).unwrap();

        assert_eq!(workspace, dir.path().join("likec4"));
        assert!(workspace.join("likec4.config.json").exists());
        assert!(workspace.join("specification.c4").exists());
        assert!(workspace.join("model.c4").exists());
        assert!(workspace.join("views.c4").exists());
        assert!(workspace.join("README.md").exists());

        let model = std::fs::read_to_string(workspace.join("model.c4")).unwrap();
        assert!(model.contains("graphify = system \"graphify\""));
        assert!(model.contains("community_0 = container \"Core APIs\""));
        assert!(model.contains("src_foo_rs_Foo_API = component \"Foo \\\"API\\\"\" {"));
        assert!(model.contains("technology \"struct\""));
        assert!(model.contains("metadata {"));
        assert!(model.contains("graphify_id \"src/foo.rs:Foo \\\"API\\\"\""));
        assert!(model.contains("source_file \"src/foo.rs\""));
        assert!(model.contains("source_location \"L1\""));
        assert!(model.contains("source_kind \"source\""));
        assert!(model.contains("src_foo_clone_rs_Foo_API_Clone = component \"Foo API Clone\" {"));
        assert!(
            model.contains(
                "community_0.src_foo_rs_Foo_API -[calls]-> community_0.src_foo_clone_rs_Foo_API_Clone \"calls\" {"
            )
        );
        assert!(model.contains("graphify_relation \"calls\""));
        assert!(!model.contains("helper"));

        let views = std::fs::read_to_string(workspace.join("views.c4")).unwrap();
        assert!(views.contains("view index"));
        assert!(views.contains("include graphify.*"));
        assert!(views.contains("view community_0 {"));
        assert!(views.contains("title \"Core APIs\""));
        assert!(views.contains("include graphify.community_0.src_foo_rs_Foo_API"));
        assert!(views.contains("include graphify.community_0.src_foo_clone_rs_Foo_API_Clone"));
        assert!(views.contains("autoLayout TopBottom"));

        let specification = std::fs::read_to_string(workspace.join("specification.c4")).unwrap();
        assert!(specification.contains("relationship calls"));

        let config: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(workspace.join("likec4.config.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(config["$schema"], "https://likec4.dev/schemas/config.json");
        assert_eq!(config["implicitViews"], true);
        assert_eq!(config["manualLayouts"]["outDir"], ".likec4");
        assert!(
            config["exclude"]
                .as_array()
                .unwrap()
                .iter()
                .any(|value| value == "**/.likec4/**")
        );
    }

    #[test]
    fn export_likec4_preserves_manual_layout_snapshots() {
        let dir = tempfile::tempdir().unwrap();
        let graph = sample_graph();
        let communities = HashMap::new();
        let community_labels = HashMap::new();

        let workspace = export_likec4(&graph, &communities, &community_labels, dir.path()).unwrap();
        let manual_dir = workspace.join(".likec4");
        std::fs::create_dir_all(&manual_dir).unwrap();
        let snap = manual_dir.join("index.likec4.snap");
        std::fs::write(&snap, "manual layout").unwrap();

        export_likec4(&graph, &communities, &community_labels, dir.path()).unwrap();

        assert_eq!(std::fs::read_to_string(snap).unwrap(), "manual layout");
    }

    #[test]
    fn export_likec4_metadata_uses_repo_relative_source_paths() {
        let project = tempfile::tempdir().unwrap();
        let output = tempfile::tempdir().unwrap();
        let src_path = project.path().join("src/api.rs");
        let db_path = project.path().join("internal/db.rs");
        let src_file = src_path.to_string_lossy().to_string();
        let db_file = db_path.to_string_lossy().to_string();
        let src_id = sanitize_identifier(&format!("{src_file}_Api"));
        let db_id = sanitize_identifier(&format!("{db_file}_Database"));
        let mut graph = KnowledgeGraph::new();
        graph
            .add_node(GraphNode {
                id: src_id.clone(),
                label: "Api".into(),
                source_file: src_file,
                source_location: Some("L10".into()),
                node_type: NodeType::Struct,
                community: None,
                extra: HashMap::new(),
            })
            .unwrap();
        graph
            .add_node(GraphNode {
                id: db_id.clone(),
                label: "Database".into(),
                source_file: db_file,
                source_location: Some("L20".into()),
                node_type: NodeType::Struct,
                community: None,
                extra: HashMap::new(),
            })
            .unwrap();
        graph
            .add_edge(GraphEdge {
                source: src_id,
                target: db_id,
                relation: "uses".into(),
                confidence: Confidence::Extracted,
                confidence_score: 1.0,
                source_file: src_path.to_string_lossy().to_string(),
                source_location: Some("L11".into()),
                weight: 1.0,
                extra: HashMap::new(),
            })
            .unwrap();

        let workspace =
            export_likec4(&graph, &HashMap::new(), &HashMap::new(), output.path()).unwrap();
        let model = std::fs::read_to_string(workspace.join("model.c4")).unwrap();

        assert!(model.contains("source_file \"src/api.rs\""));
        assert!(model.contains("source_file \"internal/db.rs\""));
        assert!(model.contains("graphify_id \"src/api.rs:Api\""));
        assert!(!model.contains(&project.path().to_string_lossy().to_string()));
    }

    #[test]
    fn include_node_filters_low_signal_and_code_level_noise() {
        let options = LikeC4Options::default();
        let code_function = GraphNode {
            id: "src/main.rs_call".into(),
            label: "call()".into(),
            source_file: "src/main.rs".into(),
            source_location: None,
            node_type: NodeType::Function,
            community: None,
            extra: HashMap::new(),
        };
        assert!(!include_node_in_likec4(&code_function, &options));

        let sql_function = GraphNode {
            id: "schema.sql_normalize".into(),
            label: "normalize()".into(),
            source_file: "schema.sql".into(),
            source_location: None,
            node_type: NodeType::Function,
            community: None,
            extra: HashMap::new(),
        };
        assert!(!include_node_in_likec4(&sql_function, &options));

        let test_struct = GraphNode {
            id: "src/foo_test.go_case".into(),
            label: "testCase".into(),
            source_file: "src/foo_test.go".into(),
            source_location: None,
            node_type: NodeType::Struct,
            community: None,
            extra: HashMap::new(),
        };
        assert!(!include_node_in_likec4(&test_struct, &options));

        let import_node = GraphNode {
            id: "src_main_go_import_context".into(),
            label: "context".into(),
            source_file: "src/main.go".into(),
            source_location: Some("import".into()),
            node_type: NodeType::Package,
            community: None,
            extra: HashMap::new(),
        };
        assert!(!include_node_in_likec4(&import_node, &options));
    }

    #[test]
    fn include_node_filters_transient_likec4_outputs_and_scratch_artifacts() {
        let options = LikeC4Options::default();

        for (path, label, ty) in [
            (
                ".graphify/likec4/model.c4",
                "GeneratedLikeC4",
                NodeType::Struct,
            ),
            (
                ".graphify/likec4/.likec4/index.snap",
                "ManualLayout",
                NodeType::Struct,
            ),
            ("MR_DESCRIPTION.md", "Merge Request", NodeType::Concept),
            (
                "/repo/service/MR_DESCRIPTION.md",
                "Merge Request",
                NodeType::File,
            ),
            ("DD template.md", "Design Doc Draft", NodeType::Concept),
            (
                "confluence-exporter-v10.user.js",
                "ConfluenceExporter",
                NodeType::Struct,
            ),
        ] {
            let node = GraphNode {
                id: path.replace(['/', '.', ' '], "_"),
                label: label.into(),
                source_file: path.into(),
                source_location: None,
                node_type: ty,
                community: None,
                extra: HashMap::new(),
            };
            assert!(
                !include_node_in_likec4(&node, &options),
                "{path} should be filtered"
            );
        }
    }

    #[test]
    fn options_path_filters_are_applied_before_export() {
        let include_only_src = LikeC4Options {
            include_paths: vec!["src/".into()],
            ..LikeC4Options::default()
        };
        let exclude_internal = LikeC4Options {
            exclude_paths: vec!["src/internal/".into()],
            ..LikeC4Options::default()
        };

        let public_node = GraphNode {
            id: "src_api".into(),
            label: "Api".into(),
            source_file: "src/api.rs".into(),
            source_location: None,
            node_type: NodeType::Struct,
            community: None,
            extra: HashMap::new(),
        };
        let doc_node = GraphNode {
            id: "docs_api".into(),
            label: "Api Docs".into(),
            source_file: "docs/api.md".into(),
            source_location: None,
            node_type: NodeType::Concept,
            community: None,
            extra: HashMap::new(),
        };
        let internal_node = GraphNode {
            id: "src_internal_impl".into(),
            label: "InternalImpl".into(),
            source_file: "src/internal/impl.rs".into(),
            source_location: None,
            node_type: NodeType::Struct,
            community: None,
            extra: HashMap::new(),
        };

        assert!(include_node_in_likec4(&public_node, &include_only_src));
        assert!(!include_node_in_likec4(&doc_node, &include_only_src));
        assert!(!include_node_in_likec4(&internal_node, &exclude_internal));
    }

    #[test]
    fn path_filter_globs_match_root_and_nested_paths() {
        assert!(path_matches_pattern(
            "confluence-exporter-v10.user.js",
            "**/*.user.js"
        ));
        assert!(path_matches_pattern(
            "tools/confluence-exporter-v10.user.js",
            "**/*.user.js"
        ));
        assert!(path_matches_pattern("src/internal/api.rs", "src/*/api.rs"));
        assert!(!path_matches_pattern("src/public/api.rs", "src/internal/*"));
    }

    #[test]
    fn include_node_prefers_canonical_sql_schema_objects() {
        let options = LikeC4Options::default();
        let schema_table = GraphNode {
            id: "schema.sql_users".into(),
            label: "users_current_state".into(),
            source_file: "database/archive/schema.sql".into(),
            source_location: None,
            node_type: NodeType::Struct,
            community: None,
            extra: HashMap::new(),
        };
        assert!(include_node_in_likec4(&schema_table, &options));

        let migration_false_positive = GraphNode {
            id: "migration_the".into(),
            label: "the".into(),
            source_file: "database/archive/migrations/20240101000000_init.up.sql".into(),
            source_location: None,
            node_type: NodeType::Struct,
            community: None,
            extra: HashMap::new(),
        };
        assert!(!include_node_in_likec4(&migration_false_positive, &options));

        let query_ref = GraphNode {
            id: "query_active_records".into(),
            label: "active_records".into(),
            source_file: "pkg/repository/queries/check.sql".into(),
            source_location: None,
            node_type: NodeType::Struct,
            community: None,
            extra: HashMap::new(),
        };
        assert!(!include_node_in_likec4(&query_ref, &options));
    }

    #[test]
    fn export_likec4_with_options_respects_size_caps() {
        let dir = tempfile::tempdir().unwrap();
        let mut graph = KnowledgeGraph::new();
        for id in ["a", "b", "c"] {
            graph
                .add_node(GraphNode {
                    id: id.into(),
                    label: id.into(),
                    source_file: format!("src/{id}.rs"),
                    source_location: None,
                    node_type: NodeType::Struct,
                    community: None,
                    extra: HashMap::new(),
                })
                .unwrap();
        }
        for (source, target) in [("a", "b"), ("a", "c"), ("b", "c")] {
            graph
                .add_edge(GraphEdge {
                    source: source.into(),
                    target: target.into(),
                    relation: "uses".into(),
                    confidence: Confidence::Extracted,
                    confidence_score: 1.0,
                    source_file: "src/a.rs".into(),
                    source_location: None,
                    weight: 1.0,
                    extra: HashMap::new(),
                })
                .unwrap();
        }

        let workspace = export_likec4_with_options(
            &graph,
            &HashMap::new(),
            &HashMap::new(),
            dir.path(),
            &LikeC4Options {
                max_nodes: 2,
                max_relations: 1,
                ..LikeC4Options::default()
            },
        )
        .unwrap();

        let model = std::fs::read_to_string(workspace.join("model.c4")).unwrap();
        assert_eq!(model.matches(" = component ").count(), 2);
        assert_eq!(model.matches("-[uses]->").count(), 1);
    }

    #[test]
    fn export_likec4_skips_same_file_uses_cliques() {
        let dir = tempfile::tempdir().unwrap();
        let mut graph = KnowledgeGraph::new();
        for id in ["Config", "Kafka", "Telemetry"] {
            graph
                .add_node(GraphNode {
                    id: id.into(),
                    label: id.into(),
                    source_file: "internal/app/config.go".into(),
                    source_location: None,
                    node_type: NodeType::Struct,
                    community: None,
                    extra: HashMap::new(),
                })
                .unwrap();
        }
        for (source, target) in [("Config", "Kafka"), ("Kafka", "Telemetry")] {
            graph
                .add_edge(GraphEdge {
                    source: source.into(),
                    target: target.into(),
                    relation: "uses".into(),
                    confidence: Confidence::Extracted,
                    confidence_score: 1.0,
                    source_file: "internal/app/config.go".into(),
                    source_location: None,
                    weight: 1.0,
                    extra: HashMap::new(),
                })
                .unwrap();
        }

        let workspace =
            export_likec4(&graph, &HashMap::new(), &HashMap::new(), dir.path()).unwrap();

        let model = std::fs::read_to_string(workspace.join("model.c4")).unwrap();
        assert!(model.contains("Config"));
        assert!(!model.contains("-[uses]->"));
    }

    #[test]
    fn export_likec4_lifts_function_calls_to_architecture_owners() {
        let dir = tempfile::tempdir().unwrap();
        let mut graph = KnowledgeGraph::new();
        for (id, label, source_file, node_type) in [
            (
                "api/handler.rs",
                "handler",
                "api/handler.rs",
                NodeType::File,
            ),
            ("db/store.rs", "store", "db/store.rs", NodeType::File),
            (
                "api/handler.rs_handle_request",
                "handle_request()",
                "api/handler.rs",
                NodeType::Function,
            ),
            (
                "db/store.rs_load_user",
                "load_user()",
                "db/store.rs",
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
        for (source, target, relation) in [
            ("api/handler.rs", "api/handler.rs_handle_request", "defines"),
            ("db/store.rs", "db/store.rs_load_user", "defines"),
            (
                "api/handler.rs_handle_request",
                "db/store.rs_load_user",
                "calls",
            ),
        ] {
            graph
                .add_edge(GraphEdge {
                    source: source.into(),
                    target: target.into(),
                    relation: relation.into(),
                    confidence: Confidence::Extracted,
                    confidence_score: 1.0,
                    source_file: "api/handler.rs".into(),
                    source_location: None,
                    weight: 1.0,
                    extra: HashMap::new(),
                })
                .unwrap();
        }

        let workspace = export_likec4_with_options(
            &graph,
            &HashMap::new(),
            &HashMap::new(),
            dir.path(),
            &LikeC4Options {
                detail: LikeC4Detail::Architecture,
                ..LikeC4Options::default()
            },
        )
        .unwrap();

        let model = std::fs::read_to_string(workspace.join("model.c4")).unwrap();
        assert!(model.contains("pkg_api = container \"api\""));
        assert!(model.contains("pkg_db = container \"db\""));
        assert!(model.contains("pkg_api -[calls]-> pkg_db \"calls (1)\""));
        assert!(!model.contains("handle_request()"));
        assert!(!model.contains("load_user()"));
    }

    #[test]
    fn architecture_detail_collapses_type_imports_to_file_owners() {
        let dir = tempfile::tempdir().unwrap();
        let mut graph = KnowledgeGraph::new();
        for (id, label, source_file, node_type) in [
            ("cmd/main.go", "main", "cmd/main.go", NodeType::File),
            (
                "pkg/store/store.go",
                "store",
                "pkg/store/store.go",
                NodeType::File,
            ),
            (
                "cmd/main.go_Server",
                "Server",
                "cmd/main.go",
                NodeType::Struct,
            ),
            (
                "pkg/store/store.go_Repository",
                "Repository",
                "pkg/store/store.go",
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
                "cmd/main.go",
                "cmd/main.go_Server",
                "defines",
                "cmd/main.go",
            ),
            (
                "pkg/store/store.go",
                "pkg/store/store.go_Repository",
                "defines",
                "pkg/store/store.go",
            ),
            (
                "cmd/main.go_Server",
                "pkg/store/store.go_Repository",
                "uses",
                "cmd/main.go",
            ),
        ] {
            graph
                .add_edge(GraphEdge {
                    source: source.into(),
                    target: target.into(),
                    relation: relation.into(),
                    confidence: Confidence::Inferred,
                    confidence_score: 0.8,
                    source_file: source_file.into(),
                    source_location: None,
                    weight: 0.8,
                    extra: HashMap::new(),
                })
                .unwrap();
        }

        let workspace = export_likec4_with_options(
            &graph,
            &HashMap::new(),
            &HashMap::new(),
            dir.path(),
            &LikeC4Options {
                detail: LikeC4Detail::Architecture,
                ..LikeC4Options::default()
            },
        )
        .unwrap();

        let model = std::fs::read_to_string(workspace.join("model.c4")).unwrap();
        assert!(model.contains("pkg_cmd = container \"cmd\""));
        assert!(model.contains("pkg_pkg_store = container \"pkg/store\""));
        assert!(model.contains("pkg_cmd -[uses]-> pkg_pkg_store \"uses (1)\""));
        assert!(!model.contains("component \"Server\""));
        assert!(!model.contains("component \"Repository\""));
        assert!(!model.contains("cmd_main_go_Server -[uses]->"));
    }

    #[test]
    fn select_likec4_nodes_caps_large_graphs_by_architecture_signal() {
        let mut graph = KnowledgeGraph::new();
        for idx in 0..(DEFAULT_LIKEC4_MAX_NODES + 5) {
            graph
                .add_node(GraphNode {
                    id: format!("file_{idx}"),
                    label: format!("file_{idx}"),
                    source_file: format!("src/file_{idx}.rs"),
                    source_location: None,
                    node_type: NodeType::File,
                    community: None,
                    extra: HashMap::new(),
                })
                .unwrap();
        }
        graph
            .add_node(GraphNode {
                id: "important_type".into(),
                label: "ImportantType".into(),
                source_file: "src/important.rs".into(),
                source_location: None,
                node_type: NodeType::Struct,
                community: None,
                extra: HashMap::new(),
            })
            .unwrap();

        let selected = select_likec4_nodes(&graph, &LikeC4Options::default());

        assert_eq!(selected.len(), DEFAULT_LIKEC4_MAX_NODES);
        assert!(selected.iter().any(|node| node.id == "important_type"));
    }

    fn sample_graph() -> KnowledgeGraph {
        let mut graph = KnowledgeGraph::new();
        graph
            .add_node(GraphNode {
                id: "foo-bar".into(),
                label: "Foo \"API\"".into(),
                source_file: "src/foo.rs".into(),
                source_location: Some("L1".into()),
                node_type: NodeType::Struct,
                community: Some(0),
                extra: HashMap::new(),
            })
            .unwrap();
        graph
            .add_node(GraphNode {
                id: "foo_bar".into(),
                label: "Foo API Clone".into(),
                source_file: "src/foo_clone.rs".into(),
                source_location: None,
                node_type: NodeType::Struct,
                community: Some(0),
                extra: HashMap::new(),
            })
            .unwrap();
        graph
            .add_node(GraphNode {
                id: "helper".into(),
                label: "helper()".into(),
                source_file: "src/foo_test.rs".into(),
                source_location: None,
                node_type: NodeType::Function,
                community: Some(0),
                extra: HashMap::new(),
            })
            .unwrap();
        graph
            .add_edge(GraphEdge {
                source: "foo-bar".into(),
                target: "foo_bar".into(),
                relation: "calls".into(),
                confidence: Confidence::Extracted,
                confidence_score: 1.0,
                source_file: "src/foo.rs".into(),
                source_location: None,
                weight: 1.0,
                extra: HashMap::new(),
            })
            .unwrap();
        graph
    }
}
