//! `deploy` — a pure-Nix OCI image pushed to the in-cluster registry, rolled
//! out by ArgoCD + argocd-image-updater (digest strategy on the channel tag).
//!
//! ```text
//!   session-web -> registry.starcommand.live:30050/session-web  (session.fasttrackstudio.app)
//!   signal-web  -> registry.starcommand.live:30050/signal-web   (signal.fasttrackstudio.app)
//! ```
//!
//! One matrix leg per site; a failing leg does not stop the other.
//!
//! The in-cluster registry is NOT reachable from GitHub-hosted runners, so
//! this runs on the org's SELF-HOSTED runner labelled `studio-deploy` (the
//! Mac Pro VM; its own label so a deploy never queues behind a check
//! build). That runner is registered on the FastTrackStudios org, so it
//! reaches this repo; it does not reach personal-account repos. It keeps
//! nix: the images are nix builds (`streamLayeredImage`), and nix is that
//! host's own package manager.
//!
//! GitHub Deployments: the `environment:` block registers a real
//! Deployment, so runs appear under the repo's Environments tab with a link
//! to the live site. It only goes green after "Verify live" passes — so
//! "Active" means the cluster is SERVING this sha, not merely that an image
//! was pushed.
//!
//! Production is the latest `v*` TAG, not main (fleet release flow v4,
//! docs/agents/release-workflow.md): every PR squash-merges into main, and a
//! push to main deploys nothing. This runs only when a `vYY.M.MICRO` tag is
//! pushed (`just release-tag`) or dispatched with a tag as its ref (a
//! rollback: the previous tag); the first step refuses a tag that is not a
//! release commit on main or on a release/v* hotfix line, or whose `Build +
//! test` is not green. Both sites deploy the tagged commit, once per
//! release.
//!
//! No path filtering. session-web's demo backend is the real in-process
//! daw-standalone/session stack, so a change almost anywhere in the
//! workspace can change what the site does — a path filter would either be
//! wrong or be a list of everything.

use crate::model::{Job, RunsOn, Step, Workflow, yaml};
use crate::steps::checkout_full;
use anyhow::Result;

pub fn workflow() -> Result<Workflow> {
    Ok(Workflow::new(
        "deploy",
        yaml(
            r#"
push:
  tags: ["v*"]
workflow_dispatch:
"#,
        )?,
    )
    // Never cancel a deploy mid-push; queue instead. A cancelled deploy can
    // leave the registry holding a tag the cluster never rolled.
    .concurrency("deploy-${{ github.ref }}", false)
    .env("REGISTRY", "registry.starcommand.live:30050")
    // Newer skopeo rejects the host's v1 registries.conf; every registry is
    // named explicitly below, so an empty (valid) config is enough.
    .env("CONTAINERS_REGISTRIES_CONF", "/dev/null")
    .job("deploy", deploy()?))
}

fn deploy() -> Result<Job> {
    Ok(Job::new(RunsOn::Labels(&["self-hosted", "studio-deploy"]))
        .name("deploy ${{ matrix.img }}")
        // The repo default token is read-only with no `checks` scope; the
        // tag guard reads the `CI passed` check of the tagged commit.
        .permissions(yaml("contents: read\nchecks: read\n")?)
        .strategy(yaml(
            r"
fail-fast: false
matrix:
  include:
    - img: session-web
      env: session-production
      url: https://session.fasttrackstudio.app
    - img: signal-web
      env: signal-production
      url: https://signal.fasttrackstudio.app
",
        )?)
        .environment(yaml("name: ${{ matrix.env }}\nurl: ${{ matrix.url }}\n")?)
        .step(checkout_full(None))
        // Production changes only by tag: refuse a branch ref, a tag whose
        // version is not Cargo.toml's, a commit on neither main nor a
        // release/v* line, a commit whose `CI passed` is not green.
        .step(
            Step::run(
                "The ref is a green release tag on main or release/v*",
                "bash scripts/release.sh tag-guard",
            )
            .env("GH_TOKEN", "${{ github.token }}"),
        )
        .step(
            Step::run(
                "Compute tags",
                "\
echo \"sha=sha-$(git rev-parse --short HEAD)\" >> \"$GITHUB_OUTPUT\"
echo \"version=${GITHUB_REF_NAME}\" >> \"$GITHUB_OUTPUT\"
",
            )
            .id("tags"),
        )
        .step(Step::run("Push image", PUSH_IMAGE))
        .step(Step::run("Verify live", VERIFY_LIVE)))
}

/// streamLayeredImage builds an EXECUTABLE that streams a docker-archive to
/// stdout — there is no tarball on disk to copy, which is what keeps a ~60
/// MB bundle out of the store twice. The other tags are copied from the
/// just-pushed :latest rather than re-streamed: same digest by
/// construction, and the image is built once.
const PUSH_IMAGE: &str = r#"set -euo pipefail
img=${{ matrix.img }}
echo "── $img ──"
streamer="$(nix build .#${img}-image --no-link --print-out-paths)"
"$streamer" | nix run nixpkgs#skopeo -- --insecure-policy copy \
  --dest-tls-verify=false --dest-no-creds docker-archive:/dev/stdin \
  "docker://${REGISTRY}/${img}:latest"
for tag in dev "${{ steps.tags.outputs.sha }}" "${{ steps.tags.outputs.version }}"; do
  nix run nixpkgs#skopeo -- --insecure-policy copy \
    --src-tls-verify=false --src-no-creds \
    --dest-tls-verify=false --dest-no-creds \
    "docker://${REGISTRY}/${img}:latest" \
    "docker://${REGISTRY}/${img}:${tag}"
done
"#;

/// A green run must mean the cluster serves THIS commit. The bundle reports
/// the rev it was built from in version.json (written by mkStaticSite), so
/// this is the site answering, not the registry.
const VERIFY_LIVE: &str = r#"url="${{ matrix.url }}"
want="${GITHUB_SHA}"
echo "waiting for $url to serve $want"
for i in $(seq 1 60); do
  got="$(curl -sf --max-time 10 "$url/version.json" \
    | sed -n 's/.*"rev":"\([^"]*\)".*/\1/p' || true)"
  echo "  poll $i: ${got:-?}"
  if [ "$got" = "$want" ]; then
    echo "LIVE: $url serves $want"
    exit 0
  fi
  sleep 15
done
echo "TIMED OUT: $url never served $want — check argocd-image-updater / rollouts"
exit 1
"#;
