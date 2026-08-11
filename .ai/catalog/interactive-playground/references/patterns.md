# Playground Patterns

- **Design explorer:** controls beside a live component or layout preview.
- **Data/query builder:** structured controls, generated query preview, and sample results.
- **Concept or code map:** keyboard-selectable nodes and relationships with a textual alternative.
- **Document or diff review:** source panel, filterable comments, approve/reject state, and line-addressed prompt output.

Use CSS layout for ordinary panels, SVG for accessible diagrams, and canvas only when interaction volume makes DOM or SVG impractical. When rendering source text, create text nodes; syntax color can be applied to parsed tokens without accepting HTML. Keep user comments in data attributes or state only after safe encoding.
