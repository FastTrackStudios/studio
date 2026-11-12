//! The release's `SHA256SUMS`, one file over the tag's ONE GitHub Release,
//! signed with the org's minisign key (`SHA256SUMS.minisig`), as Architect's
//! release signs its own (architect `release.yml`, job `sums`).
//!
//! More than one workflow attaches to the Release (`desktop`: the
//! installers; `studio-architect`: the distribution's node and web
//! tarballs), each finishing when it finishes, and the Release is a DRAFT
//! until one of them has signed (release.sh `publish` creates it so). Each
//! workflow ends with these jobs; one concurrency group per tag runs them one
//! after the other.
//!
//! What gets signed is ONLY what a job can vouch for, never whatever is on
//! the Release when it runs (scripts/release-sums.sh):
//!   - the digests THIS workflow's build jobs reported (their `digests`
//!     output: the files they uploaded);
//!   - `release-facts.json`, recomputed from the tag (`release.sh facts`);
//!   - the SHA256SUMS already on the Release, only once its signature
//!     verifies with the org key and its trusted comment names this tag
//!     (the workflows that signed before).
//!
//! An asset it vouches for whose digest differs on the Release (replaced
//! after upload), or one of its own that is missing, makes it refuse. Any
//! other asset is left out of the sums, unsigned. Then the draft is
//! published.
//!
//! - `sums` signs: it runs in the `release-signing` environment (tags `v*`
//!   only: the org key is in no other job), so only on a tag.
//! - `sums-unsigned` is the same on a run by hand from a branch, which the
//!   environment refuses: SHA256SUMS alone, a stale `.minisig` deleted, and
//!   nothing published.
//!
//! minisign is the pinned upstream release binary (INSTALL_MINISIGN), not
//! whatever apt has today.

use crate::model::{Job, Step, UBUNTU, pinned, yaml};
use anyhow::Result;

/// The org key's public half (task apps/server/src/self_update.rs
/// `RELEASE_SIGNING_KEY`, architect release.yml `ORG_RELEASE_KEY`): nothing
/// is signed with any other key.
pub const ORG_RELEASE_KEY: &str = "RWRh9Gk9qaStQMhku4THMidEYrQsmAEsm2dDprUkZxShEtqfIcroM8Od";

const ON_TAG: &str = "startsWith(github.ref, 'refs/tags/v')";

/// minisign 0.12, the upstream Linux binary, by sha256: onto PATH.
pub const INSTALL_MINISIGN: &str = r#"set -euo pipefail
curl -fsSL --retry 3 -o "$RUNNER_TEMP/minisign.tgz" https://github.com/jedisct1/minisign/releases/download/0.12/minisign-0.12-linux.tar.gz
echo "9a599b48ba6eb7b1e80f12f36b94ceca7c00b7a5173c95c3efc88d9822957e73  $RUNNER_TEMP/minisign.tgz" | sha256sum -c -
tar -xzf "$RUNNER_TEMP/minisign.tgz" -C "$RUNNER_TEMP" --warning=no-unknown-keyword minisign-linux/x86_64/minisign
echo "$RUNNER_TEMP/minisign-linux/x86_64" >> "$GITHUB_PATH"
"#;

const SUMS: &str = r####"set -euo pipefail
dir="$RUNNER_TEMP/sums"
rm -rf "$dir" && mkdir -p "$dir"
printf '%s\n' "$OWN" | sed 's/^[[:space:]]*//; /^$/d' > "$dir/own"
# release-facts.json as the tag says it must be.
bash scripts/release.sh facts "$TAG" > "$dir/release-facts.json"
facts_sha="$(sha256sum < "$dir/release-facts.json" | cut -d' ' -f1)"
printf '%s  release-facts.json\n' "$facts_sha" >> "$dir/own"
id="$(gh api --paginate "repos/$GITHUB_REPOSITORY/releases?per_page=100" --jq ".[] | select(.tag_name == \"$TAG\") | .id" | sort -n | head -1)"
[ -n "$id" ] || { echo "::error::no release for $TAG (release.yml's publish job creates it)"; exit 1; }
assets() { gh api "repos/$GITHUB_REPOSITORY/releases/$id/assets?per_page=100" --jq '[.[] | {name, digest}]'; }
assets > "$dir/assets.json"
# Never over a signed release from a branch run: it would replace the signed
# sums with this workflow's alone and drop the signature. Re-run a tag with
# `gh workflow run <workflow> --ref vX.Y.Z` (a tag ref: the signed path).
if [ "$SIGNED" != true ] && jq -e 'map(.name) | index("SHA256SUMS.minisig")' "$dir/assets.json" >/dev/null; then
  echo "::error::$TAG already has a signed SHA256SUMS: an unsigned (branch) run must not touch it; dispatch with --ref $TAG"
  exit 1
fi
# The facts on the release: put them there when the release job's upload
# did not make it (re-running the release job cannot), refuse other ones.
have="$(jq -r '.[] | select(.name == "release-facts.json") | .digest // ""' "$dir/assets.json")"
if [ -z "$have" ]; then
  for _ in 1 2 3; do gh release upload "$TAG" --repo "$GITHUB_REPOSITORY" --clobber "$dir/release-facts.json" && break; sleep 5; done
  assets > "$dir/assets.json"
elif [ "$have" != "sha256:$facts_sha" ]; then
  echo "::error::release-facts.json on $TAG is $have, the tag says sha256:$facts_sha: refusing"
  exit 1
fi
# The workflows that signed before: trusted only through their signature.
prev=/dev/null
if jq -e 'map(.name) | index("SHA256SUMS")' "$dir/assets.json" >/dev/null; then
  gh release download "$TAG" --repo "$GITHUB_REPOSITORY" --dir "$dir/prev" -p SHA256SUMS -p SHA256SUMS.minisig
  if [ "$SIGNED" = true ]; then
    sig="$dir/prev/SHA256SUMS.minisig"
    if [ ! -f "$sig" ] || [ "$(wc -l < "$sig")" -ne 4 ] \
       || [ "$(sed -n 3p "$sig")" != "trusted comment: studio $TAG SHA256SUMS" ] \
       || ! minisign -V -q -P "$ORG_RELEASE_KEY" -m "$dir/prev/SHA256SUMS" -x "$sig" >/dev/null; then
      echo "::error::the SHA256SUMS on the $TAG release does not verify with the org key for this tag: refusing to build on it"
      exit 1
    fi
    prev="$dir/prev/SHA256SUMS"
  fi
fi
bash scripts/release-sums.sh plan "$dir/own" "$dir/assets.json" "$prev" > "$dir/SHA256SUMS"
cat "$dir/SHA256SUMS"
upload=("$dir/SHA256SUMS")
if [ -n "${MINISIGN_KEY:-}" ]; then
  key="$RUNNER_TEMP/release.key"
  ( umask 077; printf '%s\n' "$MINISIGN_KEY" > "$key" )
  minisign -S -s "$key" -m "$dir/SHA256SUMS" -x "$dir/SHA256SUMS.minisig" -t "studio $TAG SHA256SUMS"
  rm -f "$key"
  minisign -V -P "$ORG_RELEASE_KEY" -m "$dir/SHA256SUMS" -x "$dir/SHA256SUMS.minisig"
  upload+=("$dir/SHA256SUMS.minisig")
elif [ "$SIGNED" = true ]; then
  echo "::warning::release-signing has no FTS_RELEASE_MINISIGN_KEY: SHA256SUMS goes out unsigned"
fi
gh release upload "$TAG" --repo "$GITHUB_REPOSITORY" --clobber "${upload[@]}"
if [ "${#upload[@]}" = 1 ] && jq -e 'map(.name) | index("SHA256SUMS.minisig")' "$dir/assets.json" >/dev/null; then
  gh release delete-asset "$TAG" SHA256SUMS.minisig --repo "$GITHUB_REPOSITORY" --yes
fi
# Nothing moved while signing; then publish (a signed run only): ONLY a
# draft, and Latest only for the highest stable tag (scripts/release-publish.sh).
assets > "$dir/assets.json"
bash scripts/release-sums.sh plan "$dir/own" "$dir/assets.json" "$prev" > /dev/null
if [ "$SIGNED" = true ] && [ "${#upload[@]}" = 2 ]; then
  draft="$(gh api "repos/$GITHUB_REPOSITORY/releases/$id" --jq .draft)"
  highest="$(git tag --list 'v[0-9]*' | grep -E '^v[0-9]+\.[0-9]+\.[0-9]+$' | sort -V | tail -1)"
  case "$(bash scripts/release-publish.sh decide "$draft" "$TAG" "$highest")" in
    publish-latest) gh release edit "$TAG" --repo "$GITHUB_REPOSITORY" --draft=false --latest ;;
    publish) gh release edit "$TAG" --repo "$GITHUB_REPOSITORY" --draft=false --latest=false ;;
    leave) echo "$TAG is published already: Latest left where it is" ;;
  esac
fi
{
  echo "### SHA256SUMS of $TAG ($(wc -l < "$dir/SHA256SUMS") assets$( [ "${#upload[@]}" = 2 ] && echo ', signed, published'))"
  echo '```'
  cat "$dir/SHA256SUMS"
  echo '```'
} >> "$GITHUB_STEP_SUMMARY"
"####;

fn job(needs: &[&str], cond: &str, own: &str, signed: bool) -> Result<Job> {
    let on = if signed {
        format!("${{{{ {ON_TAG} && {cond} }}}}")
    } else {
        format!("${{{{ !{ON_TAG} && {cond} }}}}")
    };
    let mut step = Step::run(
        if signed {
            "Sum what this workflow built (and the signed sums), sign, publish"
        } else {
            "Sum what this workflow built (unsigned: not a tag ref)"
        },
        SUMS,
    )
    .env("GH_TOKEN", "${{ github.token }}")
    .env("ORG_RELEASE_KEY", ORG_RELEASE_KEY)
    .env("SIGNED", if signed { "true" } else { "false" })
    .env("OWN", own);
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
        // One at a time per tag, across workflows: each builds on the last.
        .concurrency("release-sums-${{ inputs.tag || github.ref_name }}", false)
        .step(
            Step::action(pinned::CHECKOUT)
                .with("ref", "refs/tags/${{ inputs.tag || github.ref_name }}")
                .with("fetch-depth", 0)
                .with("persist-credentials", false),
        )
        .step(Step::run("minisign (pinned)", INSTALL_MINISIGN))
        .step(step);
    if signed {
        job = job.environment("release-signing");
    }
    Ok(job)
}

/// The two jobs, `sums` and `sums-unsigned`, after `needs`, when `cond`
/// (an expression without `${{ }}`); `own` is the expression for the
/// "<sha256>  <name>" lines this workflow's build jobs reported.
pub fn jobs(needs: &[&str], cond: &str, own: &str) -> Result<[(&'static str, Job); 2]> {
    Ok([
        ("sums", job(needs, cond, own, true)?),
        ("sums-unsigned", job(needs, cond, own, false)?),
    ])
}
