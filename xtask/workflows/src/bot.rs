//! The release bots' App tokens: the fts-forge App, narrowed when minted.
//!
//! The App is installed org-wide with broad permissions, so each job that
//! needs it:
//! - runs in its own GitHub environment and reads the key under a secret
//!   name that exists in THAT environment only (`PIN_BUMP_APP_KEY`,
//!   `RELEASE_DISPATCH_APP_KEY`, ...), never `FORGE_APP_PRIVATE_KEY`
//!   (an org secret and repo secrets carry that name, and a job whose
//!   environment lacks the key would silently fall back to them);
//! - first checks it really runs there: the environment's own variable
//!   `BOT_ENVIRONMENT` names it, and its key is set. A job moved out of
//!   its environment, or an environment without its key, stops here,
//!   before any token exists;
//! - mints a token for exactly ONE repository with only the permissions
//!   its hop needs, and uses it in one step's environment only (never a
//!   file, a job output or an artifact).
//!
//! scripts/check-token-mints.py (PR checks) fails any workflow that mints
//! outside its allow-list.

use crate::model::Step;

/// Fail unless the job runs in `environment` and that environment holds
/// `secret`.
pub fn environment_guard(environment: &str, secret: &str) -> Step {
    Step::run(
        &format!("Runs in the {environment} environment, with its key"),
        &format!(
            r#"set -euo pipefail
if [ "${{BOT_ENVIRONMENT:-}}" != "{environment}" ] || [ "$KEY_SET" != true ]; then
  echo "::error::this job must run in the '{environment}' environment, whose variable BOT_ENVIRONMENT is '{environment}' and which holds the secret {secret} (got BOT_ENVIRONMENT='${{BOT_ENVIRONMENT:-}}', key set: $KEY_SET)"
  exit 1
fi
"#
        ),
    )
    .env("BOT_ENVIRONMENT", "${{ vars.BOT_ENVIRONMENT }}")
    .env("KEY_SET", &format!("${{{{ secrets.{secret} != '' }}}}"))
}

/// The fts-forge App's token for one `repository`, with `permissions`
/// only, from `secret` (the environment's own copy of the key).
pub fn app_token(secret: &str, repository: &str, permissions: &[(&str, &str)]) -> Step {
    let mut step = Step::uses(
        &format!("fts-forge token ({repository} only)"),
        crate::model::pinned::CREATE_APP_TOKEN,
    )
    .id("forge")
    .with("app-id", "${{ vars.FORGE_APP_ID }}")
    .with("private-key", format!("${{{{ secrets.{secret} }}}}"))
    .with("owner", "FastTrackStudios")
    .with("repositories", repository);
    for (scope, level) in permissions {
        step = step.with(&format!("permission-{scope}"), *level);
    }
    step
}
