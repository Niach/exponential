# VAPP-3 results (generated 2026-09-28T12:13:41.527Z by scripts/report.ts)

## Connectivity + performance

| source | scenario | policy | runs | ok | direct (no relay) | connect p50/p95 ms | RTT p50/p95 ms | up / down Mbps (2000000 B) | paths seen |
|---|---|---|---|---|---|---|---|---|---|
| android-device | home-wifi-cellular | all | 14 | 10 | 100% | 999.0 / 1271.0 | 75.85 / 93.14 | 3.1 / 7.4 | host↔srflx |
| android-device | same-lan | all | 18 | 18 | 100% | 601.0 / 710.0 | 13.41 / 23.73 | 11.3 / 21.3 | host↔host |
| android-device | same-lan | host | 5 | 5 | 100% | 451.0 / 537.0 | 14.92 / 21.76 | 8.3 / 22.7 | host↔host |
| android-device | same-lan | relay | 10 | 5 | 0% | 504.0 / 522.0 | 15.01 / 22.81 | 19.8 / 24.8 | relay↔host |
| android-emu | emulator-nat | all | 18 | 18 | 100% | 624.0 / 945.0 | 0.42 / 1.19 | 105.7 / 93.9 | host↔host |
| android-emu | emulator-nat | host | 6 | 6 | 100% | 547.0 / 577.0 | 0.45 / 1.69 | 104.3 / 101.6 | host↔host |
| android-emu | emulator-nat | relay | 8 | 6 | 0% | 431.0 / 530.0 | 0.52 / 1.08 | 97.6 / 94.8 | relay↔host |
| cli | same-machine-double-relay | relay | 10 | 10 | 0% | 266.9 / 284.1 | 0.28 / 0.40 | 312.3 / 313.3 | relay↔relay |
| cli | same-machine | all | 30 | 30 | 100% | 17.6 / 89.0 | 0.20 / 0.69 | 368.3 / 355.1 | host↔host |
| cli | same-machine | host | 20 | 20 | 100% | 17.7 / 90.2 | 0.23 / 0.81 | 365.3 / 334.8 | host↔host |
| cli | same-machine | relay | 20 | 20 | 0% | 16.4 / 24.0 | 0.23 / 0.32 | 500.2 / 502.2 | relay↔host |
| ios-device | same-lan | all | 15 | 10 | 100% | 316.8 / 348.9 | 11.37 / 21.61 | 25.4 / 23.7 | host↔host |
| ios-device | same-lan | host | 5 | 5 | 100% | 305.2 / 349.9 | 10.33 / 18.99 | 27.4 / 26.3 | host↔host |
| ios-device | same-lan | relay | 6 | 6 | 0% | 186.2 / 295.7 | 11.09 / 19.50 | 26.5 / 24.0 | relay↔host |
| ios-sim | same-lan | all | 7 | 6 | 67% | 29.5 / 186.5 | 0.15 / 0.36 | 449.6 / 514.2 | host↔relay, host↔host |
| ios-sim | same-lan | host | 3 | 3 | 100% | 13.8 / 14.1 | 0.13 / 0.37 | 494.6 / 569.6 | host↔host |
| ios-sim | same-lan | relay | 5 | 5 | 0% | 14.8 / 15.2 | 0.17 / 0.42 | 470.1 / 449.0 | relay↔host |
| web-chromium | same-machine-double-relay | relay | 10 | 10 | 0% | 136.3 / 145.2 | 0.40 / 0.70 | 251.6 / 284.2 | relay↔relay |
| web-chromium | same-machine | all | 20 | 20 | 100% | 12.2 / 15.8 | 0.20 / 0.30 | 245.8 / 450.7 | host↔host |
| web-chromium | same-machine | relay | 20 | 20 | 0% | 70.2 / 80.9 | 0.30 / 0.40 | 231.5 / 421.1 | relay↔host |
| web-safari | same-machine | all | 10 | 10 | 100% | 18.0 / 50.0 | 0.00 / 1.00 | 444.4 / 347.8 | srflx↔host, host↔host |
| web-safari | same-machine | relay | 10 | 10 | 0% | 66.0 / 70.0 | 0.00 / 1.00 | 484.8 / 410.3 | relay↔host |
| web-webkit | same-machine | all | 20 | 20 | 100% | 22.0 / 30.0 | 0.00 / 1.00 | 571.4 / 410.3 | host↔host, prflx↔host |
| web-webkit | same-machine | relay | 20 | 20 | 0% | 64.0 / 72.0 | 0.00 / 1.00 | 457.1 / 372.1 | relay↔host |

### Reconnect

| source | scenario | trigger | runs | reconnect p50/p95 ms |
|---|---|---|---|---|
| android-device | home-wifi-cellular | network-switch | 3 | 727.0 / 778.0 |
| android-device | same-lan | background | 4 | 378.0 / 454.0 |
| android-device | same-lan | network-switch | 3 | 121.0 / 130.0 |
| android-emu | emulator-nat | background | 1 | 296.0 / 296.0 |
| android-emu | emulator-nat | manual | 2 | 19.0 / 19.0 |
| android-emu | emulator-nat | network-switch | 2 | 566.0 / 566.0 |
| cli | same-machine | manual | 10 | 4.3 / 22.3 |
| ios-device | same-lan | background | 5 | 2988.7 / 5253.6 |
| ios-device | same-lan | manual | 1 | 42.0 / 42.0 |
| ios-sim | same-lan | manual | 14 | 26.6 / 29.5 |

### Failures

- ios-sim same-lan all: not connected within 30s (state connecting) PeerHello 1.0 core peer-core 0.0.1 str0m 0.24 spike; appConnectMs -; local -; remote -
- android-emu emulator-nat relay: turn/stun failure: turn allocate: Allocate error response (error 508: ) peer-ffi peer-core 0.0.1 str0m 0.24 spike; sdk_gphone64_arm64 API 36
- android-emu emulator-nat relay: turn/stun failure: turn allocate: Allocate error response (error 508: ) peer-ffi peer-core 0.0.1 str0m 0.24 spike; sdk_gphone64_arm64 API 36
- ios-device same-lan all: acceptAnswer: Failed(message: "invalid argument: no pending offer") PeerHello 1.0 core peer-core 0.0.1 str0m 0.24 spike; appConnectMs 537; local host 192.168.178.96:59495; remote host 192.168.178.71:65313
- ios-device same-lan all: acceptAnswer: Failed(message: "invalid argument: no pending offer") PeerHello 1.0 core peer-core 0.0.1 str0m 0.24 spike; appConnectMs 585; local host 192.168.178.96:61200; remote host 192.168.178.71:57434
- ios-device same-lan all: acceptAnswer: Failed(message: "invalid argument: no pending offer") PeerHello 1.0 core peer-core 0.0.1 str0m 0.24 spike; appConnectMs 520; local host 192.168.178.96:65232; remote host 192.168.178.71:63448
- ios-device same-lan all: runBench: Failed(message: "timeout") PeerHello 1.0 core peer-core 0.0.1 str0m 0.24 spike; appConnectMs 520; local host 192.168.178.96:65232; remote host 192.168.178.71:63448
- ios-device same-lan all: runBench: Failed(message: "timeout") PeerHello 1.0 core peer-core 0.0.1 str0m 0.24 spike; appConnectMs 520; local host 192.168.178.96:65232; remote host 192.168.178.71:63448
- android-device home-wifi-cellular all: timeout peer-ffi peer-core 0.0.1 str0m 0.24 spike; Pixel 7 API 37
- android-device home-wifi-cellular all: timeout peer-ffi peer-core 0.0.1 str0m 0.24 spike; Pixel 7 API 37
- android-device home-wifi-cellular all: timeout peer-ffi peer-core 0.0.1 str0m 0.24 spike; Pixel 7 API 37
- android-device home-wifi-cellular all: timeout peer-ffi peer-core 0.0.1 str0m 0.24 spike; Pixel 7 API 37
- android-device same-lan relay: turn/stun failure: turn allocate: Allocate error response (error 508: ) peer-ffi peer-core 0.0.1 str0m 0.24 spike; Pixel 7 API 37
- android-device same-lan relay: turn/stun failure: turn allocate: Allocate error response (error 508: ) peer-ffi peer-core 0.0.1 str0m 0.24 spike; Pixel 7 API 37
- android-device same-lan relay: turn/stun failure: turn allocate: Allocate error response (error 508: ) peer-ffi peer-core 0.0.1 str0m 0.24 spike; Pixel 7 API 37
- android-device same-lan relay: turn/stun failure: turn allocate: Allocate error response (error 508: ) peer-ffi peer-core 0.0.1 str0m 0.24 spike; Pixel 7 API 37
- android-device same-lan relay: turn/stun failure: turn allocate: Allocate error response (error 508: ) peer-ffi peer-core 0.0.1 str0m 0.24 spike; Pixel 7 API 37

## Signed fingerprints: tamper test

| source | variant | rejected | reason |
|---|---|---|---|
| cli | sdp | yes | fingerprint_signature_invalid |
| cli | sig | yes | fingerprint_signature_invalid |
| cli | control | NO | accepted: connected and benched |
| web-chromium | sdp | yes | fingerprint_signature_invalid |
| web-chromium | sig | yes | fingerprint_signature_invalid |
| web-chromium | control | NO | accepted: channel open |
| web-chromium | sdp | yes | answer rejected by browser: fingerprint_signature_invalid |

## Binary cost

| platform | artifact | variant | bytes | size | notes |
|---|---|---|---|---|---|
| android | app-production-release.apk lib/arm64-v8a | full-rust-crypto | 4390024 | 4.19 MB | baseline lib/arm64-v8a 17208 B -> +4372816 B; libpeer_ffi.so 4196296 B (1940722 B zlib-9); JNA libjnidispatch.so 176520 B (43019 B zlib-9); per-ABI download delta ~ 2080591 B incl. dex |
| android | app-production-release.apk lib/armeabi-v7a | full-rust-crypto | 2763600 | 2.64 MB | baseline lib/armeabi-v7a 11668 B -> +2751932 B; libpeer_ffi.so 2625436 B (1480369 B zlib-9); JNA libjnidispatch.so 126496 B (45195 B zlib-9); per-ABI download delta ~ 1622414 B incl. dex |
| android | app-production-release.apk lib/x86_64 | full-rust-crypto | 5209496 | 4.97 MB | baseline lib/x86_64 16984 B -> +5192512 B; libpeer_ffi.so 5065600 B (2079843 B zlib-9); JNA libjnidispatch.so 126912 B (44830 B zlib-9); per-ABI download delta ~ 2221523 B incl. dex |
| android | app-production-release.apk | baseline | 6006884 | 5.73 MB | R8 release, no peer core; dex 8998880 B (4380996 B deflated); ABIs ['arm64-v8a', 'armeabi-v7a', 'x86', 'x86_64'] |
| android | app-production-release.apk | full-rust-crypto | 18978004 | 18.10 MB | +12971120 B vs baseline (universal APK, .so stored uncompressed, all ABIs). dex +231472 B (+96850 deflated: JNA + UniFFI Kotlin). arm64-v8a: peer_ffi 4196296 jna 176520; armeabi: peer_ffi 0 jna 126980; armeabi-v7a: peer_ffi 2625436 jna 126496; mips: peer_ffi 0 jna 130556; mips64: peer_ffi 0 jna 150256; x86: peer_ffi 0 jna 124380; x86_64: peer_ffi 5065600 jna 126912. JNA also ships armeabi/mips/mips64/x86 dispatch libs (dead weight; no libpeer_ffi for x86) |
| android | libpeer_ffi.so arm64-v8a | full-rust-crypto | 4196296 | 4.00 MB | cargo-ndk stripped, opt-z fat LTO, API 26; LOAD align 0x4000 |
| android | libpeer_ffi.so arm64-v8a | minimal-rust-crypto | 3630744 | 3.46 MB | cargo-ndk stripped, opt-z fat LTO, API 26; LOAD align 0x4000 |
| android | libpeer_ffi.so armeabi-v7a | full-rust-crypto | 2625436 | 2.50 MB | cargo-ndk stripped, opt-z fat LTO, API 26; LOAD align 0x1000 |
| android | libpeer_ffi.so armeabi-v7a | minimal-rust-crypto | 2282668 | 2.18 MB | cargo-ndk stripped, opt-z fat LTO, API 26; LOAD align 0x1000 |
| android | libpeer_ffi.so x86_64 | full-rust-crypto | 5065600 | 4.83 MB | cargo-ndk stripped, opt-z fat LTO, API 26; LOAD align 0x4000 |
| android | libpeer_ffi.so x86_64 | minimal-rust-crypto | 4407104 | 4.20 MB | cargo-ndk stripped, opt-z fat LTO, API 26; LOAD align 0x4000 |
| ios | Exponential.app (Release, arm64, unsigned, archive-style strip -S -x) | baseline | 21359330 | 20.37 MB | zipped (download-size proxy) 8186233; main 9188008; ExpCore 5247144. The arm64 device slice IS what App Store thinning ships to a device, so this delta ~= the install delta and the zip delta ~= the download delta. |
| ios | Exponential.app (Release, arm64, unsigned, archive-style strip -S -x) | full-apple-crypto | 23207930 | 22.13 MB | zipped (download-size proxy) 9270861; main 9171144; ExpCore 7112608. The arm64 device slice IS what App Store thinning ships to a device, so this delta ~= the install delta and the zip delta ~= the download delta. |
| ios | Exponential.app (Release, arm64, unsigned, xcodebuild build = unstripped) | baseline | 46604682 | 44.45 MB | sum of file bytes (du 45656 KiB); zipped (download-size proxy) 11670172; main binary 25619272 [__TEXT 8028160 __DATA* 507904 __LINKEDIT 17154048]; ExpCore.framework/ExpCore 9812776 [__TEXT 3932160 __DATA* 507904 __LINKEDIT 5619712]. The arm64 device slice IS what App Store thinning ships to a device, so the .app delta ~= the install delta and the zip delta ~= the download delta. |
| ios | Exponential.app (Release, arm64, unsigned, xcodebuild build = unstripped) | full-apple-crypto | 50188770 | 47.86 MB | sum of file bytes (du 49156 KiB); zipped (download-size proxy) 13062092; main binary 25598408 [__TEXT 8011776 __DATA* 507904 __LINKEDIT 17154048]; ExpCore.framework/ExpCore 13417440 [__TEXT 5636096 __DATA* 606208 __LINKEDIT 7421952]. The arm64 device slice IS what App Store thinning ships to a device, so the .app delta ~= the install delta and the zip delta ~= the download delta. |
| ios | libpeer_ffi linked arm64 device (dead_strip, all exports kept) | full-apple-crypto | 1807776 | 1.72 MB | peer-core=core; delta of a stripped arm64 executable linking the .a vs an empty main |
| ios | libpeer_ffi linked arm64 device (dead_strip, all exports kept) | full-rust-crypto | 2612200 | 2.49 MB | peer-core=core; delta of a stripped arm64 executable linking the .a vs an empty main |
| ios | libpeer_ffi linked arm64 device (dead_strip, all exports kept) | minimal-apple-crypto | 1526552 | 1.46 MB | peer-core=core; delta of a stripped arm64 executable linking the .a vs an empty main |
| ios | libpeer_ffi.a arm64 device | full-apple-crypto | 10847376 | 10.34 MB | peer-core=core; release .a (opt-z, fat LTO via cargo rustc --crate-type staticlib, panic=abort; profile strip does not apply to staticlibs, objects carry embedded bitcode); __TEXT sum 2045413, __DATA sum 97562 over members; dead-stripped link of every exported uniffi fn into an empty iOS main adds 1807776 bytes (linked __TEXT/__DATA 1687552 114688) |
| ios | libpeer_ffi.a arm64 device | full-rust-crypto | 14190152 | 13.53 MB | peer-core=core; release .a (opt-z, fat LTO via cargo rustc --crate-type staticlib, panic=abort; profile strip does not apply to staticlibs, objects carry embedded bitcode); __TEXT sum 3362275, __DATA sum 413029 over members; dead-stripped link of every exported uniffi fn into an empty iOS main adds 2612200 bytes (linked __TEXT/__DATA 2293760 278528) |
| ios | libpeer_ffi.a arm64 device | minimal-apple-crypto | 9638832 | 9.19 MB | peer-core=core; release .a (opt-z, fat LTO via cargo rustc --crate-type staticlib, panic=abort; profile strip does not apply to staticlibs, objects carry embedded bitcode); __TEXT sum 1539244, __DATA sum 81554 over members; dead-stripped link of every exported uniffi fn into an empty iOS main adds 1526552 bytes (linked __TEXT/__DATA 1425408 98304) |

## Cold start

| platform | variant | samples | median ms | method |
|---|---|---|---|---|
| ios-sim | with-core | 5 | 1886.9 | XCTApplicationLaunchMetric |
| ios-sim | baseline | 5 | 1879.2 | XCTApplicationLaunchMetric |
| ios-sim | with-core | 5 | 1844.0 | XCTApplicationLaunchMetric |
| ios-sim | baseline | 5 | 1820.3 | XCTApplicationLaunchMetric |
| android-emu | baseline | 15 | 235.0 | am start -W TotalTime |
| android-emu | with-core | 15 | 242.0 | am start -W TotalTime |
| android-emu | baseline | 15 | 244.0 | am start -W TotalTime |
| android-emu | with-core | 15 | 217.0 | am start -W TotalTime |
| android-device | baseline | 15 | 188.0 | am start -W TotalTime |
| android-device | with-core | 15 | 192.0 | am start -W TotalTime |
| android-device | baseline | 15 | 185.0 | am start -W TotalTime |
| android-device | with-core | 15 | 258.0 | am start -W TotalTime |
| android-device | with-core | 15 | 261.0 | am start -W TotalTime |
| android-device | baseline | 15 | 181.0 | am start -W TotalTime |
| android-device | core-load-only | 5 | 8.0 | in-app SystemClock around JNA + libpeer_ffi.so load + peerVersion() in Application.onCreate (Pixel 7, release, at.exponential.vapp3) |

## TURN bandwidth per session

| source | bytes relayed | seconds | notes |
|---|---|---|---|
| coturn native log usage lines | 4572513 | 0.74 | single relay: cli --policy relay, daemon --policy all; native coturn; sum of coturn per-interval usage deltas over 4 allocation(s): from-client 2240662 B, to-client 2345763 B, from-peer 2331851 B, to-peer 2228583 B (TURN payload bytes: ChannelData/SCTP/DTLS, excl. IP/UDP); session = 2,000,000 B up + 2,000,000 B down + 100 pings; bench ok=True up 497.2 down 515.3 Mbps |
| coturn native log usage lines | 9169718 | 1.05 | double relay: cli --policy relay, daemon --policy relay (traffic crosses coturn twice); native coturn; sum of coturn per-interval usage deltas over 4 allocation(s): from-client 4604551 B, to-client 4591359 B, from-peer 4565167 B, to-peer 4585449 B (TURN payload bytes: ChannelData/SCTP/DTLS, excl. IP/UDP); session = 2,000,000 B up + 2,000,000 B down + 100 pings; bench ok=True up 355.6 down 370.2 Mbps |
