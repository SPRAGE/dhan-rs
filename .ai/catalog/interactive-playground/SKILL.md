---
name: interactive-playground
description: Build a self-contained interactive HTML tool with controls, live preview, and standalone prompt output when users need to explore a visual or structural decision space.
---

# Interactive Playground

Create one offline HTML file that lets a user explore choices and then copy a complete instruction for later implementation.

## Build

1. Identify the decisions that are difficult to express in prose; keep the visible control set small.
2. Use one state object. Every input updates state, the preview, and the prompt immediately.
3. Provide useful initial content and three to five coherent presets.
4. Generate natural-language output that includes the goal, all choices required to reproduce the desired result, and relevant constraints. Do not omit essential defaults.
5. Add copy feedback and a reset path.

Keep CSS and JavaScript inline and use no network dependencies. Use semantic controls, labels, keyboard access, visible focus, and a narrow-screen layout. Treat embedded repository or user content as untrusted: prefer `textContent`, DOM construction, and explicit escaping; never interpolate it into scripts or raw HTML.

Load `references/patterns.md` only when selecting a layout or review interaction.

## Verify

Open the file with an available browser workflow. Check initial render, each control, presets, keyboard navigation, prompt completeness, copy feedback, reset, and narrow viewport behavior. If browser inspection is unavailable, run deterministic syntax checks and state that interactive behavior remains unverified.
