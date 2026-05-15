---
name: graphify
description: Build, update, and query graphify-rs knowledge graphs from Claude Code; supports graphifyq and MCP graph tools.
trigger: /graphify
---

# graphify for Claude Code

Use this skill when the user invokes `/graphify`, asks to build/query a graph,
or asks architecture questions that benefit from graphify-rs.

## Build or update a graph

Default full build from the current repository:

```bash
graphify-rs build --path . --output .graphify --no-llm --embed
```

This default makes no new LLM calls and preserves existing cached LLM output in
`.graphify/llm-cache.json`.

After code edits, prefer graphifyq's per-repo auto-refresh path:

```bash
graphifyq ensure
```

It refreshes stale graphs every 300s using the safe incremental build path and
restarts the local HTTP sidecar if the graph changed. To force the update
immediately:

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

## Optional LLM enrichment via local CLI

graphify-rs no longer requires an API key for LLM enrichment. It can call any
installed CLI that reads the graphify prompt from stdin and prints compact JSON
with `entities` and `relationships`. For Codex CLI, use the bundled adapter:

```bash
graphify-rs build --path . --output .graphify --update --embed \
  --llm-command "graphify-llm-codex --model gpt-5.4-mini --reasoning-effort low" \
  --llm-provider codex-cli

graphifyq ensure --with-llm \
  --llm-command "graphify-llm-codex --model gpt-5.4-mini --reasoning-effort low" \
  --llm-provider codex-cli
```

Rules:

- `--no-llm` preserves existing LLM annotations; it does not erase them.
- LLM rebuilds are incremental: changed files receive prior extraction context.
- Provider/command/prompt-contract changes mark old output stale-preserved rather
  than overwriting it silently.

## Query options

Direct CLI query:

```bash
graphify-rs query "QUESTION" --graph .graphify/graph.json
```

Short-lived HTTP MCP helper, useful in Claude Code and other terminal agents:

```bash
graphifyq ensure
graphifyq ensure --no-auto-refresh
graphifyq query "focused identifiers or subsystem" --format toon
graphifyq summary architecture --budget 3000 --format toon
graphifyq stats --format toon
graphifyq gc --dry-run
```

Long-lived stdio MCP server:

```bash
graphify-rs serve --graph .graphify/graph.json
```

HTTP MCP sidecar:

```bash
graphify-rs serve --transport http --http-bind 127.0.0.1:0 \
  --registry-path .graphify/.graphifyq-server.json \
  --graph .graphify/graph.json
```

## Project-level Claude setup

```bash
graphify-rs claude install
```

This updates `CLAUDE.md` and `.claude/settings.json` so Claude Code is reminded
that a graph exists before broad file search.

## Response workflow

When invoked for a build:

1. Use `.` if no path was provided.
2. Run `graphify-rs build` with user-provided flags.
3. Read `.graphify/GRAPH_REPORT.md`.
4. Summarize only the useful parts: god nodes, communities, surprising edges,
   cycles, and suggested questions.
5. Offer one concrete follow-up query.

When answering architecture questions:

- Prefer existing `.graphify/GRAPH_REPORT.md` and `graphifyq summary architecture --budget 2000 --format toon` first.
- Use `graphifyq query "<focused identifiers or subsystem>" --format toon` for focused questions; do not paste whole user prompts into `query`.
- Default `graphifyq query`, `summary`, `stats`, and `tool` to `--format toon` for agent context; omit it only when the user asks for prose/human-readable text.
- Rebuild with `graphifyq ensure` or `--no-llm --update` after meaningful code changes; use `--with-llm` only for explicit LLM refresh/enrichment.
- Do not paste full graph JSON or full reports unless explicitly requested.
