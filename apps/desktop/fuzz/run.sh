#!/bin/sh
# NEW-88: run every fuzz target for <seconds> each (seeding the Opus corpora
# from a speech clip first). The release suite runs it short; the long runs
# before caller mode leaves beta go through the heavy lane:
#
#   scripts/lanes/lane.sh heavy <owner> -- nice -n 19 apps/desktop/fuzz/run.sh 1800 8
#
#   run.sh <seconds> [jobs] [cargo-fuzz flags, e.g. -F libopus]
#
# jobs > 1 forks that many libFuzzer workers per target and keeps going past
# a crash (every crash lands in artifacts/<target>/); jobs = 1 stops at the
# first. Exits non-zero when any target crashed.
set -eu
cd "$(dirname "$0")"
seconds=${1:?usage: run.sh <seconds> [jobs] [cargo-fuzz flags]}
jobs=${2:-1}
[ $# -ge 2 ] && shift 2 || shift 1
targets=${FUZZ_TARGETS:-opus_decode call_uplink aec}

if [ ! -d corpus/opus_decode ]; then
  cargo run --release --example seeds -- ../crates/voice/assets/builtin-en.wav corpus
fi
cargo +nightly fuzz build -a "$@"

failed=""
for target in $targets; do
  mkdir -p "corpus/$target" "artifacts/$target"
  if [ "$jobs" -gt 1 ]; then
    mode="-fork=$jobs -ignore_crashes=1 -ignore_timeouts=1 -ignore_ooms=1"
  else
    mode=""
  fi
  echo "== $target: ${seconds}s, $jobs job(s)"
  # shellcheck disable=SC2086
  cargo +nightly fuzz run -a "$@" "$target" "corpus/$target" -- \
    -max_total_time="$seconds" -timeout=10 -rss_limit_mb=4096 -max_len=8192 -print_final_stats=1 $mode \
    || failed="$failed $target"
  if ls "artifacts/$target"/crash-* >/dev/null 2>&1; then
    failed="$failed $target"
  fi
done
if [ -n "$failed" ]; then
  echo "fuzz: crashes in:$failed (see fuzz/artifacts/)" >&2
  exit 1
fi
echo "fuzz: no crash"
