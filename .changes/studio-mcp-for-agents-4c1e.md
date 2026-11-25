---
"studio-mcp" = "minor"
---

Studio's MCP for an agent: `studio-mcp`, a stdio MCP server on the app's control socket (no port, no token, survives Studio's restarts), with curated typed mixing tools (`session_tree`, `mix_state`, `signal_block`, `listen`, `block_measure`, level-matched `listen_ab`, `mix_measure`, `balance`, `signal_block_set`, …), compact answers that page instead of being cut, and long calls as jobs (`job`, `jobs`, `job_cancel`). `.mcp.json` names it as `studio` (session)
