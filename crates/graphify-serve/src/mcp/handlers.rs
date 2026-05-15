//! MCP tool handler implementations.

use std::collections::{HashMap, HashSet, VecDeque};

use graphify_core::{graph::KnowledgeGraph, model::GraphEdge};
use serde_json::{Value, json};
use tracing::debug;

use crate::{
    GraphifyOutputFormat, SemanticState, bfs, format_value, graph_stats, query_search_terms,
    score_nodes, subgraph_to_text,
};

pub(crate) fn tool_result_text(text: &str) -> Value {
    json!({
        "content": [{
            "type": "text",
            "text": text
        }]
    })
}

pub(crate) fn tool_result_error(text: &str) -> Value {
    json!({
        "content": [{
            "type": "text",
            "text": text
        }],
        "isError": true
    })
}

fn output_format(args: &Value) -> GraphifyOutputFormat {
    GraphifyOutputFormat::parse(args["format"].as_str())
}

fn tool_result_value(value: &Value, format: GraphifyOutputFormat) -> Value {
    match format_value(value, format) {
        Ok(text) => tool_result_text(&text),
        Err(err) => tool_result_error(&format!("failed to format output: {err}")),
    }
}

fn node_context_value(graph: &KnowledgeGraph, node_id: &str) -> Option<Value> {
    let node = graph.get_node(node_id)?;
    Some(json!({
        "id": node.id,
        "label": node.label,
        "type": node.node_type,
        "file": node.source_file,
        "location": node.source_location,
        "community": node.community,
        "degree": graph.degree(node_id),
    }))
}

fn edge_context_value(all_edges: &[(&str, &str, &GraphEdge)], source: &str, target: &str) -> Value {
    if let Some((_, _, edge)) = all_edges.iter().find(|(src, tgt, _)| {
        (*src == source && *tgt == target) || (*src == target && *tgt == source)
    }) {
        json!({
            "source": source,
            "target": target,
            "relation": edge.relation,
            "confidence": edge.confidence,
            "confidence_score": edge.confidence_score,
            "file": edge.source_file,
        })
    } else {
        json!({
            "source": source,
            "target": target,
        })
    }
}

fn subgraph_to_value(
    graph: &KnowledgeGraph,
    nodes: &[String],
    edges: &[(String, String)],
) -> Value {
    let graph_edges = graph.edges_with_endpoints();
    let node_values: Vec<Value> = nodes
        .iter()
        .filter_map(|node_id| node_context_value(graph, node_id))
        .collect();
    let edge_values: Vec<Value> = edges
        .iter()
        .map(|(source, target)| edge_context_value(&graph_edges, source, target))
        .collect();

    json!({
        "kind": "query_graph",
        "node_count": node_values.len(),
        "edge_count": edge_values.len(),
        "nodes": node_values,
        "edges": edge_values,
    })
}

#[derive(Clone, Copy, Debug)]
struct QueryLimits {
    max_nodes: usize,
    max_edges: usize,
    max_neighbors_per_node: usize,
    hub_degree_cutoff: usize,
}

impl QueryLimits {
    fn for_budget(token_budget: usize) -> Self {
        Self {
            max_nodes: output_row_limit(token_budget, 40, 6, 36),
            max_edges: output_row_limit(token_budget, 25, 8, 64),
            max_neighbors_per_node: output_row_limit(token_budget, 120, 3, 10),
            hub_degree_cutoff: output_row_limit(token_budget, 12, 24, 160),
        }
    }
}

#[derive(Debug)]
struct QueryContext {
    nodes: Vec<String>,
    edges: Vec<(String, String)>,
    truncated: bool,
    omitted_nodes: usize,
    omitted_edges: usize,
}

fn query_context(
    graph: &KnowledgeGraph,
    start: &[String],
    terms: &[String],
    depth: usize,
    limits: QueryLimits,
) -> QueryContext {
    let mut visited: HashSet<String> = HashSet::new();
    let mut visited_order: Vec<String> = Vec::with_capacity(limits.max_nodes);
    let mut edges: Vec<(String, String)> = Vec::with_capacity(limits.max_edges);
    let mut queue: VecDeque<(String, usize)> = VecDeque::new();
    let mut truncated = false;
    let mut omitted_nodes = 0usize;
    let mut omitted_edges = 0usize;

    for node_id in start {
        if graph.get_node(node_id).is_none() {
            continue;
        }
        if !visited.insert(node_id.clone()) {
            continue;
        }
        if visited_order.len() >= limits.max_nodes {
            truncated = true;
            omitted_nodes += 1;
            continue;
        }
        visited_order.push(node_id.clone());
        queue.push_back((node_id.clone(), 0));
    }

    while let Some((current, current_depth)) = queue.pop_front() {
        if current_depth >= depth {
            continue;
        }

        let degree = graph.degree(&current);
        if !should_expand_query_node(graph, &current) && degree > limits.max_neighbors_per_node {
            truncated = true;
            omitted_nodes += degree;
            continue;
        }
        let per_node_limit = if degree > limits.hub_degree_cutoff {
            truncated = true;
            limits.max_neighbors_per_node.min(6)
        } else {
            limits.max_neighbors_per_node
        };

        let mut neighbors = graph.neighbor_ids(&current);
        neighbors.sort_by(|a, b| {
            query_node_score(graph, b, terms)
                .partial_cmp(&query_node_score(graph, a, terms))
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.cmp(b))
        });

        for (idx, neighbor_id) in neighbors.into_iter().enumerate() {
            if idx >= per_node_limit {
                truncated = true;
                omitted_nodes += 1;
                continue;
            }

            if edges.len() >= limits.max_edges {
                truncated = true;
                omitted_edges += 1;
            } else {
                edges.push((current.clone(), neighbor_id.clone()));
            }

            if visited.insert(neighbor_id.clone()) {
                if visited_order.len() >= limits.max_nodes {
                    truncated = true;
                    omitted_nodes += 1;
                    continue;
                }
                visited_order.push(neighbor_id.clone());
                queue.push_back((neighbor_id, current_depth + 1));
            }
        }
    }

    QueryContext {
        nodes: visited_order,
        edges,
        truncated,
        omitted_nodes,
        omitted_edges,
    }
}

fn should_expand_query_node(graph: &KnowledgeGraph, node_id: &str) -> bool {
    graph.get_node(node_id).is_some_and(|node| {
        !matches!(
            node.node_type,
            graphify_core::model::NodeType::File
                | graphify_core::model::NodeType::Module
                | graphify_core::model::NodeType::Package
        )
    })
}

fn query_node_score(graph: &KnowledgeGraph, node_id: &str, terms: &[String]) -> f64 {
    let Some(node) = graph.get_node(node_id) else {
        return 0.0;
    };
    let label = node.label.to_ascii_lowercase();
    let id = node.id.to_ascii_lowercase();
    let path = node.source_file.to_ascii_lowercase();

    let mut score = 0.0f64;
    for term in terms {
        let term = term.as_str();
        if label == term {
            score += 5.0;
        } else if label.contains(term) {
            score += 3.0;
        }
        if id == term {
            score += 4.0;
        } else if id.contains(term) {
            score += 2.5;
        }
        if path.contains(term) {
            score += 0.8;
        }
    }

    let quality = graphify_core::quality::node_priority(node) as f64;
    let node_kind = match node.node_type {
        graphify_core::model::NodeType::Function | graphify_core::model::NodeType::Method => 1.25,
        graphify_core::model::NodeType::Struct
        | graphify_core::model::NodeType::Class
        | graphify_core::model::NodeType::Interface
        | graphify_core::model::NodeType::Enum
        | graphify_core::model::NodeType::Trait => 1.1,
        graphify_core::model::NodeType::File | graphify_core::model::NodeType::Module => 0.45,
        graphify_core::model::NodeType::Variable
        | graphify_core::model::NodeType::Constant
        | graphify_core::model::NodeType::Package => 0.65,
        _ => 1.0,
    };
    let hub_penalty = (graph.degree(node_id) as f64).ln_1p() * 0.08;
    (score * quality * node_kind) - hub_penalty
}

fn query_seed_candidate(graph: &KnowledgeGraph, node_id: &str, terms: &[String]) -> bool {
    let Some(node) = graph.get_node(node_id) else {
        return false;
    };
    if query_node_score(graph, node_id, terms) > 0.0 {
        return true;
    }
    !matches!(
        node.node_type,
        graphify_core::model::NodeType::File
            | graphify_core::model::NodeType::Module
            | graphify_core::model::NodeType::Package
    )
}

pub(super) fn semantic_seed_candidate(
    graph: &KnowledgeGraph,
    candidate: &graphify_embed::SemanticMatch,
    terms: &[String],
    identifier_query: bool,
) -> bool {
    let lexical_supported =
        query_node_score(graph, &candidate.node_id, terms) > 0.0 || candidate.lexical_score >= 0.5;
    if identifier_query && !lexical_supported {
        return false;
    }
    query_seed_candidate(graph, &candidate.node_id, terms)
}

fn query_has_code_identifier(question: &str) -> bool {
    question
        .split(|ch: char| !ch.is_alphanumeric() && ch != '_' && ch != '.')
        .any(|raw| {
            raw.len() > 4
                && (raw.contains('_')
                    || raw.contains('.')
                    || raw.chars().any(|ch| ch.is_ascii_lowercase())
                        && raw.chars().any(|ch| ch.is_ascii_uppercase()))
        })
}

fn query_context_to_value(
    graph: &KnowledgeGraph,
    context: &QueryContext,
    start: &[String],
    terms: &[String],
    warnings: &[String],
) -> Value {
    let mut value = subgraph_to_value(graph, &context.nodes, &context.edges);
    value["truncated"] = json!(context.truncated);
    value["omitted_nodes"] = json!(context.omitted_nodes);
    value["omitted_edges"] = json!(context.omitted_edges);
    value["seed_count"] = json!(start.len());
    value["terms"] = json!(terms);
    if !warnings.is_empty() {
        value["warnings"] = json!(warnings);
    }
    value
}

fn output_row_limit(token_budget: usize, divisor: usize, min: usize, max: usize) -> usize {
    (token_budget / divisor).clamp(min, max)
}

fn smart_summary_to_value(
    graph: &KnowledgeGraph,
    communities: &HashMap<usize, Vec<String>>,
    level: crate::SummaryLevel,
    token_budget: usize,
) -> Value {
    match level {
        crate::SummaryLevel::Detailed => detailed_summary_value(graph, token_budget),
        crate::SummaryLevel::Community => community_summary_value(graph, communities, token_budget),
        crate::SummaryLevel::Architecture => architecture_summary_value(graph, token_budget),
    }
}

fn detailed_summary_value(graph: &KnowledgeGraph, token_budget: usize) -> Value {
    let mut nodes = graph.node_ids();
    nodes.sort();
    let node_limit = output_row_limit(token_budget, 20, 10, 500);
    let edge_limit = output_row_limit(token_budget, 10, 20, 1000);
    let all_edges = graph.edges_with_endpoints();
    let edges: Vec<(String, String)> = all_edges
        .iter()
        .take(edge_limit)
        .map(|(source, target, _)| ((*source).to_string(), (*target).to_string()))
        .collect();

    let mut value = subgraph_to_value(graph, &nodes[..nodes.len().min(node_limit)], &edges);
    value["kind"] = json!("smart_summary_detailed");
    value["total_nodes"] = json!(graph.node_count());
    value["total_edges"] = json!(graph.edge_count());
    value
}

fn community_summary_value(
    graph: &KnowledgeGraph,
    communities: &HashMap<usize, Vec<String>>,
    token_budget: usize,
) -> Value {
    let mut sorted_cids: Vec<usize> = communities.keys().copied().collect();
    sorted_cids.sort_unstable();
    let community_limit = output_row_limit(token_budget, 10, 5, 250);

    let community_values: Vec<Value> = sorted_cids
        .iter()
        .take(community_limit)
        .filter_map(|cid| {
            let members = communities.get(cid)?;
            let (rep_id, rep_degree) = members
                .iter()
                .map(|id| (id.as_str(), graph.degree(id)))
                .max_by_key(|(_, degree)| *degree)
                .unwrap_or(("", 0));
            let rep_label = graph
                .get_node(rep_id)
                .map(|node| node.label.as_str())
                .unwrap_or(rep_id);
            Some(json!({
                "id": cid,
                "node_count": members.len(),
                "representative_id": rep_id,
                "representative": rep_label,
                "degree": rep_degree,
            }))
        })
        .collect();

    let mut node_cid: HashMap<&str, usize> = HashMap::new();
    for (&cid, members) in communities {
        for member in members {
            node_cid.insert(member.as_str(), cid);
        }
    }

    let mut cross_edges: HashMap<(usize, usize), usize> = HashMap::new();
    for (source, target, _) in graph.edges_with_endpoints() {
        let source_community = node_cid.get(source).copied().unwrap_or(usize::MAX);
        let target_community = node_cid.get(target).copied().unwrap_or(usize::MAX);
        if source_community != target_community
            && source_community != usize::MAX
            && target_community != usize::MAX
        {
            let key = if source_community < target_community {
                (source_community, target_community)
            } else {
                (target_community, source_community)
            };
            *cross_edges.entry(key).or_default() += 1;
        }
    }
    let mut sorted_cross: Vec<_> = cross_edges.into_iter().collect();
    sorted_cross.sort_by_key(|(_, count)| std::cmp::Reverse(*count));
    let dependency_limit = output_row_limit(token_budget, 40, 5, 50);
    let dependencies: Vec<Value> = sorted_cross
        .iter()
        .take(dependency_limit)
        .map(|((from, to), edges)| {
            json!({
                "from": from,
                "to": to,
                "edges": edges,
            })
        })
        .collect();

    json!({
        "kind": "smart_summary_community",
        "community_count": communities.len(),
        "node_count": graph.node_count(),
        "communities": community_values,
        "dependencies": dependencies,
    })
}

fn architecture_summary_value(graph: &KnowledgeGraph, token_budget: usize) -> Value {
    let mut dir_nodes: HashMap<String, Vec<&str>> = HashMap::new();
    for node in graph.nodes() {
        let dir = std::path::Path::new(&node.source_file)
            .parent()
            .and_then(|path| path.to_str())
            .unwrap_or(".")
            .to_string();
        dir_nodes.entry(dir).or_default().push(node.id.as_str());
    }

    let mut node_dir: HashMap<&str, &str> = HashMap::new();
    for (dir, nodes) in &dir_nodes {
        for &node_id in nodes {
            node_dir.insert(node_id, dir.as_str());
        }
    }

    let package_limit = output_row_limit(token_budget, 40, 5, 60);
    let mut sorted_dirs: Vec<_> = dir_nodes.iter().collect();
    sorted_dirs.sort_by_key(|(_, nodes)| std::cmp::Reverse(nodes.len()));
    let packages: Vec<Value> = sorted_dirs
        .iter()
        .take(package_limit)
        .map(|(path, nodes)| {
            json!({
                "path": path,
                "node_count": nodes.len(),
            })
        })
        .collect();

    let mut dir_edges: HashMap<(&str, &str), usize> = HashMap::new();
    for (source, target, _) in graph.edges_with_endpoints() {
        let source_dir = node_dir.get(source).copied().unwrap_or("?");
        let target_dir = node_dir.get(target).copied().unwrap_or("?");
        if source_dir != target_dir {
            let key = if source_dir < target_dir {
                (source_dir, target_dir)
            } else {
                (target_dir, source_dir)
            };
            *dir_edges.entry(key).or_default() += 1;
        }
    }
    let mut sorted_deps: Vec<_> = dir_edges.into_iter().collect();
    sorted_deps.sort_by_key(|(_, count)| std::cmp::Reverse(*count));
    let dependency_limit = output_row_limit(token_budget, 40, 5, 50);
    let dependencies: Vec<Value> = sorted_deps
        .iter()
        .take(dependency_limit)
        .map(|((from, to), edges)| {
            json!({
                "from": from,
                "to": to,
                "edges": edges,
            })
        })
        .collect();

    json!({
        "kind": "smart_summary_architecture",
        "package_count": dir_nodes.len(),
        "node_count": graph.node_count(),
        "packages": packages,
        "dependencies": dependencies,
    })
}

pub(crate) fn handle_query_graph(
    graph: &KnowledgeGraph,
    semantic: Option<&SemanticState>,
    args: &Value,
) -> Value {
    let question = args["question"].as_str().unwrap_or("");
    let budget = args["budget"].as_u64().unwrap_or(2000) as usize;

    if question.is_empty() {
        return tool_result_error("Missing required parameter: question");
    }

    let terms = query_search_terms(question);
    let identifier_query = query_has_code_identifier(question);

    if terms.is_empty() {
        return tool_result_text("No meaningful search terms found in the question.");
    }

    let scored = score_nodes(graph, &terms);
    let mut start: Vec<String> = scored
        .iter()
        .map(|(_, id)| id)
        .filter(|id| query_seed_candidate(graph, id, &terms))
        .take(6)
        .cloned()
        .collect();

    let mut warnings = Vec::new();
    if let Some(semantic) = semantic {
        match semantic.query(graph, question, 8) {
            Ok(matches) => {
                start.extend(
                    matches
                        .into_iter()
                        .filter(|m| semantic_seed_candidate(graph, m, &terms, identifier_query))
                        .map(|m| m.node_id),
                );
            }
            Err(err) => {
                debug!("semantic query unavailable, falling back to lexical query: {err}");
                warnings.push(format!(
                    "semantic query unavailable; falling back to lexical query: {err}"
                ));
            }
        }
    }

    start = dedupe_start_nodes(start, 8);

    if start.is_empty() {
        return tool_result_text("No matching nodes found for the given question.");
    }

    let context = query_context(graph, &start, &terms, 2, QueryLimits::for_budget(budget));
    let format = output_format(args);
    if format != GraphifyOutputFormat::Text {
        return tool_result_value(
            &query_context_to_value(graph, &context, &start, &terms, &warnings),
            format,
        );
    }

    let mut text = String::new();
    for warning in &warnings {
        text.push_str(warning);
        text.push('\n');
    }
    text.push_str(&subgraph_to_text(
        graph,
        &context.nodes,
        &context.edges,
        budget,
    ));
    if context.truncated {
        text.push_str(&format!(
            "\n... (bounded graph context: omitted {} node(s), {} edge(s))\n",
            context.omitted_nodes, context.omitted_edges
        ));
    }
    tool_result_text(&text)
}

fn dedupe_start_nodes(nodes: Vec<String>, limit: usize) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for node in nodes {
        if seen.insert(node.clone()) {
            out.push(node);
        }
        if out.len() >= limit {
            break;
        }
    }
    out
}

pub(crate) fn handle_get_node(graph: &KnowledgeGraph, args: &Value) -> Value {
    let node_id = args["node_id"].as_str().unwrap_or("");
    if node_id.is_empty() {
        return tool_result_error("Missing required parameter: node_id");
    }

    match graph.get_node(node_id) {
        Some(node) => {
            let neighbors = graph.neighbor_ids(node_id);
            let degree = graph.degree(node_id);
            let result = json!({
                "id": node.id,
                "label": node.label,
                "node_type": node.node_type,
                "source_file": node.source_file,
                "source_location": node.source_location,
                "community": node.community,
                "degree": degree,
                "neighbors": neighbors,
            });
            tool_result_value(&result, output_format(args))
        }
        None => tool_result_error(&format!("Node not found: {node_id}")),
    }
}

pub(crate) fn handle_get_neighbors(graph: &KnowledgeGraph, args: &Value) -> Value {
    let node_id = args["node_id"].as_str().unwrap_or("");
    let depth = args["depth"].as_u64().unwrap_or(1) as usize;

    if node_id.is_empty() {
        return tool_result_error("Missing required parameter: node_id");
    }

    if graph.get_node(node_id).is_none() {
        return tool_result_error(&format!("Node not found: {node_id}"));
    }

    let (nodes, edges) = bfs(graph, &[node_id.to_string()], depth);

    let mut neighbor_info: Vec<Value> = Vec::new();
    for nid in &nodes {
        if nid == node_id {
            continue; // skip the start node
        }
        if let Some(node) = graph.get_node(nid) {
            let edge_count = edges
                .iter()
                .filter(|(s, t)| (s == node_id && t == nid) || (s == nid && t == node_id))
                .count();

            neighbor_info.push(json!({
                "id": node.id,
                "label": node.label,
                "node_type": node.node_type,
                "source_file": node.source_file,
                "community": node.community,
                "edge_count": edge_count,
            }));
        }
    }

    let result = json!({
        "node_id": node_id,
        "depth": depth,
        "neighbor_count": neighbor_info.len(),
        "neighbors": neighbor_info,
    });

    tool_result_value(&result, output_format(args))
}

pub(crate) fn handle_get_community(graph: &KnowledgeGraph, args: &Value) -> Value {
    let community_id = match args["community_id"].as_u64() {
        Some(id) => id as usize,
        None => return tool_result_error("Missing required parameter: community_id"),
    };

    let mut members: Vec<Value> = Vec::new();
    for node_id in graph.node_ids() {
        if let Some(node) = graph.get_node(&node_id)
            && node.community == Some(community_id)
        {
            members.push(json!({
                "id": node.id,
                "label": node.label,
                "node_type": node.node_type,
                "source_file": node.source_file,
                "degree": graph.degree(&node_id),
            }));
        }
    }

    if members.is_empty() {
        return tool_result_error(&format!("Community not found or empty: {community_id}"));
    }

    members.sort_by(|a, b| {
        let da = a["degree"].as_u64().unwrap_or(0);
        let db = b["degree"].as_u64().unwrap_or(0);
        db.cmp(&da)
    });

    let result = json!({
        "community_id": community_id,
        "member_count": members.len(),
        "members": members,
    });

    tool_result_value(&result, output_format(args))
}

pub(crate) fn handle_god_nodes(graph: &KnowledgeGraph, args: &Value) -> Value {
    let top_n = args["top_n"].as_u64().unwrap_or(10) as usize;

    let gods = graphify_analyze::god_nodes(graph, top_n);

    let result: Vec<Value> = gods
        .iter()
        .enumerate()
        .map(|(i, g)| {
            json!({
                "rank": i + 1,
                "id": g.id,
                "label": g.label,
                "degree": g.degree,
                "community": g.community,
            })
        })
        .collect();

    let output = json!({
        "top_n": top_n,
        "god_nodes": result,
    });

    tool_result_value(&output, output_format(args))
}

pub(crate) fn handle_graph_stats(graph: &KnowledgeGraph, args: &Value) -> Value {
    let stats = graph_stats(graph);
    tool_result_value(&json!(stats), output_format(args))
}

pub(crate) fn handle_shortest_path(graph: &KnowledgeGraph, args: &Value) -> Value {
    let source = args["source"].as_str().unwrap_or("");
    let target = args["target"].as_str().unwrap_or("");

    if source.is_empty() || target.is_empty() {
        return tool_result_error("Missing required parameters: source and target");
    }

    if graph.get_node(source).is_none() {
        return tool_result_error(&format!("Source node not found: {source}"));
    }
    if graph.get_node(target).is_none() {
        return tool_result_error(&format!("Target node not found: {target}"));
    }

    if source == target {
        let node = graph.get_node(source).unwrap();
        let result = json!({
            "source": source,
            "target": target,
            "path_length": 0,
            "path": [{"id": node.id, "label": node.label}],
        });
        return tool_result_value(&result, output_format(args));
    }

    let mut visited: HashSet<String> = HashSet::new();
    let mut parent: HashMap<String, String> = HashMap::new();
    let mut queue: VecDeque<String> = VecDeque::new();

    visited.insert(source.to_string());
    queue.push_back(source.to_string());

    let mut found = false;
    while let Some(current) = queue.pop_front() {
        if current == target {
            found = true;
            break;
        }
        for neighbor in graph.neighbor_ids(&current) {
            if !visited.contains(&neighbor) {
                visited.insert(neighbor.clone());
                parent.insert(neighbor.clone(), current.clone());
                queue.push_back(neighbor);
            }
        }
    }

    if !found {
        return tool_result_text(&format!(
            "No path found between '{source}' and '{target}'. They may be in disconnected components."
        ));
    }

    let mut path = vec![target.to_string()];
    let mut current = target.to_string();
    while let Some(p) = parent.get(&current) {
        path.push(p.clone());
        current = p.clone();
    }
    path.reverse();

    let path_nodes: Vec<Value> = path
        .iter()
        .map(|id| {
            let label = graph.get_node(id).map(|n| n.label.as_str()).unwrap_or(id);
            json!({"id": id, "label": label})
        })
        .collect();

    let result = json!({
        "source": source,
        "target": target,
        "path_length": path.len() - 1,
        "path": path_nodes,
    });

    tool_result_value(&result, output_format(args))
}

pub(crate) fn handle_find_all_paths(graph: &KnowledgeGraph, args: &Value) -> Value {
    let source = args["source"].as_str().unwrap_or("");
    let target = args["target"].as_str().unwrap_or("");
    let max_length = args["max_length"].as_u64().unwrap_or(4) as usize;

    if source.is_empty() || target.is_empty() {
        return tool_result_error("Missing required parameters: source and target");
    }
    if graph.get_node(source).is_none() {
        return tool_result_error(&format!("Source node not found: {source}"));
    }
    if graph.get_node(target).is_none() {
        return tool_result_error(&format!("Target node not found: {target}"));
    }

    let paths = crate::all_simple_paths(graph, source, target, max_length);

    let paths_json: Vec<Value> = paths
        .iter()
        .map(|path| {
            let nodes: Vec<Value> = path
                .iter()
                .map(|id| {
                    let label = graph.get_node(id).map(|n| n.label.as_str()).unwrap_or(id);
                    json!({"id": id, "label": label})
                })
                .collect();
            json!({
                "length": path.len() - 1,
                "nodes": nodes
            })
        })
        .collect();

    let result = json!({
        "source": source,
        "target": target,
        "max_length": max_length,
        "path_count": paths_json.len(),
        "paths": paths_json,
    });

    tool_result_value(&result, output_format(args))
}

pub(crate) fn handle_weighted_path(graph: &KnowledgeGraph, args: &Value) -> Value {
    let source = args["source"].as_str().unwrap_or("");
    let target = args["target"].as_str().unwrap_or("");
    let min_confidence = args["min_confidence"].as_f64().unwrap_or(0.0);

    if source.is_empty() || target.is_empty() {
        return tool_result_error("Missing required parameters: source and target");
    }
    if graph.get_node(source).is_none() {
        return tool_result_error(&format!("Source node not found: {source}"));
    }
    if graph.get_node(target).is_none() {
        return tool_result_error(&format!("Target node not found: {target}"));
    }

    match crate::dijkstra_path(graph, source, target, min_confidence) {
        Some((path, total_cost, edge_details)) => {
            let path_nodes: Vec<Value> = path
                .iter()
                .map(|id| {
                    let label = graph.get_node(id).map(|n| n.label.as_str()).unwrap_or(id);
                    json!({"id": id, "label": label})
                })
                .collect();

            let edges: Vec<Value> = edge_details
                .iter()
                .map(|(from, to, cost, relation)| {
                    json!({
                        "from": from,
                        "to": to,
                        "cost": cost,
                        "relation": relation
                    })
                })
                .collect();

            let result = json!({
                "source": source,
                "target": target,
                "min_confidence": min_confidence,
                "total_cost": total_cost,
                "path_length": path.len() - 1,
                "path": path_nodes,
                "edges": edges,
            });
            tool_result_value(&result, output_format(args))
        }
        None => tool_result_text(&format!(
            "No path found between {source} and {target} with min_confidence {min_confidence}"
        )),
    }
}

pub(crate) fn handle_community_bridges(graph: &KnowledgeGraph, args: &Value) -> Value {
    let mut communities: HashMap<usize, Vec<String>> = HashMap::new();
    for node_id in graph.node_ids() {
        if let Some(node) = graph.get_node(&node_id)
            && let Some(cid) = node.community
        {
            communities.entry(cid).or_default().push(node_id);
        }
    }

    let top_n = args["top_n"].as_u64().unwrap_or(10) as usize;
    let bridges = graphify_analyze::community_bridges(graph, &communities, top_n);

    let result: Vec<Value> = bridges
        .iter()
        .map(|b| {
            json!({
                "id": b.id,
                "label": b.label,
                "total_edges": b.total_edges,
                "cross_community_edges": b.cross_community_edges,
                "bridge_ratio": format!("{:.2}", b.bridge_ratio),
                "communities_touched": b.communities_touched,
            })
        })
        .collect();

    let output = json!({
        "top_n": top_n,
        "bridge_count": result.len(),
        "bridges": result,
    });

    tool_result_value(&output, output_format(args))
}

pub(crate) fn handle_graph_diff(graph: &KnowledgeGraph, args: &Value) -> Value {
    let other_path = args["other_graph"].as_str().unwrap_or("");
    if other_path.is_empty() {
        return tool_result_error("Missing required parameter: other_graph");
    }

    let path = std::path::Path::new(other_path);
    if let Err(e) = graphify_security::validate_graph_path(other_path) {
        return tool_result_error(&format!("Invalid path: {e}"));
    }

    let other_graph = match crate::load_graph(path) {
        Ok(g) => g,
        Err(e) => return tool_result_error(&format!("Failed to load graph: {e}")),
    };

    let diff = graphify_analyze::graph_diff(graph, &other_graph);
    tool_result_value(
        &serde_json::to_value(diff).unwrap_or_else(|_| json!({})),
        output_format(args),
    )
}

pub(crate) fn handle_pagerank(graph: &KnowledgeGraph, args: &Value) -> Value {
    let top_n = args["top_n"].as_u64().unwrap_or(10) as usize;
    let results = graphify_analyze::pagerank(graph, top_n, 0.85, 20);
    tool_result_value(
        &serde_json::to_value(results).unwrap_or_else(|_| json!([])),
        output_format(args),
    )
}

pub(crate) fn handle_detect_cycles(graph: &KnowledgeGraph, args: &Value) -> Value {
    let max_cycles = args["max_cycles"].as_u64().unwrap_or(10) as usize;
    let cycles = graphify_analyze::detect_cycles(graph, max_cycles);
    if cycles.is_empty() {
        tool_result_text("No dependency cycles detected.")
    } else {
        tool_result_value(
            &serde_json::to_value(cycles).unwrap_or_else(|_| json!([])),
            output_format(args),
        )
    }
}

pub(crate) fn handle_smart_summary(graph: &KnowledgeGraph, args: &Value) -> Value {
    let level_str = args["level"].as_str().unwrap_or("community");
    let budget = args["budget"].as_u64().unwrap_or(2000) as usize;

    let level = match level_str {
        "detailed" => crate::SummaryLevel::Detailed,
        "architecture" => crate::SummaryLevel::Architecture,
        _ => crate::SummaryLevel::Community,
    };

    let mut communities: HashMap<usize, Vec<String>> = HashMap::new();
    for node in graph.nodes() {
        let cid = node.community.unwrap_or(0);
        communities.entry(cid).or_default().push(node.id.clone());
    }

    let format = output_format(args);
    if format != GraphifyOutputFormat::Text {
        return tool_result_value(
            &smart_summary_to_value(graph, &communities, level, budget),
            format,
        );
    }

    tool_result_text(&crate::smart_summary(graph, &communities, level, budget))
}

pub(crate) fn handle_find_similar(graph: &KnowledgeGraph, args: &Value) -> Value {
    let top_n = args["top_n"].as_u64().unwrap_or(10) as usize;
    let embeddings = graphify_analyze::embedding::compute_embeddings(graph, 64, 10, 40);
    let pairs = graphify_analyze::embedding::find_similar(graph, &embeddings, top_n);
    if pairs.is_empty() {
        tool_result_text("No structurally similar node pairs found.")
    } else {
        tool_result_value(
            &serde_json::to_value(pairs).unwrap_or_else(|_| json!([])),
            output_format(args),
        )
    }
}

pub(crate) fn handle_semantic_query(
    graph: &KnowledgeGraph,
    semantic: Option<&SemanticState>,
    args: &Value,
) -> Value {
    let question = args["question"].as_str().unwrap_or("");
    let top_n = args["top_n"].as_u64().unwrap_or(10) as usize;
    if question.is_empty() {
        return tool_result_error("Missing required parameter: question");
    }
    let Some(semantic) = semantic else {
        return tool_result_error(
            "Semantic index not loaded. Run `graphify-rs build --embed` to create .graphify/semantic-index.json.",
        );
    };

    match semantic.query(graph, question, top_n) {
        Ok(matches) if matches.is_empty() => tool_result_text("No semantic matches found."),
        Ok(matches) => tool_result_value(
            &serde_json::to_value(matches).unwrap_or_else(|_| json!([])),
            output_format(args),
        ),
        Err(err) => tool_result_error(&format!("{err}")),
    }
}
