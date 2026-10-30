#!/usr/bin/env bash
# #EXP-1441: the Maven Central Portal upload bundle for at.exponential:etalk
# (eUI's packages/exponential-ui-compose/release/central-bundle.sh).
#
#   bash release/central-bundle.sh [-PtalkVersion=1.2.3] [more gradle args]
#
# Publishes the AAR + POM + sources + javadoc jars (signed when
# ORG_GRADLE_PROJECT_signingInMemoryKey / …Password are set) into the local
# `centralBundle` repository (android/build/central-bundle), adds the .md5 /
# .sha1 of every artifact and zips the Maven layout into
# android/build/central-bundle.zip (the upload itself is the workflow's curl).
# Needs the .so files first: bash build-android.sh. The Gradle wrapper is eUI
# Compose's; android/local.properties (sdk.dir) as for any Android build.
set -euo pipefail

cd "$(dirname "$0")/../android"
root="$PWD"
gradlew="$(cd ../../../../../packages/exponential-ui-compose && pwd)/gradlew"
bundle="$root/build/central-bundle"
zip="$root/build/central-bundle.zip"
for abi in arm64-v8a armeabi-v7a x86_64; do
  [ -f "../out/jniLibs/$abi/libetalk_ffi.so" ] || { echo "missing out/jniLibs/$abi/libetalk_ffi.so (run build-android.sh)" >&2; exit 1; }
done

rm -rf "$bundle" "$zip"
"$gradlew" -p "$root" --no-configuration-cache publishAllPublicationsToCentralBundleRepository "$@"

cd "$bundle"
# The Portal takes the component files only: no repository-level metadata.
find . -name 'maven-metadata.xml*' -delete
find . -type f ! -name '*.md5' ! -name '*.sha1' ! -name '*.sha256' ! -name '*.sha512' ! -name '*.asc' | while read -r f; do
  [ -f "$f.md5" ] || md5 -q "$f" 2>/dev/null >"$f.md5" || md5sum "$f" | cut -d' ' -f1 >"$f.md5"
  [ -f "$f.sha1" ] || shasum -a 1 "$f" | cut -d' ' -f1 >"$f.sha1"
done
# Every ABI made it into the AAR.
aar="$(find . -name '*.aar' | head -1)"
listing="$(unzip -l "$aar")"
for abi in arm64-v8a armeabi-v7a x86_64; do
  grep -q "jni/$abi/libetalk_ffi.so" <<<"$listing" || { echo "$aar lacks jni/$abi/libetalk_ffi.so" >&2; exit 1; }
done

if ! find . -name '*.aar.asc' | grep -q .; then
  echo "note: unsigned bundle (set ORG_GRADLE_PROJECT_signingInMemoryKey to sign); the Portal rejects it" >&2
fi

zip -qr "$zip" at
echo "$zip"
unzip -l "$zip"
