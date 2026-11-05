---
title: Numbers
description: Measured latency, accuracy, VRAM and download sizes for Voice's speech in and speech out, on a GPU and on the CPU, with the targets not yet measured.
---

Sources: [`docs/talk-spike.md`](https://github.com/Niach/exponential/blob/master/docs/talk-spike.md) (“Speech in through CrispASR”), `etalk-engine` (`MIN_GPU_BYTES`, `MAX_WARMUP_RTF`), [Licences](/docs/sdk/voice/reference/licences/) (download sizes).

## Speech in

Silero VAD and Parakeet TDT 0.6B v3 (Q8_0), on an RTX 4090 and an i9. Ten synthetic English clips, run through the worker's own pipeline. Word error rate counts against the spoken form, so “New 54” for “NEW fifty four” is an error.

| | GPU (CUDA) | CPU, 8 threads |
|---|---|---|
| Text after you let go | 10–41 ms | 102–450 ms |
| Word error rate | 14.9 % | 12.6 % |
| Words while you speak | every 300 ms | no |
| Model load | 0.72 s | 0.48 s |
| VRAM | 1,246 MiB | none |

Words while you speak need 1.6 GB of VRAM free. Our German and English built-in clips came through word for word on both. No human recordings yet.

## Speech out

Qwen3-TTS 0.6B Base (F16) and its codec, on the GPU only.

| | |
|---|---|
| VRAM, peak | 3.7 GB |
| GPU needed | 4 GB or more |
| Real-time factor, RTX 4090 | 0.13–0.27 |
| Real-time factor, CPU | ~2.0, so no speech on the CPU |

A real-time factor below 1 is faster than real time. A GPU must reach 0.6 in a warm-up sentence. Time to first audio is not measured on this model yet; the earlier CustomVoice Q8_0 model took 0.3–0.5 s for a short sentence.

## Downloads

| | Linux | macOS | Windows |
|---|---|---|---|
| Speech out, first use | ~2.97 GB | ~2.19 GB | ~2.28 GB |
| Speech in models | 675 MB | 675 MB | 675 MB |
| CPU-only runtime (no GPU driver) | 22.7 MB | — | 58.4 MB |

Linux: runtime 196 MB, cuBLAS 581 MB, model 1.83 GB, codec 358 MB. macOS: the runtime ships in the app, so only the models download.

## Targets

What we aim for; these are targets, the Today column is measured.

| | Target | Today |
|---|---|---|
| First words while you speak | < 300 ms | not measured |
| Final text | < 500 ms | 10–41 ms GPU, 102–450 ms CPU |
| First spoken audio | < 400 ms at p95 | not measured on this model |
| Local models, VRAM | ≤ 4 GB | 3.7 GB speech out + 1,246 MiB speech in: over, together |
