# SPDX-License-Identifier: 0BSD
# shellcheck shell=sh
# Load this file from the checkout root: . ./scripts/cloud-env.sh
: "${PODRUNNER_TOOLS:=$(pwd -P)/.work/tools/dev}"
: "${CARGO_HOME:=$PODRUNNER_TOOLS/cargo}"
: "${RUSTUP_HOME:=$PODRUNNER_TOOLS/rustup}"
: "${ZIG_GLOBAL_CACHE_DIR:=$PODRUNNER_TOOLS/zig-cache}"
: "${CARGO_ZIGBUILD_CACHE_DIR:=$PODRUNNER_TOOLS/zigbuild-cache}"
: "${CARGO_BUILD_JOBS:=4}"
: "${CARGO_NET_GIT_FETCH_WITH_CLI:=true}"
case ":$PATH:" in *":$PODRUNNER_TOOLS/bin:"*) ;; *) PATH=$PODRUNNER_TOOLS/bin:$PATH ;; esac
case ":$PATH:" in *":$CARGO_HOME/bin:"*) ;; *) PATH=$CARGO_HOME/bin:$PATH ;; esac
export PODRUNNER_TOOLS CARGO_HOME RUSTUP_HOME CARGO_BUILD_JOBS CARGO_NET_GIT_FETCH_WITH_CLI PATH
export ZIG_GLOBAL_CACHE_DIR CARGO_ZIGBUILD_CACHE_DIR

podrunner_runner_version=$(sed -n 's/^version = "\(.*\)"$/\1/p' images/runner/runner.toml)
podrunner_runner_cache=$PODRUNNER_TOOLS/runner-$podrunner_runner_version-$(uname -m)
if [ -z "${PODRUNNER_RUNNER_DIR:-}" ] && [ -x "$podrunner_runner_cache/bin/Runner.Listener" ]; then
    PODRUNNER_RUNNER_DIR=$podrunner_runner_cache
    export PODRUNNER_RUNNER_DIR
fi
