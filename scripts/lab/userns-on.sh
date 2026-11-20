#!/bin/sh
# SPDX-License-Identifier: 0BSD
# The class userns-on (T-564): a user with no root can make a user
# namespace with mounts, so podvm takes its namespace rung. GitHub's
# Ubuntu 24.04 runners restrict that by AppArmor, so each other lane runs
# podvm on its supervise or userland rung. Needs root for the sysctl: a
# disposable host only.
set -eu
[ "${1:-}" = "--" ] && shift
if [ -e /proc/sys/kernel/apparmor_restrict_unprivileged_userns ]; then
    sudo sysctl -q -w kernel.apparmor_restrict_unprivileged_userns=0
fi
sudo sysctl -q -w user.max_user_namespaces=63000
# The check as a user with no root: a user namespace and a mount in it.
if ! sudo -u nobody unshare -Urm sh -c 'mount -t tmpfs none /mnt'; then
    echo "lab userns-on: a user with no root cannot mount in a user namespace" >&2
    exit 70
fi
echo "lab userns-on: a user with no root mounts in a user namespace (the lane checks the rung of podvm too)" >&2
exec "$@"
