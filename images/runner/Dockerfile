# The unit image of the official runner (GitHub #30). A host with root or a
# namespace builds it (DR-14, call C8); a host with no root imports it. The
# build context is the root of the repository:
#
#   docker buildx build -f images/runner/Dockerfile \
#     --build-arg RUNNER_VERSION=2.338.2 --build-arg RUNNER_SHA256=HEX .
#
# The larger image adds --build-arg "EXTRA_PACKAGES=$(cat images/runner/full.txt)".
#
# images/runner/runner.toml holds the version and the digest of each
# tarball; .github/workflows/runner-image.yml reads them.
#
# BASE_REGISTRY gives the registry of the base image: the digest pins its
# content, so each mirror of Docker Hub gives the same image. CI tries
# mirror.gcr.io when public.ecr.aws refuses a pull (429, T-478).
ARG BASE_REGISTRY=public.ecr.aws/docker
FROM ${BASE_REGISTRY}/library/ubuntu:24.04@sha256:fe81560990fae86dc4743202b2cb4d3df98d0bcf949173cecef15ce1127ef852

ARG RUNNER_VERSION
ARG RUNNER_SHA256
ARG TARGETARCH

# The runtime of the .NET runner (ICU, OpenSSL) and the tools of a step;
# iptables (nft) for a container engine in a guest (T-103). A podvm job
# cannot install a package (no sudo there): make, python3 and libatomic1
# (the Node of setup-node from 26 on) come in the base image (T-324); less
# for the pager of a test (T-353).
# EXTRA_PACKAGES: the larger image (T-129) adds the packages of
# images/runner/full.txt, the common tools of `ubuntu-latest`.
ARG EXTRA_PACKAGES=""
RUN apt-get update \
 && apt-get install -y --no-install-recommends bash ca-certificates curl git iptables jq less libatomic1 libicu74 libssl3t64 make python3 sudo tar gzip unzip zstd $EXTRA_PACKAGES \
 && rm -rf /var/lib/apt/lists/*

# `sudo` with no password, as on ubuntu-latest (T-315). apt runs as root:
# its sandbox user `_apt` has no uid in a user namespace of one uid.
RUN useradd --create-home --uid 1001 runner \
 && echo 'runner ALL=(ALL) NOPASSWD:ALL' > /etc/sudoers.d/runner && chmod 0440 /etc/sudoers.d/runner \
 && echo 'APT::Sandbox::User "root";' > /etc/apt/apt.conf.d/10podrunner-sandbox
WORKDIR /home/runner

# The tarball of the release, checked against the digest of runner.toml.
RUN arch=x64; [ "$TARGETARCH" = arm64 ] && arch=arm64; \
    curl -fsSLo runner.tgz "https://github.com/actions/runner/releases/download/v${RUNNER_VERSION}/actions-runner-linux-${arch}-${RUNNER_VERSION}.tar.gz" \
 && echo "${RUNNER_SHA256}  runner.tgz" | sha256sum -c - \
 && tar -xzf runner.tgz && rm runner.tgz \
 && chown -R runner:runner /home/runner

# The tarball has no licence file; the image carries the MIT licence of
# the official runner (THIRD_PARTY.md).
COPY images/runner/LICENSE.actions-runner /home/runner/LICENSE.actions-runner
# The relay of the container hooks (GitHub #35).
COPY --chmod=0755 crates/podrunner-hook/relay/podrunner-hook.sh /home/runner/hooks/podrunner-hook.sh
# The podvm driver writes the credential files of the runner into
# /podrunner/.runner (mode 0600); the runner reads them from its own
# directory. Relative links: on the supervise rung of podvm an absolute
# link resolves on the host, out of the rootfs.
RUN for f in .runner .credentials .credentials_rsaparams .podrunner-job; do ln -s "../../podrunner/.runner/$f" "/home/runner/$f"; done
# T-175: GitHub binds no runner of a scale set to one job. The hook ends a
# job other than the one that podrunner placed in the unit
# (.podrunner-job) before its first step.
COPY --chmod=0755 images/runner/podrunner-job-started.sh /home/runner/hooks/podrunner-job-started.sh
ENV ACTIONS_RUNNER_HOOK_JOB_STARTED=/home/runner/hooks/podrunner-job-started.sh

USER runner
