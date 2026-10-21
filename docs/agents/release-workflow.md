# Releases: dev first, main is production

studio has two long-lived branches:

- **`dev`** — the default branch and the integration branch. Everything
  lands here: the Captain's pushes, agents' PRs (squash-merged), the
  release commit.
- **`main`** — production. It only ever receives release merges from
  `dev` (and `hotfix/*`). A push to main deploys the sites, uploads the
  iOS apps to TestFlight and tags the release.

CI is spent once per release, not once per push.

## What runs where

| Event | Workflows | Runner |
|---|---|---|
| PR into `dev` | `pr-title` (a regex on the title, seconds) | GitHub-hosted |
| push to `dev` | nothing | — |
| release PR `dev` → `main` (or `hotfix/*` → `main`) | `checks` (the full `just ci` set + advisory clippy), `release` guard | THEBATTLESHIP (`session`), guard GitHub-hosted |
| push to `main` (the release merge) | `deploy` (session-web, signal-web), `session-ios` / `signal-ios` (path-filtered), `release` publish (tag `v<version>` + GitHub Release, then dispatches `reaper-integration` on the tag), `cache-devshell` (flake changes only) | `session`, `airlock`, GitHub-hosted |
| by hand | any of them: `gh workflow run <file> --ref <branch>` | |

The gate for work going into `dev` is a **local `just ci`** on the commit
being pushed (it runs exactly what `checks.yml` runs, in the same order).
An agent's PR into dev carries that evidence in its description or card:
the head sha, `origin/dev`'s sha, the nextest summary and the host.

## Day to day

```bash
git switch dev && git pull            # once: track dev instead of main
just ci                               # before pushing
git push origin HEAD:dev              # or open a PR into dev
```

PR titles into dev must be conventional commits — `feat(editor): …`,
`fix: …`, `chore(deps)!: …` — because a squash merge turns the title into
the commit subject and git-cliff turns that into the changelog line.
Merge PRs into dev with **squash**, and give `gh pr merge` a conventional
`--subject` when the title is not one. Commits pushed directly with prose
subjects are fine; they land under "Changes".

## Cutting a release

In the dev shell, on a clean checkout:

```bash
just release-plan          # next version, unreleased changelog, git deps moved
just release-prepare       # release/vX.Y.Z off origin/dev: version bump, Cargo.lock, CHANGELOG.md
git push origin release/vX.Y.Z:dev   # fast-forward dev to the release commit
                                     # (or a PR into dev titled "chore(release): vX.Y.Z", squashed)
just release-pr            # opens dev -> main "Release vX.Y.Z" with the notes as the body
```

Nothing else may land on dev between `release-prepare` and the release
merge: the guard requires the head commit to be `chore(release): vX.Y.Z`.

On the release PR, `checks` and the `Release guard` must be green. The
guard fails the PR unless:

- the head is `dev` (or `hotfix/*`),
- `[workspace.package] version` is greater than main's and has no tag yet,
- `CHANGELOG.md` has that version's section,
- the head commit is the release commit,
- merging produces exactly the head's tree (main has nothing dev lacks).

Merge with **Create a merge commit** — never squash or rebase, or main
gets a commit dev does not have and the next release's guard refuses.
After the merge, `release` publishes `vX.Y.Z` on the dev tip (the merge's
second parent) and starts the REAPER suites on the tag; check that
`deploy` went green (it only does once the sites serve the new sha).

Versions are `0.x` for now: a feature is a minor, a breaking change also
a minor. `just release-prepare patch|minor|major|X.Y.Z` overrides the
suggestion; 1.0.0 is a decision, made explicitly.

Per-app releases (Signal and Session builds, the plugins, the macOS and
Linux installers) carry their own `<app>-v*` tags and are not the repo's
`v*` tag; cliff.toml's `tag_pattern` ignores them.

## Hotfix

```bash
git switch -c hotfix/<name> origin/main
# commit the fix
just release-hotfix        # patch bump + changelog + release commit
git push origin hotfix/<name>   # PR -> main: checks + guard; merge commit
```

Then back-merge: a PR `main` → `dev` merged with a merge commit, so the
next release's tree identity holds.

## Rollback

- **Sites**: `deploy` tags every image `sha-<short>`. Point
  argocd-image-updater back at the previous release's `sha-…` tag (or
  revert the release merge on main with a PR, which redeploys the old
  tree). Nothing in studio's sites holds data, so an image swap is a full
  rollback.
- **TestFlight**: expire the build in App Store Connect; the previous
  build stays installable.
- **The workflow itself**: set the default branch back to `main` and
  restore `pull_request:` (no branch filter) in `checks.yml`; `dev` can
  stay.

## Branches

The Captain's feature branches (`editing-engine`, …) are cut from and
merged into `dev`. A branch that was cut from main before the switch and
is not behind it fast-forwards onto dev unchanged
(`git push origin <branch>:dev` after `just ci`).
