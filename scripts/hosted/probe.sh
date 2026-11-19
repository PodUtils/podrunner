#!/bin/sh
# SPDX-License-Identifier: 0BSD
# The probe of the hosted parity (T-539): the facts of this machine that a
# job of the GitHub-hosted runner can see, one "KEY<TAB>VALUE" line each,
# sorted. scripts/hosted/diff.py compares the probe of a hosted label with
# the probe of a unit. POSIX sh and the tools of a small image only: a
# fact whose tool is not there is left out (a missing key), not an error.
#
#   sh scripts/hosted/probe.sh > probe.tsv
#
# The values that change for each job (run ids, names of the repository,
# times, free space) are not facts here, or they are normalized.
set -u
LC_ALL=C
export LC_ALL

out() { printf '%s\t%s\n' "$1" "$2"; }
have() { command -v "$1" >/dev/null 2>&1; }
# The first token of the first lines of "$@" that looks like a version:
# a word prefix ("go1.24.1", "jq-1.7.1", "v22.1.0") and colour codes go.
esc=$(printf '\033')
ver() {
  "$@" 2>&1 </dev/null | sed "s/$esc\[[0-9;]*m//g" | sed -n '1,6p' | tr ' ,()"' '\n\n\n\n\n' |
    sed -n 's/^[A-Za-z]*-\{0,1\}v\{0,1\}\([0-9][0-9]*\(\.[0-9A-Za-z+~-]*\)\{1,\}\)$/\1/p' | sed -n 1p
}
tool() {
  name=$1
  shift
  have "$1" || return 0
  v=$(ver "$@")
  out "tool.$name" "${v:-present}"
}

{
  # The OS and the image.
  if [ -r /etc/os-release ]; then
    # shellcheck disable=SC1091
    . /etc/os-release
    out os.id "${ID:-}"
    out os.version_id "${VERSION_ID:-}"
  fi
  out image.os "${ImageOS:-}"
  out arch "$(uname -m)"

  # The tools of the software report, by the command that a job runs.
  tool 7z 7z i
  tool ansible ansible --version
  tool aws aws --version
  tool az az version
  tool bazel bazel --version
  tool buildah buildah --version
  tool cargo cargo --version
  tool clang clang --version
  tool cmake cmake --version
  tool composer composer --version
  tool curl curl --version
  tool docker docker --version
  tool docker-buildx docker buildx version
  tool docker-compose docker compose version
  tool dotnet dotnet --version
  tool firefox firefox --version
  tool g++ g++ --version
  tool gcc gcc --version
  tool gcloud gcloud --version
  tool gh gh --version
  tool git git --version
  tool git-lfs git-lfs --version
  tool go go version
  tool google-chrome google-chrome --version
  tool gradle gradle --version
  tool helm helm version --short
  tool java java -version
  tool jq jq --version
  tool julia julia --version
  tool kotlin kotlin -version
  tool kubectl kubectl version --client
  tool make make --version
  tool mvn mvn --version
  tool mysql mysql --version
  tool ninja ninja --version
  tool node node --version
  tool npm npm --version
  tool php php --version
  tool pip3 pip3 --version
  tool pipx pipx --version
  tool podman podman --version
  tool psql psql --version
  tool pwsh pwsh --version
  tool python3 python3 --version
  tool ruby ruby --version
  tool rustc rustc --version
  tool rustup rustup --version
  tool skopeo skopeo --version
  tool sqlite3 sqlite3 --version
  tool sudo sudo --version
  tool swift swift --version
  tool systemctl systemctl --version
  tool wget wget --version
  tool yarn yarn --version
  tool yq yq --version
  tool zstd zstd --version

  # The packages of the distribution, with their versions.
  if have dpkg-query; then
    dpkg-query -W -f '${Package}\t${Version}\n' 2>/dev/null | sed 's/^/dpkg./'
  fi

  # The tool cache: the versions of each tool.
  cache=${RUNNER_TOOL_CACHE:-/opt/hostedtoolcache}
  out env.tool_cache_dir "$cache"
  if [ -d "$cache" ]; then
    for d in "$cache"/*/; do
      [ -d "$d" ] || continue
      n=${d%/}
      n=${n##*/}
      vs=$(ls "$d" 2>/dev/null | grep -v '\.complete$' | sort | tr '\n' ' ' | sed 's/ $//')
      out "toolcache.$n" "$vs"
    done
  fi

  # The environment: the names, and the values of the stable ones. The
  # names of a job (GITHUB_*, RUNNER_* of the run, INPUT_*, ACTIONS_*) are
  # left out.
  env | sed -n 's/^\([A-Za-z_][A-Za-z0-9_]*\)=.*/\1/p' | sort -u | while read -r n; do
    case $n in
      GITHUB_* | INPUT_* | ACTIONS_* | RUNNER_TEMP | RUNNER_TRACKING_ID | RUNNER_NAME | RUNNER_WORKSPACE | \
        STATE_* | JOB_* | INVOCATION_ID | JOURNAL_STREAM | SHLVL | PWD | OLDPWD | _ | TERM | SSH_* | LS_COLORS | \
        PODRUNNER_* | HTTP_PROXY | HTTPS_PROXY | NO_PROXY | http_proxy | https_proxy | no_proxy)
        continue
        ;;
    esac
    case $n in
      PATH | HOME | USER | SHELL | LANG | ImageOS | RUNNER_TOOL_CACHE | AGENT_TOOLSDIRECTORY | RUNNER_OS | \
        RUNNER_ARCH | RUNNER_ENVIRONMENT | XDG_RUNTIME_DIR | DOTNET_ROOT | JAVA_HOME | ANDROID_HOME | \
        ANDROID_SDK_ROOT | CHROMEWEBDRIVER | GECKOWEBDRIVER | EDGEWEBDRIVER | PIPX_HOME | PIPX_BIN_DIR | \
        CONDA | VCPKG_INSTALLATION_ROOT | SWIFT_PATH | GOROOT_* | JAVA_HOME_* | DEBIAN_FRONTEND | ACCEPT_EULA | \
        POWERSHELL_DISTRIBUTION_CHANNEL | DOTNET_* | NVM_DIR | HOMEBREW_* | BOOTSTRAP_HASKELL_NONINTERACTIVE | \
        GHCUP_INSTALL_BASE_PREFIX | AZURE_EXTENSION_DIR | LEIN_HOME | LEIN_JAR | SELENIUM_JAR_PATH)
        eval "v=\${$n-}"
        out "env.$n" "$v"
        ;;
      *) out "env.$n" set ;;
    esac
  done
  # The form of the work folder: /home/runner/work/R/R on the hosted runner.
  if [ -n "${GITHUB_WORKSPACE:-}" ]; then
    w=${GITHUB_WORKSPACE%/*}
    out env.workspace_form "${w%/*}/R/R"
  fi

  # The user, its groups and sudo.
  out user.name "$(id -un)"
  out user.uid "$(id -u)"
  out user.groups "$(id -Gn | tr ' ' '\n' | sort | tr '\n' ' ' | sed 's/ $//')"
  if have sudo && sudo -n true 2>/dev/null; then out user.sudo nopasswd; else out user.sudo no; fi
  if [ -r /etc/subuid ]; then
    out user.subuid "$(sed -n "s/^$(id -un):\(.*\)$/\1/p" /etc/subuid | sed -n 1p)"
  fi

  # The init and the services.
  out pid1 "$(cat /proc/1/comm 2>/dev/null)"
  if have systemctl; then
    out systemd.state "$(systemctl is-system-running 2>/dev/null)"
    systemctl list-units --type=service --state=failed --no-legend --plain 2>/dev/null |
      sed -n 's/^\([^ ]*\.service\).*/systemd.failed.\1\tfailed/p'
    systemctl list-unit-files --type=service --state=enabled --no-legend 2>/dev/null |
      sed -n 's/^\([^ ]*\.service\).*/systemd.enabled.\1\tenabled/p'
  fi

  # The limits of the user and of the kernel.
  out ulimit.nofile "$(ulimit -n)"
  out ulimit.nproc "$(ulimit -u 2>/dev/null)"
  for k in vm.max_map_count fs.inotify.max_user_watches fs.inotify.max_user_instances fs.file-max \
    kernel.pid_max net.core.somaxconn vm.swappiness kernel.unprivileged_userns_clone \
    kernel.apparmor_restrict_unprivileged_userns user.max_user_namespaces; do
    f=/proc/sys/$(printf '%s' "$k" | tr . /)
    [ -r "$f" ] && out "sysctl.$k" "$(cat "$f")"
  done
  if [ -f /sys/fs/cgroup/cgroup.controllers ]; then
    out cgroup.version 2
    out cgroup.controllers "$(tr ' ' '\n' < /sys/fs/cgroup/cgroup.controllers | sort | tr '\n' ' ' | sed 's/ $//')"
  elif [ -d /sys/fs/cgroup ]; then
    out cgroup.version 1
  fi

  # The kernel, its devices and its modules.
  out kernel.release "$(uname -r)"
  for d in /dev/fuse /dev/kvm /dev/net/tun /dev/loop-control; do
    [ -e "$d" ] && out "dev.$d" present
  done
  if [ -r /proc/swaps ]; then
    s=$(sed -n '2,$p' /proc/swaps | wc -l)
    [ "$s" -gt 0 ] && out swap on || out swap off
  fi
  if [ -d /proc/sys/fs/binfmt_misc ]; then
    for b in /proc/sys/fs/binfmt_misc/*; do
      n=${b##*/}
      case $n in register | status | '*') continue ;; esac
      out "binfmt.$n" present
    done
  fi

  # The file capabilities that a job uses (ping as runner).
  if have getcap; then
    for f in /usr/bin/ping /usr/bin/mtr-packet; do
      [ -e "$f" ] && out "cap.$f" "$(getcap "$f" 2>/dev/null | sed 's/^[^ ]* //')"
    done
  fi

  # The engines.
  [ -S /var/run/docker.sock ] && out docker.socket /var/run/docker.sock
  if have docker && docker info >/dev/null 2>&1; then out docker.daemon up; else out docker.daemon down; fi
  if have podman && podman info >/dev/null 2>&1; then out podman.info ok; fi
} | LC_ALL=C sort -u
