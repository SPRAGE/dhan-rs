# Codex Project Config

This directory contains Codex-facing runtime assets compiled from `.ai/`.

- `.codex/config.toml` defines project-scoped effort, search, and delegation defaults; it does not pin the main model.
- `.codex/agents/` contains generated `scout`, `researcher`, `worker`, and `reviewer` roles. Role model bindings come from `.ai/capabilities/runtimes/codex.yaml`.
- `.codex/skills/` mirrors `.ai/skills/` for compatibility with agents and tools that look under `.codex/`.
- `.ai/catalog/` stays outside discovery; `.ai/tools/skillctl.py` activates only recurring specialist procedures.
- `.agents/skills/` is the official Codex repo-scoped skill link.
- `AGENTS.md` is the Codex-compatible auto-load adapter.
- `CODEX.md` is the named Codex adapter for humans and tools.
- `AI.md` and `.ai/` remain the shared source of truth.
- Edit neutral agent contracts under `.ai/agents/` and regenerate; do not edit generated TOML directly.

Local Codex runtime state should live under `.codex/local/`, `.codex/tmp/`, `.codex/sessions/`, or `.codex/logs/`, which are gitignored.
