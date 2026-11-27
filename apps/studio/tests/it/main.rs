//! Studio's integration tests, one binary (the `rust-testing` skill):
//!
//! - `web_boot`: Seam 3 for the web, the web build opening and playing a
//!   fixture session in a browser (`just test-web-boot`);
//! - `first_run`: Seam 1 on desktop for the first run, a fresh install
//!   signing in and opening a setlist over MCP (`just test-first-run`);
//! - `org_create`: Seam 1 on desktop for creating an organization, on
//!   `first_run`'s harness (`just test-org-create`).
//!
//! All are ignored unless their recipe runs them.

mod first_run;
mod org_create;
mod web_boot;
