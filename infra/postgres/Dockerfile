# #NEW-134: the stock Postgres image + pgvector, nothing else.
#
#   ghcr.io/niach/exponential-postgres:<PG_MAJOR>-pgvector<PGVECTOR_VERSION>
#
# Why a derived image and not pgvector/pgvector:pg18: that one is Debian
# (glibc). Every Exponential database so far runs postgres:<major>-alpine
# (musl); putting another libc under an existing data directory silently
# changes text collation order and with it every text index. This image is
# `FROM postgres:<major>-alpine`: the same libc, locale behaviour, data
# directory layout (PG 18: /var/lib/postgresql/18/docker), entrypoint, user
# (70) and flags as the stock image, so it is a drop-in for an existing
# volume in BOTH directions (back to stock: `DROP EXTENSION vector CASCADE`
# first, see INSTALL.md). The only additions are the extension's files:
#
#   /usr/local/lib/postgresql/vector.so
#   /usr/local/share/postgresql/extension/vector*
#
# PG_MAJOR: 18 (the repo's compose files, CI) or 17 (databases that have not
# moved to 18 yet). pgvector is built from its release tarball, pinned by
# version AND sha256, portable (no -march=native: the image runs on any
# x86-64 / arm64 host) and without LLVM bitcode (JIT inlining of an index
# access method buys nothing and would pull clang into the build).
ARG PG_MAJOR=18

FROM postgres:${PG_MAJOR}-alpine AS build
ARG PGVECTOR_VERSION=0.8.7
ARG PGVECTOR_SHA256=cac0b10c360f05b2d521200105ba3697e773d4cd3731f5a915a7e37ebe0bea85
RUN apk add --no-cache build-base
RUN set -eux; \
    wget -O /tmp/pgvector.tar.gz "https://github.com/pgvector/pgvector/archive/refs/tags/v${PGVECTOR_VERSION}.tar.gz"; \
    echo "${PGVECTOR_SHA256}  /tmp/pgvector.tar.gz" | sha256sum -c -; \
    mkdir /tmp/pgvector; \
    tar -xzf /tmp/pgvector.tar.gz -C /tmp/pgvector --strip-components=1; \
    cd /tmp/pgvector; \
    make OPTFLAGS="" with_llvm=no; \
    make install with_llvm=no DESTDIR=/out; \
    install -D -m 0644 LICENSE /out/usr/local/share/doc/pgvector/LICENSE; \
    # The runtime needs the library and the extension's SQL/control files,
    # not its C headers.
    rm -rf /out/usr/local/include

FROM postgres:${PG_MAJOR}-alpine
ARG PGVECTOR_VERSION=0.8.7
COPY --from=build /out/ /
LABEL org.opencontainers.image.source="https://github.com/Niach/exponential" \
      org.opencontainers.image.description="postgres:<major>-alpine + pgvector ${PGVECTOR_VERSION} (Exponential's database image)" \
      at.exponential.pgvector.version="${PGVECTOR_VERSION}"
