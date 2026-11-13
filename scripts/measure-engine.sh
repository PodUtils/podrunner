#!/bin/sh
# Measure a container engine inside a guest of the machine tier of podvm
# (T-103, step 3), with `--podvm-engine` (podvm v0.2.0-beta.14: the bridge,
# veth, overlay and netfilter modules): the static Docker binaries start
# in the guest, a container runs, and a user-defined network of
# `services:` lets one container reach another by its name. It prints one line
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

out=$(timeout 600 "$podvm" run --rm --podvm-tier=machine --podvm-engine --podvm-net none --podvm-mem "${MACHINE_MEM:-4G}" --podvm-cpus 2 \
  -v "$work/vol:/mnt/engine" "$image" /bin/sh -c '
  echo "guest kernel $(uname -r)"
  echo "guest cgroup $(stat -fc %T /sys/fs/cgroup 2>&1)"
  for fs in overlay tmpfs; do grep -qw "$fs" /proc/filesystems && echo "guest fs $fs yes" || echo "guest fs $fs no"; done
  echo "guest controllers $(cat /sys/fs/cgroup/cgroup.controllers 2>&1 | tr "\n" " ")"
  echo "guest iptables $(iptables --version 2>&1)"
  export PATH=/mnt/engine/docker:$PATH
  mkdir -p /var/lib/docker /run
  # The root of the guest is an initramfs: pivot_root fails there, and
  # DOCKER_RAMDISK makes runc move the root and chroot instead.
  DOCKER_RAMDISK=true dockerd --host=unix:///run/docker.sock > /tmp/dockerd.log 2>&1 &
  i=0; while [ $i -lt 60 ] && ! docker -H unix:///run/docker.sock info >/dev/null 2>&1; do sleep 1; i=$((i+1)); done
  if docker -H unix:///run/docker.sock info >/dev/null 2>&1; then
    echo "guest dockerd up after ${i}s"
    docker -H unix:///run/docker.sock info --format "guest driver {{.Driver}} cgroup {{.CgroupDriver}} v{{.CgroupVersion}}"
    docker -H unix:///run/docker.sock load -i /mnt/engine/image.tar >/dev/null && echo "guest load ok"
    docker -H unix:///run/docker.sock run --rm --network none measure:latest sh -c "echo container-\$((6*7))" 2>&1 | sed "s/^/guest run /"
    # services: needs a user-defined bridge network (bridge and veth in
    # the kernel) where one container reaches another by its name.
    D="docker -H unix:///run/docker.sock"
    if $D network create measure-net 2>&1 | sed "s/^/guest network /"; then
      $D run -d --name measure-svc --network measure-net measure:latest sh -c "echo svc-\$((6*7)) > /tmp/i; httpd -f -p 8080 -h /tmp" 2>&1 | sed "s/^/guest svc /"
      sleep 2
      $D run --rm --network measure-net measure:latest wget -qO- http://measure-svc:8080/i 2>&1 | sed "s/^/guest reach /"
      $D rm -f measure-svc >/dev/null 2>&1
    fi
  else
    echo "guest dockerd down"
    tail -n 15 /tmp/dockerd.log | sed "s/^/guest dockerd-log /"
  fi
' 2>"$work/stderr")
code=$?
echo "engine run exit=$code"
case "$out" in *"guest dockerd up"*) echo "engine dockerd ok" ;; *) echo "engine dockerd failed" ;; esac
case "$out" in *"guest run container-42"*) echo "engine container ok" ;; *) echo "engine container failed" ;; esac
case "$out" in *"guest reach svc-42"*) echo "engine service-network ok" ;; *) echo "engine service-network failed" ;; esac
printf '%s\n' "$out" | sed 's/^/engine out /'
tail -c 400 "$work/stderr" | sed 's/^/engine stderr /'
