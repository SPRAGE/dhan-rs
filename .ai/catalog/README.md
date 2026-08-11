# Conditional Skill Catalog

These provider-neutral skills ship with generated projects but are outside `.ai/skills/`, so they add no default discovery context.

For Planned or Hard work, consult `index.yaml` only when specialist procedure would materially improve the result. Load at most two relevant skill bodies for a task. Activate a skill only when the domain recurs in the project:

```bash
python .ai/tools/skillctl.py list
python .ai/tools/skillctl.py activate performance-engineering
python .ai/tools/skillctl.py deactivate performance-engineering
```

Activation creates a relative link under `.ai/skills/`; provider discovery paths already point there. The tool preserves custom collisions and removes only links it created to this catalog.

Default project skills remain under `template/.ai/skills/`. Deterministic archives for default and catalog skills are published under `skills/`.
