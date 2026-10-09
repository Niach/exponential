#!/usr/bin/env bash
# VAPP-91: the Maven Central Portal upload bundle for at.exponential:ui-compose
# and at.exponential:ui-compose-primitives.
#
#   bash release/central-bundle.sh [-PuiVersion=1.2.3] [more gradle args]
#
# Publishes both modules (AAR + POM + sources + javadoc jars, signed when
# ORG_GRADLE_PROJECT_signingInMemoryKey / …Password are set) into the local
# `centralBundle` repository (build/central-bundle), adds the .md5 / .sha1
# checksum of every artifact that lacks one, and zips the Maven layout into
# build/central-bundle.zip, the Portal's upload format (the upload itself is
# the CI workflow's curl). Needs the facade's .so files first:
# bash apps/desktop/crates/exponential-ui-ffi/build-android.sh
set -euo pipefail

cd "$(dirname "$0")/.."
root="$PWD"
bundle="$root/build/central-bundle"
zip="$root/build/central-bundle.zip"

rm -rf "$bundle" "$zip"
./gradlew --no-configuration-cache \
  :ui-compose:publishAllPublicationsToCentralBundleRepository \
  :ui-compose-primitives:publishAllPublicationsToCentralBundleRepository "$@"

cd "$bundle"
# The Portal takes the component files only: no repository-level metadata.
find . -name 'maven-metadata.xml*' -delete
find . -type f ! -name '*.md5' ! -name '*.sha1' ! -name '*.sha256' ! -name '*.sha512' ! -name '*.asc' | while read -r f; do
  [ -f "$f.md5" ] || md5 -q "$f" 2>/dev/null >"$f.md5" || md5sum "$f" | cut -d' ' -f1 >"$f.md5"
  [ -f "$f.sha1" ] || shasum -a 1 "$f" | cut -d' ' -f1 >"$f.sha1"
done

if ! find . -name '*.aar.asc' | grep -q .; then
  echo "note: unsigned bundle (set ORG_GRADLE_PROJECT_signingInMemoryKey to sign); the Portal rejects it" >&2
fi

zip -qr "$zip" at
echo "$zip"
unzip -l "$zip"
