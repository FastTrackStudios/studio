# Splitting session-daw

Measured 2026-10-13 over `apps/session/daw/src` (156 modules, ~139k lines).
115 modules are one strongly connected component and every agreed feature
group sits in one cycle, so the split needs symbols moved down and a few
references inverted before crates can be cut. `proposal.py` checks a cut
(118 symbol moves, 40 inverted references) that leaves the crate graph
acyclic.

- `proposal.txt` — the proposal: crate order, each crate's modules, the
  symbols each receives, the inversions (hooks / traits).
- `group_cycles.txt`, `feature_edges.txt` — the group-level cycles and the
  references behind every edge.
- `grab_bags.txt` — actions.rs, engine.rs, widget.rs and friends: how they
  split (actions.rs holds ~2.6k lines of the drum quantize engine).
- `extraction_risks_and_tests.txt` — moves to check by hand; where the
  binaries and integration tests land.
- `deps.py` builds `module_deps*.json`; `groups.py` the agreed groups;
  `proposal.py` / `proposal_alt.py` the two acyclic cuts (alt puts session
  lower). Rerun after the tree changes — line numbers age.

Pairs with `../timeline.md`, the shared editing engine the arrangement and
the editor converge on (the timeline-candidate modules here).
