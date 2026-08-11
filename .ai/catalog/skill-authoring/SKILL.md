---
name: skill-authoring
description: Create or improve reusable agent skills, including trigger design, progressive disclosure, deterministic validation, and reproducible packaging.
---

# Skill Authoring

Use a skill for reusable judgment, a script or hook for deterministic enforcement, and repository guidance for facts that apply to most tasks. Do not create a skill when a short instruction or existing command is enough.

## Author

1. Define the outcome, inputs, constraints, failure behavior, and proof.
2. Write at least five realistic positive triggers and five near-misses before tuning the description.
3. Put only the starting procedure in `SKILL.md`; route optional detail to references and repeated deterministic work to scripts.
4. Keep provider mechanics in separate adapters. Shared instructions must describe capabilities and outcomes, not product syntax.
5. Declare an explicit package allowlist and context budgets in `manifest.yaml`.

## Validate

From this skill directory:

```bash
python -m scripts.quick_validate <skill-directory>
python -m scripts.package_skill <skill-directory> <output-directory>
```

The validator checks frontmatter, exact manifest shape, trigger cases, references, package contents, provider-neutral text, and static budgets. Packaging includes only declared files and uses deterministic archive metadata.

Structural validation does not prove usefulness. Before making a skill default, compare representative tasks with and without it across supported runtimes. Record correctness, regressions, tool calls, tokens, latency, and trigger false positives. Keep live-model harnesses outside the neutral skill unless each runtime has an explicit adapter.
