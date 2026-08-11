---
name: data-visualization
description: Design accurate, accessible, and performant analytical visualizations from the user's question, data semantics, uncertainty, and interaction needs.
---

# Data Visualization

Make the data's meaning easier to see without distorting it.

## Frame

Identify the user, decision, comparison, time horizon, granularity, update frequency, and consequence of misreading the result. Inspect units, distributions, missingness, uncertainty, sampling, aggregation, and category cardinality before choosing a chart.

## Encode

Choose the simplest accurate encoding: position and length for precise comparison, lines for ordered change, bars for discrete comparison, distributions for spread, and tables when exact lookup dominates. Use consistent units and scales. Show denominators, aggregation windows, time zones, uncertainty, missing values, and partial data explicitly.

Avoid truncated or dual axes that manufacture a story, area/volume encodings for precise values, decorative dimensions, and color as the only carrier of meaning. Preserve a stable visual hierarchy and the project's design system.

## Interact And Prove

Make filters, selections, drill-down, reset, loading, empty, error, and stale states clear. Support keyboard use, semantic labels, contrast, reduced motion, and a non-visual data alternative. For large or streaming datasets, aggregate deliberately, virtualize or downsample without hiding extremes, and bound update frequency.

Validate representative values against source calculations. Test narrow and wide layouts, long labels, missing data, outliers, permissions, and live updates. State what the visualization cannot establish.
