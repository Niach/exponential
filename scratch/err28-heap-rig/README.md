# ERR-28 / ERR-27 Android heap rig (session dcd08adb, 2026-11-16)

Scratch, not product code. Paths assume RIG=/tmp/exp-dcd08adb (adjust).
- Stack: isolated compose project (postgres:18 with wal_level=logical, electric 1.8.1, garage), migrate,
  apply apps/web/src/db/out/custom/0001_triggers.sql by hand, migrate:contract, seed-screenshots.ts
  (demo@exponential.at / screenshots-demo), then the large seed (2175 sessions, 3434 issues,
  ~54k messages with one 10k-message tree, 1847 PRs, ~40k checks, 79 ci_runs, 713 notifications,
  10.8k events). The large-seed script was lost in the MacBook reboot of 2026-11-17 (/tmp wiped);
  rebuild it from that spec. It must write $RIG/seeded-ids.json {bigRootId, longRootIds[10], openChats[]}.
- Web: Node 24 (`PATH=/opt/homebrew/opt/node@24/bin:$PATH npx vite dev --port 5291`), env with
  BETTER_AUTH_TRUSTED_ORIGINS incl. http://localhost:5291; `adb reverse tcp:5291 tcp:5291`.
- AVD: android-36 google_apis arm64, hw.ramSize=4096, then `adb root; setprop dalvik.vm.heapgrowthlimit 256m;
  setprop dalvik.vm.heapsize 512m; stop; start` (zygote restart), check getprop. Both APKs with largeHeap=false.
- Scripts: a (adb -s emulator-5600 wrapper), tap.py (uiautomator tap-by-text; setup only, never during C2),
  c2.sh <out> (the 10-min C2), taps.sh <N> <out> (N push taps + Back + dump), pic.sh <out.png> <chatA> <chatB>,
  hcount.py (instances per class in an hprof-conv'd dump), peaks.py <out> (allocation peaks per open).
