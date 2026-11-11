//! The release's `SHA256SUMS`, one file over every asset of the tag's ONE
//! GitHub Release, signed with the org's minisign key (`SHA256SUMS.minisig`),
//! as Architect's release signs its own (architect `release.yml`, job
//! `sums`).
//!
//! More than one workflow attaches to the Release (`desktop`: the
//! installers; `studio-architect`: the distribution's node and web
//! tarballs), each finishing when it finishes. So each ends with these
//! jobs, which sum EVERYTHING the Release holds at that moment, not only
//! what that workflow made: one concurrency group per tag runs them one
//! after the other, and the last to run has seen every asset. The sums are
//! GitHub's own `digest` of each asset (each publish job has checked that
//! digest is its file's sha256), so nothing is downloaded. The `.sha256`
//! sidecars and the sums themselves are left out, as Architect's are.
//!
//! - `sums` signs: it runs in the `release-signing` environment (tags `v*`
//!   only: the org key is in no other job), so only on a tag.
//! - `sums-unsigned` is the same on a run by hand from a branch (a
//!   `publish` dispatch re-running a tag from main), which the environment
//!   refuses: `SHA256SUMS` alone, and a stale `.minisig` deleted so no
//!   signature ever covers other sums.

use crate::model::{Job, Step, UBUNTU, yaml};
use anyhow::Result;

/// The org key's public half (task apps/server/src/self_update.rs
/// `RELEASE_SIGNING_KEY`, architect release.yml `ORG_RELEASE_KEY`): nothing
/// is signed with any other key.
const ORG_RELEASE_KEY: &str = "RWRh9Gk9qaStQMhku4THMidEYrQsmAEsm2dDprUkZxShEtqfIcroM8Od";

const ON_TAG: &str = "startsWith(github.ref, 'refs/tags/v')";

const SUMS: &str = r####"set -euo pipefail
assets="$(gh api "repos/$GITHUB_REPOSITORY/releases/tags/$TAG" --jq '.assets[]
  | select(.name != "SHA256SUMS" and .name != "SHA256SUMS.minisig" and (.name | endswith(".sha256") | not))
  | "\(.digest // "")  \(.name)"' | sort -k2)"
[ -n "$assets" ] || { echo "::error::the $TAG release has no assets to sum"; exit 1; }
if grep -qv '^sha256:[0-9a-f]\{64\}  ' <<<"$assets"; then
  echo "::error::an asset of $TAG has no sha256 digest from GitHub:"
  grep -v '^sha256:[0-9a-f]\{64\}  ' <<<"$assets"
  exit 1
fi
printf '%s\n' "${assets//sha256:/}" > SHA256SUMS
cat SHA256SUMS
if [ -n "${MINISIGN_KEY:-}" ]; then
  sudo apt-get install -y -q --no-install-recommends minisign >/dev/null
  key="$RUNNER_TEMP/release.key"
  ( umask 077; printf '%s\n' "$MINISIGN_KEY" > "$key" )
  minisign -S -s "$key" -m SHA256SUMS -x SHA256SUMS.minisig -t "studio $TAG SHA256SUMS"
  rm -f "$key"
  minisign -V -P "$ORG_RELEASE_KEY" -m SHA256SUMS -x SHA256SUMS.minisig
  gh release upload "$TAG" --repo "$GITHUB_REPOSITORY" --clobber SHA256SUMS SHA256SUMS.minisig
else
  if [ "$SIGNED" = true ]; then
    echo "::warning::release-signing has no FTS_RELEASE_MINISIGN_KEY: SHA256SUMS goes out unsigned"
  fi
  gh release upload "$TAG" --repo "$GITHUB_REPOSITORY" --clobber SHA256SUMS
  if gh release view "$TAG" --repo "$GITHUB_REPOSITORY" --json assets --jq '.assets[].name' | grep -qx 'SHA256SUMS.minisig'; then
    gh release delete-asset "$TAG" SHA256SUMS.minisig --repo "$GITHUB_REPOSITORY" --yes
  fi
fi
{
  echo "### SHA256SUMS of $TAG ($(wc -l < SHA256SUMS) assets$( [ -f SHA256SUMS.minisig ] && echo ', signed'))"
  echo '```'
  cat SHA256SUMS
  echo '```'
} >> "$GITHUB_STEP_SUMMARY"
"####;

fn job(needs: &[&str], cond: &str, signed: bool) -> Result<Job> {
    let on = if signed {
        format!("${{{{ {ON_TAG} && {cond} }}}}")
    } else {
        format!("${{{{ !{ON_TAG} && {cond} }}}}")
    };
    let mut step = Step::run(
        if signed {
            "Sum every asset, sign with the org key"
        } else {
            "Sum every asset (unsigned: not a tag ref)"
        },
        SUMS,
    )
    .env("GH_TOKEN", "${{ github.token }}")
    .env("ORG_RELEASE_KEY", ORG_RELEASE_KEY)
    .env("SIGNED", if signed { "true" } else { "false" });
    if signed {
        step = step.env("MINISIGN_KEY", "${{ secrets.FTS_RELEASE_MINISIGN_KEY }}");
    }
    let mut job = Job::new(UBUNTU)
        .name(if signed {
            "SHA256SUMS (signed)"
        } else {
            "SHA256SUMS (unsigned)"
        })
        .needs(needs)
        .cond(&on)
        .timeout(10)
        .permissions(yaml("contents: write\n")?)
        // One at a time per tag, across workflows: the last sees every asset.
        .concurrency("release-sums-${{ inputs.tag || github.ref_name }}", false)
        .step(step);
    if signed {
        job = job.environment("release-signing");
    }
    Ok(job)
}

/// The two jobs, `sums` and `sums-unsigned`, after `needs`, when `cond`
/// (an expression without `${{ }}`) holds.
pub fn jobs(needs: &[&str], cond: &str) -> Result<[(&'static str, Job); 2]> {
    Ok([
        ("sums", job(needs, cond, true)?),
        ("sums-unsigned", job(needs, cond, false)?),
    ])
}
