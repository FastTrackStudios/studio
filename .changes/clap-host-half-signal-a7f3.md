---
"studio-engine" = "minor"
---

Studio is the host half of FastTrackStudio's CLAP extensions: every CLAP the engine loads is offered host-actions/1 over the Action List (list, describe, run as the song's agent on a worker, a slow action a job with a `job` event; subscriptions to `transport`, `tempo`, `hits:<piece>` after each edit step, and `job`; files by path or `rig-library:` reference), a main-thread pump runs plugins' main-thread requests and refreshes the chain's views and automation lanes when a plugin rescans its parameters, and Signal is read and driven by track: SIGNAL_CHAIN (its signal.chain/1, read again only when its revision moved), SIGNAL_RUN (any Signal action), SIGNAL_SESSION (every port by session) and SIGNAL_FEED (each instance's chain revision and meter values for the mixer, also a PubSub stream) (signal)
