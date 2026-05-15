---
name: graphify
description: Build, update, and query graphify-rs knowledge graphs from Codex; prefer graphifyq for short-lived HTTP MCP graph access.
---

# graphify for Codex

Use this skill when the user asks to build or query a project knowledge graph,
understand repository architecture through graphify-rs, or use `graphifyq`.

## Install / availability check

```bash
command -v graphify-rs >/dev/null || echo "Install graphify-rs first"
command -v graphifyq >/dev/null || echo "Install graphifyq first"
command -v graphify-llm-codex >/dev/null || echo "Install graphify-llm-codex first if you need Codex-backed LLM extraction"
```

## Build or update a graph

From the repository root:

```bash
graphify-rs build --path . --output .graphify --no-llm --format json,report
```

This default is intentionally AST/local-document only for new calls. It does
not delete existing cached LLM extraction data in `.graphify/llm-cache.json`.

After code changes, prefer the short-lived helper. It auto-refreshes a stale
per-repo graph every 300s with a safe incremental build and restarts the local
HTTP sidecar if the graph changed:

```bash
graphifyq ensure
```

To force the update immediately:

```bash
graphify-rs build --path . --output .graphify --no-llm --update --format json,report
```

## Optional LikeC4 architecture workspace

Use this when the user wants a native architecture diagram or wants to inspect
the graph through LikeC4:

```bash
graphify-rs build --path . --output .graphify --no-llm \
  --format architecture,likec4 \
  --likec4-detail architecture \
  --service-name my-service
cd .graphify/likec4
npx likec4 validate
npx likec4 start
```

Notes:

- `--format architecture` writes `.graphify/architecture.json`, a compact
  service manifest with module prefixes, capabilities/contracts, inferred capability flows, package evidence, folded
  package dependencies, external imports, API/event/proto hints, and `contracts.provided/consumed/unresolved`.
- `--likec4-detail architecture` uses that manifest for capability/contract-level
  service C4. Prefer it for service/repo diagrams.
- `balanced` keeps type-level components too; use only when the user wants that
  detail.
- Compose service landscapes with exact contract aliases first, then module-prefix/interface/package evidence. Use `graphify-rs compose --input svcA/.graphify/architecture.json --input svcB/.graphify/architecture.json --output .graphify/landscape` or `graphify-rs compose --discover <workspace> --output .graphify/landscape`.
- Landscape LikeC4 renders concrete API/module-prefix dependencies, only dependency-referenced APIs, and separate operation-labelled API edges for multi-contract dependencies; package-name-only guesses stay in JSON evidence, not the visual C4.
- Service LikeC4 workspaces emit capability-owned `api` contracts, nested `operation` elements for RPC/methods, `component` implementation modules with folded module dependencies, and `externalApi` for unresolved consumed contracts; do not convert unresolved contracts into guessed service calls.
- Generated `.c4` files can be refreshed. Manual LikeC4 layout snapshots are
  persisted under `.graphify/likec4/.likec4/` and must not be deleted by agents.
- No GitHub links are emitted. Use `source_file` / `source_location` metadata
  for private GitLab/source-host link resolution.

## Optional LLM enrichment via installed Codex CLI

Do not ask the user for Anthropic/OpenAI API keys just to enrich graphify.
graphify-rs can call any installed local CLI that reads a prompt from stdin and
prints compact JSON with `entities` and `relationships`. The bundled adapter is
for Codex CLI:

```bash
graphify-llm-codex --model gpt-5.4-mini --reasoning-effort low < prompt.txt
```

Use LLM enrichment only when the user explicitly asks for it, or when indexing
large docs/prose where AST extraction is insufficient:

```bash
graphify-rs build --path . --output .graphify --update --embed \
  --llm-command "graphify-llm-codex --model gpt-5.4-mini --reasoning-effort low" \
  --llm-provider codex-cli

graphifyq ensure --with-llm \
  --llm-command "graphify-llm-codex --model gpt-5.4-mini --reasoning-effort low" \
  --llm-provider codex-cli
```

LLM cache rules:

- `--no-llm` means "make no new LLM calls"; it preserves existing LLM output.
- `--llm-command` reuses previous extraction as context when a file changed, so
  rebuilds are incremental/reiterative instead of starting from scratch.
- If the command/provider/prompt contract changes, graphify marks old LLM output
  as stale-preserved instead of silently treating it as fresh.
- Pass the repository root to `--path`; use `.graphifyignore` include/exclude
  rules when you need to narrow to a subdirectory such as `src` or `bin`.

## Query from Codex

Prefer `graphifyq`; it behaves like `fffq`: it starts or reuses a per-project
local HTTP MCP sidecar, auto-refreshes stale graphs, writes its registry to
`.graphify/.graphifyq-server.json`, prints the answer, and exits.
Its sidecars exit after 900 idle seconds. Use `graphifyq gc --dry-run` /
`graphifyq gc` to inspect or stop stale/orphan sidecars.

```bash
graphifyq ensure
graphifyq query "where is authentication wired?" --format toon
graphifyq summary architecture --budget 3000 --format toon
graphifyq stats --format toon
graphifyq tool graph_stats '{}' --format toon
```

Use graphify for architecture/codebase questions after FFF/grepai source lookup,
not as a replacement for exact file search. Prefer architecture summaries for
broad orientation and focused identifier queries for lookup; do not paste whole
user prompts into `graphifyq query`. Good graphify questions:

- "what are the main communities in this repo?"
- "which modules bridge the data ingestion and API layers?"
- "what depends on this table/service/function?"
- "where are cycles or surprising cross-community edges?"

## Project-level Codex setup

```bash
graphify-rs codex install
```

This adds graphify guidance to `AGENTS.md` and writes `.codex/hooks.json` with a
silent `graphify-rs hook-check` entry. It intentionally does not inject unsupported
Codex hook output; use `graphifyq` explicitly for graph context.

## Rules for agents

- If `.graphify/GRAPH_REPORT.md` exists, consult it before broad architecture answers.
- Prefer `graphifyq summary architecture --budget 2000 --format toon` for broad context.
- Use `graphifyq query "<focused identifiers or subsystem>" --format toon` for focused lookup.
- Default `graphifyq query`, `summary`, `stats`, and `tool` to `--format toon` for agent context; omit it only when the user asks for prose/human-readable text.
- Keep `.graphify/` current after meaningful code edits with `graphifyq ensure`; use `--no-llm --update` only to force an immediate rebuild without new LLM calls.
- Use `graphifyq ensure --with-llm --llm-command "graphify-llm-codex ..."` only for explicit LLM refresh/enrichment.
- Do not paste entire reports; summarize god nodes, communities, cycles, and next questions.
