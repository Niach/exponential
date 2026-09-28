#!/usr/bin/env bash
# APK size delta of the REAL app: baseline vs -PpeerSpike=true (both assembleProductionRelease).
#   size-android.sh <baseline.apk> <peerspike.apk> [variant=full-rust-crypto]
# Appends `size` lines to results/c-size.jsonl: whole APK ×2, then per-ABI lib/ totals with the
# libpeer_ffi.so / libjnidispatch.so (JNA) split. Play serves per-ABI splits from the bundle, so the
# per-ABI lib delta (+ the dex delta) is what a user downloads (Play compresses the transfer).
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
BASE="$1"; SPIKE="$2"; VARIANT="${3:-full-rust-crypto}"
python3 - "$BASE" "$SPIKE" "$VARIANT" "$HERE/../results/c-size.jsonl" <<'PY'
import json, sys, zipfile, zlib, datetime, os
base, spike, variant, out = sys.argv[1:]
now = datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
def libs(apk):
    z = zipfile.ZipFile(apk); d = {}
    for i in z.infolist():
        if i.filename.startswith("lib/"):
            _, abi, name = i.filename.split("/", 2)
            gz = len(zlib.compress(z.read(i.filename), 9))
            d.setdefault(abi, {})[name] = (i.file_size, gz)
    dex = sum(i.file_size for i in z.infolist() if i.filename.endswith(".dex"))
    dexz = sum(i.compress_size for i in z.infolist() if i.filename.endswith(".dex"))
    return d, dex, dexz
bl, bdex, bdexz = libs(base); sl, sdex, sdexz = libs(spike)
lines = []
bsz, ssz = os.path.getsize(base), os.path.getsize(spike)
lines.append({"kind":"size","at":now,"platform":"android","artifact":"app-production-release.apk","variant":"baseline","bytes":bsz,
  "notes":f"R8 release, no peer core; dex {bdex} B ({bdexz} B deflated); ABIs {sorted(bl)}"})
per = "; ".join(f"{abi}: peer_ffi {sl[abi].get('libpeer_ffi.so',(0,0))[0]} jna {sl[abi].get('libjnidispatch.so',(0,0))[0]}" for abi in sorted(sl))
lines.append({"kind":"size","at":now,"platform":"android","artifact":"app-production-release.apk","variant":variant,"bytes":ssz,
  "notes":f"+{ssz-bsz} B vs baseline (universal APK, .so stored uncompressed, all ABIs). dex +{sdex-bdex} B (+{sdexz-bdexz} deflated: JNA + UniFFI Kotlin). {per}. JNA also ships armeabi/mips/mips64/x86 dispatch libs (dead weight; no libpeer_ffi for x86)"})
for abi in [a for a in sorted(sl) if 'libpeer_ffi.so' in sl[a]]:
    b = sum(v[0] for v in bl.get(abi, {}).values()); s = sum(v[0] for v in sl[abi].values())
    pf = sl[abi].get("libpeer_ffi.so", (0, 0)); jna = sl[abi].get("libjnidispatch.so", (0, 0))
    lines.append({"kind":"size","at":now,"platform":"android","artifact":f"app-production-release.apk lib/{abi}","variant":variant,"bytes":s,
      "notes":f"baseline lib/{abi} {b} B -> +{s-b} B; libpeer_ffi.so {pf[0]} B ({pf[1]} B zlib-9); JNA libjnidispatch.so {jna[0]} B ({jna[1]} B zlib-9); per-ABI download delta ~ {pf[1]+jna[1]+(sdexz-bdexz)} B incl. dex"})
with open(out, "a") as f:
    for l in lines:
        f.write(json.dumps(l) + "\n"); print(json.dumps(l))
PY
