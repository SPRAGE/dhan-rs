---
name: frontend-design
description: Design or refine production frontend interfaces when visual hierarchy, interaction quality, accessibility, and implementation detail materially affect the outcome.
---

# Frontend Design

Build a coherent interface that serves its users and fits the product before pursuing novelty.

## Frame the direction

Inspect the existing design system, product context, audience, content density, platform, and technical constraints. Preserve established tokens and components unless the request explicitly calls for a new direction. State the intended visual character and the one or two choices that make it recognizable.

## Implement

- Establish hierarchy through typography, spacing, contrast, and composition.
- Use a small token set for color, type, spacing, radii, elevation, and motion.
- Match complexity to the concept: expressive work may need richer composition; restrained work needs precise rhythm and alignment.
- Make every control functional and every state intentional, including loading, empty, error, disabled, hover, focus, and narrow layouts.
- Prefer semantic markup and existing framework patterns over decorative reinvention.
- Add motion only when it communicates change or improves orientation; honor reduced-motion preferences.

Avoid defaulting to fashionable fonts, gradient-heavy palettes, card grids, or animation merely to appear designed. Distinctiveness should come from the domain and content, not a fixed style recipe.

## Verify

Check keyboard operation, visible focus, labels, contrast, zoom, responsive layout, reduced motion, overflow, and representative content extremes. Run the repository's frontend checks and inspect the rendered result with an available browser or screenshot workflow. Report any visual behavior that could not be verified.
