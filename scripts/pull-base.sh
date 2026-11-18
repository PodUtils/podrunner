#!/bin/sh
# Pull a base image, pinned by its digest, from the first registry of the
# list that serves it, and print the reference that it pulled.
# public.ecr.aws can refuse each pull of a shared runner for a time (429,
# "toomanyrequests", T-478); the digest pins the content, so each registry
# gives the same image. Three rounds, 30 and 60 s apart (PULL_BASE_PAUSE:
# the unit of the pause in seconds, 30 by default).
#
# REFERENCE is `public.ecr.aws/docker/library/NAME[:TAG]@sha256:HEX`.
#
#   img=$(sh scripts/pull-base.sh REFERENCE)
set -u
[ "$#" -eq 1 ] || { echo "usage: pull-base.sh REFERENCE" >&2; exit 2; }
ref=$1
case $ref in
    public.ecr.aws/docker/library/*@sha256:*) ;;
    *) echo "pull-base.sh: $ref: not public.ecr.aws/docker/library/NAME@sha256:HEX" >&2; exit 2 ;;
esac
rest=${ref#public.ecr.aws/docker/}
name=${rest%%@*}
name=${name%%:*}
digest=${ref#*@}
for try in 1 2 3; do
    for registry in public.ecr.aws/docker mirror.gcr.io docker.io; do
        img="$registry/$name@$digest"
        if docker pull -q "$img" >&2; then
            echo "pull-base.sh: $name came from $registry" >&2
            echo "$img"
            exit 0
        fi
        echo "pull-base.sh: the pull from $registry failed (round $try)" >&2
    done
    [ "$try" -lt 3 ] && sleep $((try * ${PULL_BASE_PAUSE:-30}))
done
echo "pull-base.sh: each registry failed 3 times for $ref" >&2
exit 1
