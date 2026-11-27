//! Studio's integration tests, one binary (the `rust-testing` skill):
//!
//! - `web_boot`: Seam 3 for the web, the web build opening and playing a
//!   fixture session in a browser (`just test-web-boot`);
//! - `first_run`: Seam 1 on desktop for the first run, a fresh install
//!   signing in and opening a setlist over MCP (`just test-first-run`);
//! - `org_create`: Seam 1 on desktop for creating an organization, on
//!   `first_run`'s harness (`just test-org-create`);
//! - `org_invites`: Seam 1 on desktop for an org's people, an owner
//!   inviting, a second account joining and being removed, on the same
//!   harness (`just test-org-invites`);
//! - `web_room`: Seam 2 with a browser member, the web build joining a
//!   room a desktop engine leads, by its link, and following the leader,
//!   observed from the leader's MCP (`just test-web-room`).
//!
//! All are ignored unless their recipe runs them.

mod first_run;
mod org_create;
mod org_invites;
mod web_boot;
mod web_room;
