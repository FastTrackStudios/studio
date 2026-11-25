# studio-adapters' recipes. The REAPER ones (the bridge, daw-reaper's
# integration suites against a real REAPER) are the `reaper` module, as in
# Studio: `just reaper daw-test`, `just reaper install`, …

mod reaper 'features/adapters/reaper/reaper.just'

default:
    @just --list

# Link the test fixtures over 1 MB (the private studio-assets repo; see
# assets.txt) into place. Needs read access to that repo.
assets:
    bash scripts/assets.sh
