#!/usr/bin/env bash
# What a signed `sums` run does with the Release once it has signed
# (sums.rs; tested by scripts/release-sums-test.sh):
#
#   scripts/release-publish.sh decide <is_draft> <tag> <highest stable tag>
#       publish-latest   a draft of the highest stable tag: publish, mark Latest
#       publish          a draft of an older tag (a hotfix line, a re-run of
#                        an old tag): publish, never Latest
#       leave            already published (the second workflow's run, a
#                        re-run): change nothing, Latest stays where it is
#
# Marking an older tag Latest would make `scripts/install.sh` install it and
# infra's `release-pin.sh latest` see a release older than its pin.
set -euo pipefail
case "${1:-}" in
    decide)
        [ "$#" -eq 4 ] || { echo "usage: release-publish.sh decide <is_draft> <tag> <highest>" >&2; exit 1; }
        if [ "$2" != true ]; then echo leave
        elif [ "$3" = "$4" ]; then echo publish-latest
        else echo publish; fi ;;
    *) echo "usage: release-publish.sh decide <is_draft> <tag> <highest>" >&2; exit 1 ;;
esac
