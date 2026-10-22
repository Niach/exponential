# EXP-1364: the Mint's self-hosted runner (label exp-linux). Ubuntu 24.04 =
# the glibc floor of the desktop AppImage, same as the ubuntu-24.04 image.
FROM myoung34/github-runner:ubuntu-noble

# The gpui/wgpu + AppImage system deps the Linux jobs apt-get every run,
# preinstalled so that step is a no-op here.
RUN apt-get update && apt-get install -y --no-install-recommends \
      build-essential clang pkg-config jq zip unzip file wget curl \
      libxkbcommon-dev libxkbcommon-x11-dev libwayland-dev libxcb1-dev \
      libssl-dev libfontconfig1-dev libx11-dev libxi-dev libxtst-dev \
      libpipewire-0.3-dev librsvg2-bin \
    && rm -rf /var/lib/apt/lists/*

# Jobs skip Swatinem/rust-cache on self-hosted: sccache on a persistent
# volume replaces it.
ARG SCCACHE_VERSION=v0.18.0
RUN curl -fsSL "https://github.com/mozilla/sccache/releases/download/${SCCACHE_VERSION}/sccache-${SCCACHE_VERSION}-x86_64-unknown-linux-musl.tar.gz" \
      | tar -xz -C /tmp \
    && install -m 0755 "/tmp/sccache-${SCCACHE_VERSION}-x86_64-unknown-linux-musl/sccache" /usr/local/bin/sccache \
    && rm -rf /tmp/sccache-*

# Jobs run as the image's `runner` user, never root: root walks through file
# permissions, and the desktop suite asserts a read-only dir refuses an unlink.
# The cache volumes mount over these dirs and inherit the ownership.
RUN install -d -o runner -g runner /home/runner/.cargo /home/runner/.rustup \
      /home/runner/.gradle /home/runner/.bun /home/runner/.cache/sccache

ENV RUSTC_WRAPPER=sccache \
    SCCACHE_DIR=/home/runner/.cache/sccache \
    SCCACHE_CACHE_SIZE=40G \
    CARGO_INCREMENTAL=0
