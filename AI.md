# PROJECTNAME

TODO replace this line with a one-sentence description of the Rust application or library and its users.

## Start

1. Run `cargo init` and replace `PROJECTNAME` in `AI.md`, `.ai/project.yaml`, and `flake.nix`.
2. Run `direnv allow` or `nix develop`.
3. Ask for the first outcome in plain language; the primary agent plans and verifies, delegating only when isolation or parallelism helps.
4. Invoke `agent-context` after code exists to capture the real architecture and commands.

## Stack

- Stable Rust toolchain.
- Cargo for build, dependency, test, lint, and format workflows.
- Nix development shell.

## Commands

- `nix develop` - enter the development environment.
- `cargo check` - validate code after `Cargo.toml` exists.
- `cargo test` - run tests after the crate is initialized.
- `cargo clippy -- -D warnings` - lint after the crate is initialized.
- `cargo fmt --check` - check formatting after the crate is initialized.
- `nix run github:SPRAGE/dev-template#ai-doctor` - validate agent assets.

## Layout

- `.ai/` - neutral agent specification, project context, and the default shared skill.
- `.agents/skills/` - Codex skill discovery link.
- `.claude/` and `.codex/` - generated provider agents plus native settings.

## Project Facts

- Add crate entry points, workspace boundaries, error strategy, and non-obvious constraints here.
