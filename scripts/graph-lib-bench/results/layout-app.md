## target/release/f_layout app
date: 2026-10-23 22:41 IST
fixture `app`: 3000 nodes, 346 edges; petgraph UnGraph build 0.02 ms, connected_components = 2681 in 0.022 ms

| engine | ticks | ms/tick p50 | p95 | settle total ms | deterministic | quality |
|---|---:|---:|---:|---:|---|---|
| impro-graph Layout | 241 | 1.49 | 1.65 | 362 | yes | min gap 44.9, overlapping pairs 0, edge len mean 221 sd 80 (cv 0.36) min 119 max 634, 98 % of nodes within r=4190 |
| fjadra (link+charge+center+collide) | 1 | 2603.19 | 2603.19 | 2603 | yes | min gap -0.1, overlapping pairs 90, edge len mean 38 sd 8 (cv 0.22) min 29 max 75, 98 % of nodes within r=4538 |
| force_graph (O(n²) repulsion) | 30 | 32.85 | 33.31 | 985 | yes | min gap -15.6, overlapping pairs 2, edge len mean 15792 sd 4290 (cv 0.27) min 2715 max 22212, 98 % of nodes within r=12730 |
