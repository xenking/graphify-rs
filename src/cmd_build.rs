//! Build command: detect → extract → build → cluster → analyze → export pipeline.

use anyhow::{Context, Result};
use colored::Colorize;
use indicatif::{ProgressBar, ProgressStyle};
use rayon::prelude::*;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::{Verbosity, info_print, llm, verbose_print};

/// Full build pipeline: detect -> extract (with cache) -> build -> cluster -> analyze -> export
#[allow(clippy::too_many_arguments)]
pub async fn cmd_build(
    path: &str,
    output: &str,
    no_llm: bool,
    llm_cli: Option<&llm::LlmCliConfig>,
    code_only: bool,
    update: bool,
    formats: &[String],
    verb: Verbosity,
    jobs: Option<usize>,
    max_viz_nodes: Option<usize>,
    embed: bool,
    embedding_provider: &str,
    embedding_model: &str,
    anthropic_semantic: bool,
) -> Result<()> {
    let root = PathBuf::from(path);
    let output_dir = PathBuf::from(output);
    let cache_dir = output_dir.join("cache");

    let all_formats = [
        "json", "html", "graphml", "cypher", "svg", "wiki", "obsidian", "report", "context",
    ];
    let selected: Vec<&str> = if formats.is_empty() {
        all_formats.to_vec()
    } else {
        formats.iter().map(std::string::String::as_str).collect()
    };
    let should_export = |name: &str| selected.iter().any(|s| s.eq_ignore_ascii_case(name));

    let detection = step_detect(&root, &output_dir, update, verb)?;

    let mut extractions = step_extract_ast(&root, &cache_dir, &detection, code_only, verb)?;

    if !code_only {
        step_extract_documents(&root, &detection, &mut extractions, verb);
    }

    let doc_files = if code_only {
        Vec::new()
    } else {
        collect_doc_files(&root, &detection)
    };

    let mut active_llm_dirs = HashSet::new();
    if let Some(cli) = llm_cli {
        active_llm_dirs.insert(llm::provider_cache_dir(&output_dir, &cli.provider));
    }
    if anthropic_semantic && !no_llm {
        active_llm_dirs.insert(llm::provider_cache_dir(&output_dir, "anthropic"));
        active_llm_dirs.insert(cache_dir.clone());
    }

    if !no_llm && !code_only {
        if let Some(cli) = llm_cli
            && !doc_files.is_empty()
        {
            extractions.extend(
                run_cli_semantic_extraction(&doc_files, &root, &output_dir, cli, verb, jobs).await,
            );
        }
        if anthropic_semantic && !doc_files.is_empty() {
            if let Ok(api_key) = std::env::var("ANTHROPIC_API_KEY") {
                extractions.extend(
                    run_anthropic_semantic_extraction(
                        &doc_files,
                        &root,
                        &output_dir,
                        &api_key,
                        verb,
                        jobs,
                    )
                    .await,
                );
            } else {
                info_print!(
                    verb,
                    "  {} --anthropic-semantic was requested but ANTHROPIC_API_KEY is not set; local document context was still indexed",
                    "ℹ".blue()
                );
            }
        }
    }

    if !doc_files.is_empty() {
        let preserved =
            llm::load_preserved_extractions(&doc_files, &root, &output_dir, &active_llm_dirs);
        if !preserved.is_empty() {
            let stale = preserved
                .iter()
                .filter(|entry| entry.stale_preserved)
                .count();
            info_print!(
                verb,
                "  Preserved {} cached LLM extraction(s){}",
                preserved.len().to_string().cyan(),
                if stale > 0 {
                    format!(" ({} stale by source hash)", stale)
                } else {
                    String::new()
                }
            );
            extractions.extend(preserved.into_iter().map(|entry| entry.extraction));
        }
    }

    info_print!(verb, "  {} graph...", "Building".cyan());
    let mut graph = graphify_build::build(&extractions).context("Failed to build graph")?;
    info_print!(
        verb,
        "  Graph: {} nodes, {} edges",
        graph.node_count().to_string().bold(),
        graph.edge_count().to_string().bold()
    );

    let ClusterResult {
        communities,
        cohesion,
        community_labels,
    } = step_cluster(&mut graph, verb);

    info_print!(verb, "  {} graph...", "Analyzing".cyan());
    let god_list = graphify_analyze::god_nodes(&graph, 10);
    let surprise_list = graphify_analyze::surprising_connections(&graph, &communities, 5);
    let questions = graphify_analyze::suggest_questions(&graph, &communities, &community_labels, 7);

    if embed {
        info_print!(
            verb,
            "  {} semantic index with {}...",
            "Embedding".cyan(),
            format!("{embedding_provider}:{embedding_model}")
        );
        let (provider, model) = if let Some((provider, model)) = embedding_model.split_once(':') {
            (provider, model)
        } else {
            (embedding_provider, embedding_model)
        };
        let index = graphify_embed::build_semantic_index(&graph, Some(&root), provider, model)
            .with_context(|| format!("Failed to build semantic index with {provider}:{model}"))?;
        let index_path = output_dir.join(graphify_embed::DEFAULT_INDEX_FILE);
        graphify_embed::write_index(&index, &index_path)
            .with_context(|| format!("Failed to write {}", index_path.display()))?;
        info_print!(
            verb,
            "  Wrote {} ({} nodes, dim {})",
            index_path.display().to_string().dimmed(),
            index.nodes.len(),
            index.dim
        );
    }

    step_export(
        &graph,
        &communities,
        &cohesion,
        &community_labels,
        &god_list,
        &surprise_list,
        &questions,
        &detection,
        &output_dir,
        path,
        max_viz_nodes,
        should_export,
        verb,
    )?;

    info_print!(
        verb,
        "\n{} Output in {}",
        "✓ Done!".green().bold(),
        output_dir.display()
    );

    Ok(())
}

fn step_detect(
    root: &Path,
    output_dir: &Path,
    update: bool,
    verb: Verbosity,
) -> Result<graphify_detect::DetectResult> {
    info_print!(verb, "  {} files...", "Detecting".cyan());
    let detection = if update {
        let manifest_path = output_dir.join(".graphify_manifest.json");
        graphify_detect::detect_incremental(root, Some(manifest_path.to_str().unwrap_or("")))
    } else {
        graphify_detect::detect(root)
    };
    let n_code = detection
        .files
        .get(&graphify_detect::FileType::Code)
        .map_or(0, std::vec::Vec::len);
    let n_doc = detection
        .files
        .get(&graphify_detect::FileType::Document)
        .map_or(0, std::vec::Vec::len);
    let n_paper = detection
        .files
        .get(&graphify_detect::FileType::Paper)
        .map_or(0, std::vec::Vec::len);
    let n_image = detection
        .files
        .get(&graphify_detect::FileType::Image)
        .map_or(0, std::vec::Vec::len);
    info_print!(
        verb,
        "  Found {} files ({} code, {} doc, {} paper, {} image) · ~{} words",
        detection.total_files.to_string().bold(),
        n_code.to_string().green(),
        n_doc.to_string().blue(),
        n_paper.to_string().magenta(),
        n_image.to_string().yellow(),
        detection.total_words
    );
    if let Some(ref warning) = detection.warning {
        info_print!(verb, "  {} {}", "⚠".yellow(), warning.yellow());
    }
    if !detection.skipped_sensitive.is_empty() {
        info_print!(
            verb,
            "  {} Skipped {} sensitive file(s)",
            "⚠".yellow(),
            detection.skipped_sensitive.len()
        );
    }
    Ok(detection)
}

fn step_extract_ast(
    root: &Path,
    cache_dir: &Path,
    detection: &graphify_detect::DetectResult,
    code_only: bool,
    verb: Verbosity,
) -> Result<Vec<graphify_core::model::ExtractionResult>> {
    let code_files: Vec<PathBuf> = detection
        .files
        .get(&graphify_detect::FileType::Code)
        .map(|v| v.iter().map(|f| root.join(f)).collect())
        .unwrap_or_default();

    if code_files.is_empty() && code_only {
        info_print!(verb, "  No code files found. Nothing to extract.");
        return Ok(vec![]);
    }

    info_print!(
        verb,
        "  {} AST from {} code files...",
        "Extracting".cyan(),
        code_files.len()
    );
    let cache_hits = AtomicUsize::new(0);
    let extract_errors = AtomicUsize::new(0);

    let pb = if verb.is_quiet() {
        None
    } else {
        let pb = ProgressBar::new(code_files.len() as u64);
        pb.set_style(
            ProgressStyle::with_template("  {bar:40.cyan/dim} {pos}/{len} files ({eta} remaining)")
                .unwrap()
                .progress_chars("██░"),
        );
        Some(pb)
    };

    let file_results: Vec<graphify_core::model::ExtractionResult> = code_files
        .par_iter()
        .map(|file_path| {
            if let Some(ref pb) = pb {
                pb.set_message(
                    file_path
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string(),
                );
            }
            if let Some(cached) = graphify_cache::load_cached_from::<
                graphify_core::model::ExtractionResult,
            >(file_path, root, cache_dir)
            {
                cache_hits.fetch_add(1, Ordering::Relaxed);
                if let Some(ref pb) = pb {
                    pb.inc(1);
                }
                return cached;
            }
            let result = if let Ok(fresh) =
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    graphify_extract::extract(std::slice::from_ref(file_path))
                })) {
                let _ = graphify_cache::save_cached_to(file_path, &fresh, root, cache_dir);
                fresh
            } else {
                extract_errors.fetch_add(1, Ordering::Relaxed);
                graphify_core::model::ExtractionResult::default()
            };
            if let Some(ref pb) = pb {
                pb.inc(1);
            }
            result
        })
        .collect();

    let mut ast_result = graphify_core::model::ExtractionResult::default();
    for partial in file_results {
        ast_result.nodes.extend(partial.nodes);
        ast_result.edges.extend(partial.edges);
        ast_result.hyperedges.extend(partial.hyperedges);
    }

    if let Some(pb) = pb {
        pb.finish_and_clear();
    }
    let cache_hits = cache_hits.load(Ordering::Relaxed);
    let extract_errors = extract_errors.load(Ordering::Relaxed);
    if cache_hits > 0 {
        info_print!(
            verb,
            "  Cache: {} hits, {} extracted fresh",
            cache_hits.to_string().green(),
            (code_files.len() - cache_hits).to_string().cyan()
        );
    }
    if extract_errors > 0 {
        info_print!(
            verb,
            "  {} {} file(s) had extraction errors (skipped)",
            "⚠".yellow(),
            extract_errors
        );
    }
    info_print!(
        verb,
        "  Pass 1 (AST): {} nodes, {} edges",
        ast_result.nodes.len().to_string().bold(),
        ast_result.edges.len().to_string().bold()
    );

    Ok(vec![ast_result])
}

fn step_extract_documents(
    root: &Path,
    detection: &graphify_detect::DetectResult,
    extractions: &mut Vec<graphify_core::model::ExtractionResult>,
    verb: Verbosity,
) {
    let doc_files: Vec<PathBuf> = detection
        .files
        .get(&graphify_detect::FileType::Document)
        .into_iter()
        .chain(detection.files.get(&graphify_detect::FileType::Paper))
        .flat_map(|v| v.iter().map(|f| root.join(f)))
        .collect();

    if doc_files.is_empty() {
        return;
    }

    info_print!(
        verb,
        "  {} local document context from {} doc/paper files...",
        "Extracting".cyan(),
        doc_files.len()
    );
    let doc_result = graphify_extract::doc_extract::extract_documents(&doc_files);
    info_print!(
        verb,
        "  Pass 1b (docs): {} nodes, {} edges",
        doc_result.nodes.len().to_string().bold(),
        doc_result.edges.len().to_string().bold()
    );
    extractions.push(doc_result);
}

fn collect_doc_files(root: &Path, detection: &graphify_detect::DetectResult) -> Vec<PathBuf> {
    detection
        .files
        .get(&graphify_detect::FileType::Document)
        .into_iter()
        .chain(detection.files.get(&graphify_detect::FileType::Paper))
        .flat_map(|v| v.iter().map(|f| root.join(f)))
        .collect()
}

fn semantic_file_type(path: &Path) -> &'static str {
    if path.extension().and_then(|e| e.to_str()) == Some("pdf") {
        "paper"
    } else {
        "document"
    }
}

async fn run_cli_semantic_extraction(
    doc_files: &[PathBuf],
    root: &Path,
    output_dir: &Path,
    cli: &llm::LlmCliConfig,
    verb: Verbosity,
    jobs: Option<usize>,
) -> Vec<graphify_core::model::ExtractionResult> {
    info_print!(
        verb,
        "  {} via local LLM CLI '{}' on {} doc/paper files...",
        "Semantic extraction".cyan(),
        cli.provider,
        doc_files.len()
    );
    let cache_dir = llm::provider_cache_dir(output_dir, &cli.provider);
    let concurrency = jobs.unwrap_or(4).min(8);
    let sem = std::sync::Arc::new(tokio::sync::Semaphore::new(concurrency));
    let mut results = Vec::new();

    let pb_sem = if verb.is_quiet() {
        None
    } else {
        let pb = ProgressBar::new(doc_files.len() as u64);
        pb.set_style(
            ProgressStyle::with_template("  {bar:40.green/dim} {pos}/{len} docs ({eta} remaining)")
                .unwrap()
                .progress_chars("██░"),
        );
        Some(pb)
    };

    let mut handles = Vec::new();
    for doc_path in doc_files {
        if let Some(cached) = llm::load_current_entry(doc_path, root, &cache_dir, cli) {
            let _ = llm::save_entry(doc_path, root, &cache_dir, cli, &cached);
            results.push(cached);
            if let Some(ref pb) = pb_sem {
                pb.inc(1);
            }
            continue;
        }

        let content = match std::fs::read_to_string(doc_path) {
            Ok(c) => c,
            Err(_) => {
                if let Some(ref pb) = pb_sem {
                    pb.inc(1);
                }
                continue;
            }
        };
        let existing = llm::load_latest_for_prompt(doc_path, root, &cache_dir);
        let fallback = llm::load_current_or_latest_for_preservation(doc_path, root, &cache_dir);
        let doc_p = doc_path.clone();
        let root_p = root.to_path_buf();
        let command = cli.command.clone();
        let provider = cli.provider.clone();
        let file_type = semantic_file_type(doc_path).to_string();
        let sem_clone = sem.clone();
        let handle = tokio::spawn(async move {
            let _permit = sem_clone
                .acquire()
                .await
                .map_err(|e| anyhow::anyhow!("semaphore closed: {e}"))?;
            tokio::task::spawn_blocking(move || {
                match graphify_extract::semantic::extract_semantic_with_cli(
                    &doc_p,
                    &content,
                    &file_type,
                    &command,
                    existing.as_ref(),
                ) {
                    Ok(result) => Ok((doc_p, root_p, provider, command, result, false)),
                    Err(err) => {
                        if let Some(fallback) = fallback {
                            Ok((doc_p, root_p, provider, command, fallback.extraction, true))
                        } else {
                            Err(err)
                        }
                    }
                }
            })
            .await
            .map_err(|e| anyhow::anyhow!("LLM command task join error: {e}"))?
        });
        handles.push(handle);
    }

    for handle in handles {
        match handle.await {
            Ok(Ok((doc_p, root_p, provider, command, sem_result, used_fallback))) => {
                verbose_print!(
                    verb,
                    "    {} → {} nodes, {} edges{}",
                    doc_p.file_name().unwrap_or_default().to_string_lossy(),
                    sem_result.nodes.len(),
                    sem_result.edges.len(),
                    if used_fallback {
                        " (cached fallback)"
                    } else {
                        ""
                    }
                );
                if !used_fallback {
                    let _ = llm::save_entry(
                        &doc_p,
                        &root_p,
                        &cache_dir,
                        &llm::LlmCliConfig { provider, command },
                        &sem_result,
                    );
                }
                results.push(sem_result);
            }
            Ok(Err(e)) => {
                verbose_print!(verb, "    {} semantic extraction: {}", "⚠".yellow(), e);
            }
            Err(e) => {
                verbose_print!(verb, "    {} task join error: {}", "⚠".yellow(), e);
            }
        }
        if let Some(ref pb) = pb_sem {
            pb.inc(1);
        }
    }
    if let Some(pb) = pb_sem {
        pb.finish_and_clear();
    }

    results
}

async fn run_anthropic_semantic_extraction(
    doc_files: &[PathBuf],
    root: &Path,
    output_dir: &Path,
    api_key: &str,
    verb: Verbosity,
    jobs: Option<usize>,
) -> Vec<graphify_core::model::ExtractionResult> {
    info_print!(
        verb,
        "  {} via legacy Anthropic on {} doc/paper files...",
        "Semantic extraction".cyan(),
        doc_files.len()
    );
    let cache_dir = llm::provider_cache_dir(output_dir, "anthropic");
    let legacy_cache_dir = output_dir.join("cache");
    let concurrency = jobs.unwrap_or(4).min(8);
    let sem = std::sync::Arc::new(tokio::sync::Semaphore::new(concurrency));
    let mut results = Vec::new();
    let config = match graphify_extract::semantic::LLMProviderConfig::resolve(
        &graphify_extract::semantic::LLMConfigRaw {
            provider: "anthropic".into(),
            model: "claude-sonnet-4.6".into(),
            anthropic_api_key: Some(api_key.to_string()),
            ..Default::default()
        },
    ) {
        Ok(config) => config,
        Err(err) => {
            verbose_print!(
                verb,
                "    {} invalid Anthropic config: {}",
                "⚠".yellow(),
                err
            );
            return results;
        }
    };

    let pb_sem = if verb.is_quiet() {
        None
    } else {
        let pb = ProgressBar::new(doc_files.len() as u64);
        pb.set_style(
            ProgressStyle::with_template("  {bar:40.green/dim} {pos}/{len} docs ({eta} remaining)")
                .unwrap()
                .progress_chars("██░"),
        );
        Some(pb)
    };

    let mut handles = Vec::new();
    for doc_path in doc_files {
        let anthropic_cli = llm::LlmCliConfig {
            provider: "anthropic".to_string(),
            command: "anthropic".to_string(),
        };
        if let Some(cached) = llm::load_current_entry(doc_path, root, &cache_dir, &anthropic_cli)
            .or_else(|| graphify_cache::load_cached_from(doc_path, root, &legacy_cache_dir))
        {
            let _ = llm::save_legacy_entry(doc_path, root, &cache_dir, "anthropic", &cached);
            results.push(cached);
            if let Some(ref pb) = pb_sem {
                pb.inc(1);
            }
            continue;
        }

        let content = match std::fs::read_to_string(doc_path) {
            Ok(c) => c,
            Err(_) => {
                if let Some(ref pb) = pb_sem {
                    pb.inc(1);
                }
                continue;
            }
        };
        let doc_p = doc_path.clone();
        let root_p = root.to_path_buf();
        let file_type = semantic_file_type(doc_path).to_string();
        let config = config.clone();
        let sem_clone = sem.clone();
        let handle = tokio::spawn(async move {
            let _permit = sem_clone
                .acquire()
                .await
                .map_err(|e| anyhow::anyhow!("semaphore closed: {e}"))?;
            graphify_extract::semantic::extract_semantic(&doc_p, &content, &file_type, &config)
                .await
                .map(|result| (doc_p, root_p, result))
        });
        handles.push(handle);
    }

    for handle in handles {
        match handle.await {
            Ok(Ok((doc_p, root_p, sem_result))) => {
                verbose_print!(
                    verb,
                    "    {} → {} nodes, {} edges",
                    doc_p.file_name().unwrap_or_default().to_string_lossy(),
                    sem_result.nodes.len(),
                    sem_result.edges.len()
                );
                let _ =
                    llm::save_legacy_entry(&doc_p, &root_p, &cache_dir, "anthropic", &sem_result);
                let _ =
                    graphify_cache::save_cached_to(&doc_p, &sem_result, &root_p, &legacy_cache_dir);
                results.push(sem_result);
            }
            Ok(Err(e)) => {
                verbose_print!(verb, "    {} semantic extraction: {}", "⚠".yellow(), e);
            }
            Err(e) => {
                verbose_print!(verb, "    {} task join error: {}", "⚠".yellow(), e);
            }
        }
        if let Some(ref pb) = pb_sem {
            pb.inc(1);
        }
    }
    if let Some(pb) = pb_sem {
        pb.finish_and_clear();
    }

    results
}

struct ClusterResult {
    communities: HashMap<usize, Vec<String>>,
    cohesion: HashMap<usize, f64>,
    community_labels: HashMap<usize, String>,
}

fn step_cluster(
    graph: &mut graphify_core::graph::KnowledgeGraph,
    verb: Verbosity,
) -> ClusterResult {
    info_print!(verb, "  {} communities...", "Detecting".cyan());
    let communities = graphify_cluster::cluster(graph);
    let cohesion = graphify_cluster::score_all(graph, &communities);

    for (&cid, members) in &communities {
        for nid in members {
            if let Some(node) = graph.get_node_mut(nid) {
                node.community = Some(cid);
            }
        }
    }

    let community_labels: HashMap<usize, String> = {
        let mut used_labels: std::collections::HashSet<String> = std::collections::HashSet::new();
        communities
            .iter()
            .map(|(cid, nodes)| {
                let generic = ["lib", "super::*", "main", "mod", "tests"];
                let best = nodes
                    .iter()
                    .filter_map(|id| graph.get_node(id))
                    .filter(|n| {
                        !generic.contains(&n.label.as_str())
                            && !n.label.starts_with("std::")
                            && !n.label.starts_with("serde::")
                            && !n.label.contains("::")
                            && graphify_core::quality::is_summary_candidate(n)
                    })
                    .max_by_key(|n| match n.node_type {
                        graphify_core::model::NodeType::Function => 3,
                        graphify_core::model::NodeType::Class
                        | graphify_core::model::NodeType::Struct => 3,
                        graphify_core::model::NodeType::Module => 1,
                        graphify_core::model::NodeType::File => 0,
                        _ => 2,
                    })
                    .map(|n| n.label.clone())
                    .unwrap_or_else(|| {
                        nodes
                            .first()
                            .and_then(|id| graph.get_node(id))
                            .map_or_else(|| format!("Community {cid}"), |n| n.label.clone())
                    });
                let label = if used_labels.contains(&best) {
                    format!("{best} ({cid})")
                } else {
                    used_labels.insert(best.clone());
                    best
                };
                (*cid, label)
            })
            .collect()
    };

    info_print!(
        verb,
        "  {} communities detected",
        communities.len().to_string().bold()
    );

    ClusterResult {
        communities,
        cohesion,
        community_labels,
    }
}

#[allow(clippy::too_many_arguments)]
fn step_export(
    graph: &graphify_core::graph::KnowledgeGraph,
    communities: &HashMap<usize, Vec<String>>,
    cohesion: &HashMap<usize, f64>,
    community_labels: &HashMap<usize, String>,
    god_list: &[graphify_core::model::GodNode],
    surprise_list: &[graphify_core::model::Surprise],
    questions: &[HashMap<String, String>],
    detection: &graphify_detect::DetectResult,
    output_dir: &Path,
    root: &str,
    max_viz_nodes: Option<usize>,
    should_export: impl Fn(&str) -> bool,
    verb: Verbosity,
) -> Result<()> {
    std::fs::create_dir_all(output_dir)?;

    if should_export("json") {
        let json_path = graphify_export::export_json(graph, output_dir)?;
        info_print!(verb, "  Wrote {}", json_path.display().to_string().dimmed());
    }

    if should_export("html") {
        let html_path = graphify_export::export_html(
            graph,
            communities,
            community_labels,
            output_dir,
            max_viz_nodes,
        )?;
        info_print!(verb, "  Wrote {}", html_path.display().to_string().dimmed());

        let split_path =
            graphify_export::export_html_split(graph, communities, community_labels, output_dir)?;
        info_print!(
            verb,
            "  Wrote {}/",
            split_path.display().to_string().dimmed()
        );
    }

    let detection_json = serde_json::json!({
        "total_files": detection.total_files,
        "total_words": detection.total_words,
        "warning": detection.warning,
    });
    let token_cost: HashMap<String, usize> =
        HashMap::from([("input".to_string(), 0), ("output".to_string(), 0)]);
    let question_json: Vec<serde_json::Value> = questions
        .iter()
        .map(|q| serde_json::to_value(q).unwrap_or_default())
        .collect();

    if should_export("report") {
        let report = graphify_export::generate_report(&graphify_export::ReportInput {
            graph,
            communities,
            cohesion_scores: cohesion,
            community_labels,
            god_nodes: god_list,
            surprises: surprise_list,
            detection_result: &detection_json,
            token_cost: &token_cost,
            root,
            suggested_questions: Some(&question_json),
        })?;
        let report_path = output_dir.join("GRAPH_REPORT.md");
        std::fs::write(&report_path, &report)?;
        info_print!(
            verb,
            "  Wrote {}",
            report_path.display().to_string().dimmed()
        );
    }

    if should_export("context") {
        let context =
            graphify_export::generate_llm_context(graph, communities, community_labels, root);
        let context_path = graphify_export::export_llm_context(&context, output_dir)?;
        info_print!(
            verb,
            "  Wrote {}",
            context_path.display().to_string().dimmed()
        );
    }

    if should_export("graphml") {
        let graphml_path = graphify_export::export_graphml(graph, output_dir)?;
        info_print!(
            verb,
            "  Wrote {}",
            graphml_path.display().to_string().dimmed()
        );
    }

    if should_export("cypher") {
        let cypher_path = graphify_export::export_cypher(graph, output_dir)?;
        info_print!(
            verb,
            "  Wrote {}",
            cypher_path.display().to_string().dimmed()
        );
    }

    if should_export("svg") {
        let svg_path = graphify_export::export_svg(graph, communities, output_dir)?;
        info_print!(verb, "  Wrote {}", svg_path.display().to_string().dimmed());
    }

    if should_export("wiki") {
        let wiki_path =
            graphify_export::export_wiki(graph, communities, community_labels, output_dir)?;
        info_print!(verb, "  Wrote {}", wiki_path.display().to_string().dimmed());
    }

    if should_export("obsidian") {
        let obsidian_path =
            graphify_export::export_obsidian(graph, communities, community_labels, output_dir)?;
        info_print!(
            verb,
            "  Wrote {}",
            obsidian_path.display().to_string().dimmed()
        );
    }

    let manifest_path = output_dir.join(".graphify_manifest.json");
    let manifest = graphify_detect::manifest_from_detection(Path::new(root), detection);
    graphify_detect::save_manifest(&manifest_path, &manifest)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use graphify_core::model::{ExtractionResult, GraphNode, NodeType};

    #[tokio::test]
    async fn cli_semantic_uses_cached_fallback_when_fresh_extraction_fails() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        let output = root.join(".graphify");
        let doc = root.join("doc.md");
        std::fs::write(&doc, "v1").expect("write v1");

        let cli = llm::LlmCliConfig {
            provider: "codex".into(),
            command: "definitely-missing-graphify-test-command".into(),
        };
        let cache = llm::provider_cache_dir(&output, &cli.provider);
        let cached = ExtractionResult {
            nodes: vec![GraphNode {
                id: "cached".into(),
                label: "Cached".into(),
                source_file: "doc.md".into(),
                source_location: None,
                node_type: NodeType::Concept,
                community: None,
                extra: HashMap::new(),
            }],
            edges: Vec::new(),
            hyperedges: Vec::new(),
        };
        assert!(llm::save_entry(&doc, root, &cache, &cli, &cached));

        std::fs::write(&doc, "v2").expect("write v2");
        let results =
            run_cli_semantic_extraction(&[doc], root, &output, &cli, Verbosity::Quiet, Some(1))
                .await;

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].nodes[0].label, "Cached");
        assert_eq!(
            results[0].nodes[0].extra["llm_stale_preserved"],
            serde_json::json!(true)
        );
    }

    #[test]
    fn manifest_from_detection_records_current_hashes() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        let file = root.join("src.rs");
        std::fs::write(&file, "fn main() {}").expect("write source");

        let detection = graphify_detect::DetectResult {
            files: HashMap::from([(graphify_detect::FileType::Code, vec!["src.rs".into()])]),
            total_files: 1,
            total_words: 3,
            needs_graph: false,
            warning: None,
            skipped_sensitive: Vec::new(),
            graphifyignore_patterns: 0,
        };

        let manifest = graphify_detect::manifest_from_detection(root, &detection);

        assert_eq!(
            manifest.files.get("src.rs"),
            Some(&graphify_detect::FileType::Code)
        );
        assert_eq!(
            manifest.hashes.get("src.rs"),
            graphify_cache::file_hash(&file).as_ref()
        );
    }
}
