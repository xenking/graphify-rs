## Status

Implemented in this branch:

- `graphify-rs build --format likec4`
- `graphify-rs build --format architecture`
- `.graphify/architecture.json` compact service manifest:
  - service name
  - module/import prefixes
  - capability/contract nodes, inferred capability flows, plus package/directory evidence
  - folded package dependencies
  - external imports
  - provided/consumed API/event/proto hints
  - exact contract registry: `contracts.provided`, `contracts.consumed`, `contracts.unresolved`
- `graphify-rs compose --input ...` for top-level service landscapes
- `graphify-rs compose --discover <workspace>` for existing nested `.graphify/architecture.json` manifests
- native LikeC4 workspace under `.graphify/likec4/`
- persistent manual layouts via `manualLayouts.outDir = ".likec4"`
- service LikeC4 uses `api` elements for provided contracts and `externalApi` elements for unresolved consumed contracts
- native `metadata` blocks on elements/relationships:
  - `graphify_id`
  - `graphify_type`
  - `source_file`
  - `source_location`
  - `community`
  - `source_kind`
  - `source_flags`
  - relation/confidence fields for edges
- no GitHub source links; metadata is private-GitLab friendly and repo-relative
- default readable caps: `250` graph nodes / `150` relationships for balanced/full, package caps for architecture
- `--likec4-detail architecture` is capability/contract-level service C4; package deps stay in manifest evidence/drilldown, not default noise
- code-level edges are folded into package dependencies and then into capability-level `calls`/`depends`/`reads`/`writes` flows with sampled provenance; visible C4 edges avoid generic `uses` spam
- compose links exact contract aliases first, then module-prefix imports, consumed/provided interfaces, target contracts named for the source service, and service-named package references; unresolved external APIs stay visible instead of guessed
- CLI build pipeline re-runs cross-file import resolution after cache-backed per-file extraction is merged, so LikeC4 gets real inter-file dependencies
- knobs:
  - `--likec4-max-nodes`
  - `--likec4-max-relations`
  - `--likec4-detail architecture|balanced|full`
  - `--likec4-include-path`
  - `--likec4-exclude-path`
- filters for generated/test/dependency/import/function noise, transient `.graphify`/LikeC4 outputs, docs/scratch folders, `*.user.js`, SQL migration/query false positives

Validated with `npx likec4@1.56.0 validate` and `npx likec4@1.56.0 format --check` on:

- `graphify-rs` self-check
- one schema repository fixture
- four private service repositories used only as validation inputs
- one composed three-service landscape

---

Original implementation plan below.

## Текущий план: MVP LikeC4 exporter

**Goal:** `graphify-rs build --format likec4` генерит LikeC4 workspace из `KnowledgeGraph`.

**Scope fixed:**
- Rust-only generation.
- No TS port.
- No Node runtime by default.
- Output: `.graphify/likec4/*.c4`.
- User потом сам: `npx likec4 start/build/validate`.

### Files

Create:
- `crates/graphify-export/src/likec4.rs`

Modify:
- `crates/graphify-export/src/lib.rs`
- `src/main.rs`
- `docs/CLI.md`
- maybe `README.md`

### Output shape

```text
.graphify/likec4/
  likec4.config.json
  specification.c4
  model.c4
  views.c4
  README.md
```

### Phase 1 — exporter core

1. Add failing tests in `likec4.rs`:
   - sanitizes arbitrary graph IDs into valid LikeC4 IDs.
   - stable IDs: same input -> same output.
   - collision handling: `foo-bar`, `foo_bar` не ломают model.
   - escapes strings: quotes/newlines/backticks safe.

2. Implement:
   - `export_likec4(graph, communities, community_labels, output_dir) -> Result<PathBuf>`
   - helpers:
     - `sanitize_identifier`
     - `escape_likec4_string`
     - `element_kind_for_node`
     - `relationship_kind_for_edge`
     - `write_specification`
     - `write_model`
     - `write_views`

3. Verify:
   ```bash
   cargo test -p graphify-export likec4
   ```

### Phase 2 — model mapping

Node mapping:

```text
Package/Namespace -> system
Module/File       -> container
Class/Struct/Trait/Interface/Enum -> component
Function/Method   -> component, but maybe filtered later
Concept/Paper/Image -> component/artifact-ish custom element
```

Edge mapping:
```text
calls/imports/uses/depends_on/contains/mentions/implements/extends
```

Generated `specification.c4`:
```c4
specification {
  element system
  element container
  element component
  relationship calls
  relationship imports
  relationship calls
  relationship depends_on
}
```

Generated `model.c4`:
```c4
model {
  graphify = system "graphify-rs" {
    some_module = container "some_module"
    some_type = component "SomeType"
  }

  some_type -> other_type "calls"
}
```

Keep nesting shallow for MVP:
- root system = repo/project.
- communities become grouping via views first, not deep element hierarchy yet.
- avoid invalid parent inference hell.

### Phase 3 — views

Generate `views.c4`:

```c4
views {
  view index {
    include *
    autoLayout
  }

  view community_1 {
    include *
    autoLayout
  }
}
```

MVP view filter:
- `index`: include all capped nodes.
- per community: include nodes from that community + direct cross-community neighbors.
- if LikeC4 predicate syntax gets risky, fallback: generate separate model files? Avoid initially.

### Phase 4 — CLI integration

Modify `src/main.rs`:
- update format help:
  ```text
  json,html,architecture,likec4,graphml,cypher,svg,wiki,obsidian,report,context
  ```
- add `"likec4"` to `all_formats`.
- in export section:
  ```rust
  if should_export("likec4") {
      let likec4_path = graphify_export::export_likec4(...)?;
      info_print!(...)
  }
  ```

Modify `crates/graphify-export/src/lib.rs`:
```rust
pub mod likec4;
pub use likec4::export_likec4;
```

### Phase 5 — docs

Add:
```bash
graphify-rs build --no-llm --format likec4
cd .graphify/likec4
npx likec4 start
npx likec4 build -o ../likec4-dist
```

Mention Node requirement for LikeC4 CLI:
- npm `likec4@1.56.0` requires Node `>=22.22.0`.

### Phase 6 — verification

Run:
```bash
cargo fmt
cargo test -p graphify-export likec4
cargo test -p graphify-export
cargo test --workspace
graphifyq ensure
```

Optional if local Node ok:
```bash
cd .graphify/likec4
npx likec4 validate
```

## Acceptance criteria MVP

- `--format likec4` writes workspace.
- generated `.c4` parses visually enough for LikeC4 CLI.
- no Node dependency in graphify itself.
- weird graph IDs do not break DSL.
- strings safely escaped.
- tests cover exporter behavior.
- docs show flow.

---

## Next plan after MVP

### Next 1 — quality filters

Problem: raw graph too dense for C4.

Add options:
```bash
--likec4-max-nodes 250
--likec4-detail architecture|balanced|full
--likec4-min-confidence 0.5
--likec4-include-relation calls,imports,depends_on
```

Goal:
- readable diagrams by default.
- functions excluded unless requested.
- top nodes/communities preserved.

### Next 2 — richer views

Generate:
- `index`
- `communities`
- `community_<id>`
- `god_nodes`
- `cross_community`
- `files_to_types`
- maybe `runtime_dependencies` if graph has relations enough.

### Next 3 — optional LikeC4 CLI bridge

Add command, opt-in only:

```bash
graphify-rs likec4 validate
graphify-rs likec4 build --output .graphify/likec4-dist
graphify-rs likec4 export png
```

Behavior:
- checks `likec4`/`npx` availability.
- prints actionable Node error.
- no install/global mutation.

### Next 4 — crate extraction

If exporter useful, split:
```text
crates/graphify-likec4/
```

Public API:
```rust
LikeC4Options
LikeC4Workspace
export_likec4_workspace(...)
```

Then `graphify-export` depends on it.

### Next 5 — “likec4-rs” only if real demand

Port only generator-side first:
- Rust data model for LikeC4.
- Rust writer for DSL.
- no parser/LSP/layout.

Full TS port still bad ROI now.

## Execution path I recommend

1. Implement MVP exporter now.
2. Run Rust tests.
3. Generate sample `.graphify/likec4` from this repo.
4. If Node available, validate with `npx likec4 validate`.
5. Then decide options/views polish from real generated output.
