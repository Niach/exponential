#!/usr/bin/env bash
# The android runner kit (scripts/runner-kits.sh): $1 = the kit directory to fill.
#
# Builds the template (apps/runner/android, :app:assembleRelease, R8) ONCE and
# keeps what `exponential bundle --target android` relinks per app without
# compiling anything (eengine-bundle README, "Android"):
#   kit.json             {templateId, compileSdk}
#   AndroidManifest.xml  the MERGED manifest (text), still on the template id, with the E7:PERMISSIONS markers
#   resources.zip        the merged compiled resources (aapt2 .flat, app + libraries)
#   resource-ids.txt     the ids the release link gave them (aapt2 --emit-ids): the dex points at these
#   payload.zip          the rest of the release APK: classes*.dex, lib/<abi>/*.so, the Java resources, assets
#                        (minus the manifest, resources.arsc, res/, the signature and assets/app/)
#   NOTICES.txt          apps/runner/android/NOTICES.txt
#
# Needs a JDK, ANDROID_HOME (platforms;android-<compileSdk>, build-tools) and
# the NDK for the two native libraries (built here when missing; ABIS="arm64-v8a x86_64").
# Extra Gradle flags: RUNNER_KIT_GRADLE_ARGS (e.g. "--max-workers=4").
set -euo pipefail
kit="$1"
root="$(cd "$(dirname "$0")/../.." && pwd)"
runner="$root/apps/runner/android"
notices="$runner/NOTICES.txt"
[ -f "$notices" ] || { echo "android: $notices is missing (the per-runner notices file)" >&2; exit 1; }

for crate in eengine-ffi exponential-ui-ffi; do
  if ! ls "$root/apps/desktop/crates/$crate/out/jniLibs"/*/*.so >/dev/null 2>&1; then
    echo "== android: $crate's .so"
    SKIP_BINDINGS=1 bash "$root/apps/desktop/crates/$crate/build-android.sh"
  fi
done

echo "== android: the template (assembleRelease)"
rm -f "$runner/app/build/runner-kit/resource-ids.txt"
# shellcheck disable=SC2086
"$root/packages/exponential-ui-compose/gradlew" -p "$runner" :app:assembleRelease --no-daemon ${RUNNER_KIT_GRADLE_ARGS:-}

build="$runner/app/build"
apk="$(ls "$build"/outputs/apk/release/*.apk | head -1)"
manifest="$(find "$build/intermediates" -path '*merged_manifest*' -path '*release*' -name AndroidManifest.xml | head -1)"
flats="$build/intermediates/merged_res/release/mergeReleaseResources"
ids="$build/runner-kit/resource-ids.txt"
for f in "$apk" "$manifest" "$ids"; do [ -f "$f" ] || { echo "android: missing build output $f" >&2; exit 1; }; done
[ -d "$flats" ] || { echo "android: missing $flats" >&2; exit 1; }
compile_sdk="$(sed -n 's/^ *compileSdk *= *\([0-9]*\).*/\1/p' "$runner/app/build.gradle.kts" | head -1)"

python3 - "$kit" "$apk" "$manifest" "$flats" "$compile_sdk" <<'PY'
import json, os, re, sys, zipfile
kit, apk, manifest, flats, compile_sdk = sys.argv[1:]
text = open(manifest, encoding="utf-8").read()
if "<!-- E7:PERMISSIONS" not in text or "<!-- /E7:PERMISSIONS -->" not in text:
    sys.exit("android: the merged manifest lost the E7:PERMISSIONS markers")
template_id = re.search(r'<manifest[^>]*\spackage="([^"]+)"', text).group(1)
open(os.path.join(kit, "AndroidManifest.xml"), "w", encoding="utf-8").write(text)
json.dump({"templateId": template_id, "compileSdk": int(compile_sdk)}, open(os.path.join(kit, "kit.json"), "w"), indent=2)
with zipfile.ZipFile(os.path.join(kit, "resources.zip"), "w", zipfile.ZIP_STORED) as z:
    for name in sorted(os.listdir(flats)):
        if name.endswith(".flat"):
            z.write(os.path.join(flats, name), name)
def kept(name):
    if name in ("AndroidManifest.xml", "resources.arsc") or name.startswith(("res/", "assets/app/")):
        return False
    return not (name.startswith("META-INF/") and (name == "META-INF/MANIFEST.MF" or name.endswith((".SF", ".RSA", ".DSA", ".EC"))))
with zipfile.ZipFile(apk) as src, zipfile.ZipFile(os.path.join(kit, "payload.zip"), "w") as dst:
    for info in src.infolist():
        if not info.is_dir() and kept(info.filename):
            dst.writestr(zipfile.ZipInfo(info.filename, date_time=(1981, 1, 1, 1, 1, 2)), src.read(info), compress_type=info.compress_type)
PY
cp "$ids" "$kit/resource-ids.txt"
cp "$notices" "$kit/NOTICES.txt"
