---
"xtask-workflows" = "minor"
---

The release attaches Studio Architect's node and web bundle for x86_64 Linux (`studio-architect-node-<tag>-x86_64-linux.tar.gz`, `studio-architect-web-<tag>.tar.gz`, each with a .sha256), and SHA256SUMS now covers every asset of the Release and is signed with the org minisign key (SHA256SUMS.minisig); PRs that reach the node get a `cargo check` of it for Linux (session)
