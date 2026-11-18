#!/usr/bin/env bash
# voyager: the org's host node and window
R=/tmp/orgrun
export ARCHITECT_NAME=voyager ARCHITECT_BIND=0.0.0.0:7412 ARCHITECT_DATA_DIR=$R/a/node
export ARCHITECT_VAULTS=notes=$R/a/vault ARCHITECT_ISSUER=http://127.0.0.1:8080
export ARCHITECT_SESSION_FILE=$R/session-a ARCHITECT_HOST_ORG=acme
export ARCHITECT_IROH=n0 ARCHITECT_IROH_BIND=0.0.0.0:7777 ARCHITECT_SWEEP_SECS=5
export ARCHITECT_CONFIG_DIR=$R/a/app ARCHITECT_FORBID_KEYCHAIN=1 ARCHITECT_SECRET_STORE=file:$R/a/app/secrets.json
export ARCHITECT_VAULT=$R/a/vault FTS_CONTROL=1 RUST_LOG=info
export IGNITION_ROOT=/Volumes/dev-drive/wt/ignition-session/features/lighting
export FTS_DAW_OPEN="$R/a/vault/Songs/Drive To LA/Drive To LA.rpp" FTS_DAW_BAR_ONE=5
export FTS_DAW_SESSION="$R/a/vault/Songs/Drive To LA.session" FTS_DAW_DOCUMENT="Songs/Drive To LA.session"
exec /Volumes/dev-drive/wt/arch-app-870/target/debug/architect-desktop
