## target/release/f_layout dense
date: 2026-10-23 23:06 IST
fixture `dense`: 3000 nodes, 8653 edges; petgraph UnGraph build 0.14 ms, connected_components = 1 in 0.039 ms

| engine | ticks | ms/tick p50 | p95 | settle total ms | deterministic | quality |
|---|---:|---:|---:|---:|---|---|
| impro-graph Layout | 241 | 1.68 | 1.81 | 408 | yes | min gap 35.6, overlapping pairs 0, edge len mean 1260 sd 719 (cv 0.57) min 64 max 6022, 98 % of nodes within r=4000 |
| fjadra (link+charge+center+collide) | 300 | 10.36 | 11.46 | 3142 | yes | min gap -0.1, overlapping pairs 439, edge len mean 325 sd 254 (cv 0.78) min 26 max 2403, 98 % of nodes within r=2095 |
| force_graph (O(n²) repulsion) | 30 | 32.65 | 33.92 | 986 | yes | min gap 7.5, overlapping pairs 0, edge len mean 1983 sd 2944 (cv 1.48) min 54 max 28319, 98 % of nodes within r=15269 |
