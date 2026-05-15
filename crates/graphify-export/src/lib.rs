//! Multi-format export for graphify knowledge graphs.
//!
//! Supports JSON, HTML (interactive visualization), SVG, GraphML, Cypher
//! (Neo4j), wiki-style markdown, and analysis reports.

pub mod architecture;
pub mod context;
pub mod cypher;
pub mod graphml;
pub mod html;
pub mod json;
pub mod likec4;
pub mod obsidian;
pub mod report;
pub mod svg;
pub mod wiki;

pub use cypher::export_cypher;
pub use graphml::export_graphml;
pub use html::export_html;
pub use html::export_html_split;
pub use json::export_json;
pub use likec4::{
    DEFAULT_LIKEC4_MAX_NODES, DEFAULT_LIKEC4_MAX_RELATIONS, LikeC4Detail, LikeC4Options,
    export_likec4, export_likec4_with_options,
};
pub use obsidian::export_obsidian;
pub use report::generate_report;
pub use svg::export_svg;
pub use wiki::export_wiki;

pub use architecture::{
    ArchitectureCapability, ArchitectureCapabilityFlow, ArchitectureContract,
    ArchitectureContracts, ArchitectureDependency, ArchitectureExternalDependency,
    ArchitectureInterfaces, ArchitectureLandscape, ArchitectureLandscapeContract,
    ArchitectureManifest, ArchitectureOptions, ArchitecturePackage, ArchitectureService,
    ArchitectureServiceDependency, build_architecture_manifest, compose_architecture_manifests,
    export_architecture_manifest, export_architecture_workspace, export_landscape_workspace,
};
pub use context::{export_llm_context, generate_llm_context};
