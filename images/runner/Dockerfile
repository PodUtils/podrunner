# The unit image of the official runner (GitHub #30). A host with root or a
# namespace builds it (DR-14, call C8); a host with no root imports it. The
# build context is the root of the repository:
#
#   docker buildx build -f images/runner/Dockerfile \
#     --build-arg RUNNER_VERSION=2.338.2 --build-arg RUNNER_SHA256=HEX .
#
# images/runner/runner.toml holds the version and the digest of each
# tarball; .github/workflows/runner-image.yml reads them.
FROM public.ecr.aws/docker/library/ubuntu:24.04@sha256:fe81560990fae86dc4743202b2cb4d3df98d0bcf949173cecef15ce1127ef852

ARG RUNNER_VERSION
ARG RUNNER_SHA256
ARG TARGETARCH

# The runtime of the .NET runner (ICU, OpenSSL) and the tools of a step;
# iptables (nft) for a container engine in a guest (T-103).
RUN apt-get update \
 && apt-get install -y --no-install-recommends bash ca-certificates curl git iptables jq libicu74 libssl3t64 tar gzip unzip zstd \
 && rm -rf /var/lib/apt/lists/*

RUN useradd --create-home --uid 1001 runner
WORKDIR /home/runner

# The tarball of the release, checked against the digest of runner.toml.
RUN arch=x64; [ "$TARGETARCH" = arm64 ] && arch=arm64; \
    curl -fsSLo runner.tgz "https://github.com/actions/runner/releases/download/v${RUNNER_VERSION}/actions-runner-linux-${arch}-${RUNNER_VERSION}.tar.gz" \
 && echo "${RUNNER_SHA256}  runner.tgz" | sha256sum -c - \
 && tar -xzf runner.tgz && rm runner.tgz \
 && chown -R runner:runner /home/runner

# The relay of the container hooks (GitHub #35).
COPY --chmod=0755 crates/podrunner-hook/relay/podrunner-hook.sh /home/runner/hooks/podrunner-hook.sh
# The podvm driver writes the credential files of the runner into
# /podrunner/.runner (mode 0600); the runner reads them from its own
# directory. Relative links: on the supervise rung of podvm an absolute
# link resolves on the host, out of the rootfs.
RUN for f in .runner .credentials .credentials_rsaparams; do ln -s "../../podrunner/.runner/$f" "/home/runner/$f"; done

USER runner
