#!/bin/sh
# Measure a container engine inside a guest of the machine tier of podvm
# (T-103, step 3): the static Docker binaries start in the guest, and a
# container from a saved image runs with no network. It prints one line
# for each property: "engine NAME ok" or "engine NAME failed: why".
#
#   sh scripts/measure-engine.sh PODVM STORE IMAGE WORKDIR DOCKER_TGZ IMAGE_TAR
#
# DOCKER_TGZ is the static Docker archive of download.docker.com; IMAGE_TAR
# a `docker save` of a small image with `sh`, named `measure:latest`.
set -u
podvm=$1
store=$2
image=$3
work=$4
tgz=$5
saved=$6
mkdir -p "$work/vol"
chmod 0777 "$work/vol"
tar -xzf "$tgz" -C "$work/vol"
cp "$saved" "$work/vol/image.tar"
export PODVM_STORE="$store"

out=$(timeout 600 "$podvm" run --rm --podvm-tier=machine --podvm-mem "${MACHINE_MEM:-4G}" --podvm-cpus 2 \
  -v "$work/vol:/mnt/engine" "$image" /bin/sh -c '
  echo "guest kernel $(uname -r)"
  echo "guest cgroup $(stat -fc %T /sys/fs/cgroup 2>&1)"
  for fs in overlay tmpfs; do grep -qw "$fs" /proc/filesystems && echo "guest fs $fs yes" || echo "guest fs $fs no"; done
  export PATH=/mnt/engine/docker:$PATH
  mkdir -p /var/lib/docker /run
  dockerd --iptables=false --ip6tables=false --bridge=none --host=unix:///run/docker.sock > /tmp/dockerd.log 2>&1 &
  i=0; while [ $i -lt 60 ] && ! docker -H unix:///run/docker.sock info >/dev/null 2>&1; do sleep 1; i=$((i+1)); done
  if docker -H unix:///run/docker.sock info >/dev/null 2>&1; then
    echo "guest dockerd up after ${i}s"
    docker -H unix:///run/docker.sock info --format "guest driver {{.Driver}} cgroup {{.CgroupDriver}} v{{.CgroupVersion}}"
    docker -H unix:///run/docker.sock load -i /mnt/engine/image.tar >/dev/null && echo "guest load ok"
    docker -H unix:///run/docker.sock run --rm --network none measure:latest sh -c "echo container-\$((6*7))" 2>&1 | sed "s/^/guest run /"
  else
    echo "guest dockerd down"
    tail -n 15 /tmp/dockerd.log | sed "s/^/guest dockerd-log /"
  fi
' 2>"$work/stderr")
code=$?
echo "engine run exit=$code"
case "$out" in *"guest dockerd up"*) echo "engine dockerd ok" ;; *) echo "engine dockerd failed" ;; esac
case "$out" in *"guest run container-42"*) echo "engine container ok" ;; *) echo "engine container failed" ;; esac
printf '%s\n' "$out" | sed 's/^/engine out /'
tail -c 400 "$work/stderr" | sed 's/^/engine stderr /'
