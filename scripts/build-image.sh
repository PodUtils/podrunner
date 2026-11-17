#!/bin/sh
# Build DOCKERFILE with docker buildx. Its base image comes
# from the first registry of the list that serves it: public.ecr.aws can
# refuse each pull of a shared runner for a time (429, "Data limit
# exceeded", T-478). Its digest pins it, so each registry gives the same
# content. Three rounds, 30 and 60 s apart (BUILD_IMAGE_PAUSE: the unit of
# the pause in seconds, 30 by default).
#
# DOCKERFILE starts `ARG BASE_REGISTRY` and `FROM ${BASE_REGISTRY}/library/...`.
#
#   sh scripts/build-image.sh DOCKERFILE [BUILDX ARGUMENT...]
set -u
[ "$#" -gt 0 ] || { echo "usage: build-image.sh DOCKERFILE [BUILDX ARGUMENT...]" >&2; exit 2; }
file=$1
shift
built=no
for try in 1 2 3; do
    for registry in public.ecr.aws/docker mirror.gcr.io docker.io; do
        if docker buildx build -f "$file" --build-arg "BASE_REGISTRY=$registry" "$@"; then
            built=$registry
            break 2
        fi
        echo "build-image.sh: the build from $registry failed (round $try)" >&2
    done
    [ "$try" -lt 3 ] && sleep $((try * ${BUILD_IMAGE_PAUSE:-30}))
done
[ "$built" != no ] || { echo "build-image.sh: each registry failed 3 times" >&2; exit 1; }
echo "build-image.sh: the base image came from $built" >&2
