# talk-spike (NEW-52)

The exponentialTalk spike: one scratch crate that measures the engines behind
the planned `crates/talk` traits and writes `numbers.json`. Nothing here ships;
the findings are in `docs/talk-spike.md`. It sits outside the desktop workspace
on purpose (its own empty `[workspace]`), so `cargo build` in `apps/desktop`
never sees it.

```bash
cd apps/desktop/spikes/talk
cargo build --release                      # sherpa-onnx fetches its prebuilt lib (Apache-2.0)

# models (gitignored): streaming zipformer en, kws zh-en 3M, silero vad, kokoro v1.0
mkdir -p models && cd models && R=https://github.com/k2-fsa/sherpa-onnx/releases/download
curl -LO $R/asr-models/sherpa-onnx-streaming-zipformer-en-2023-06-26.tar.bz2
curl -LO $R/kws-models/sherpa-onnx-kws-zipformer-zh-en-3M-2025-12-20.tar.bz2
curl -LO $R/asr-models/silero_vad.onnx
curl -LO $R/tts-models/kokoro-multi-lang-v1_0.tar.bz2
for t in *.tar.bz2; do tar xjf $t && rm $t; done; cd ..

# the synthetic English set (Kokoro speaks data/phrases.json), 24 kHz → 16 kHz
cargo run --release -- kokoro models/kokoro-multi-lang-v1_0 data/phrases.json en 0 out/kokoro-en > out/kokoro-en.jsonl
mkdir -p out/kokoro-en-16k && for f in out/kokoro-en/*.wav; do ffmpeg -loglevel error -y -i $f -ar 16000 -ac 1 -sample_fmt s16 out/kokoro-en-16k/$(basename $f); done

cargo run --release -- zipformer models/sherpa-onnx-streaming-zipformer-en-2023-06-26 out/kokoro-en-16k/*.wav > out/zipformer-en-kokoro.jsonl
cargo run --release -- vad models/silero_vad.onnx out/kokoro-en-16k/*.wav > out/vad-kokoro.jsonl
cargo run --release -- kws models/sherpa-onnx-kws-zipformer-zh-en-3M-2025-12-20 data/keywords-probe.txt out/kokoro-en-16k/*.wav > out/kws-kokoro.jsonl
python3 scripts/wer.py data/phrases.json out/zipformer-en-kokoro.jsonl zipformer
python3 scripts/collect.py "<machine label>" > numbers.json
```

Whistle (cactus-rs) is behind the `cactus` feature; on Linux it links LLVM
libc++. Without root, unpack `libc++1-18 libc++abi1-18 libunwind-18` from
`apt-get download` into a directory and build with

```bash
CACTUS_CXXSTDLIB="c++,c++abi,unwind" \
RUSTFLAGS="-L <dir>/usr/lib/x86_64-linux-gnu -C link-arg=-Wl,-rpath,<dir>/usr/lib/x86_64-linux-gnu" \
cargo run --release --features cactus -- whistle out/kokoro-en-16k/*.wav
```

Nemotron 3.5 (transcribe.cpp + the Q8 GGUF) and Qwen3-TTS (llama.cpp `llama-tts`
+ the ggml-org GGUF) were driven by hand; the exact commands and their output
are in `docs/talk-spike.md`. `scripts/os-voice-capture.sh` records the OS voice
through a PipeWire sink monitor. `scripts/record.sh` is for a human set of the
same 20 phrases (not recorded yet). `out/` and `models/` are gitignored;
`numbers.json` is the committed result.
