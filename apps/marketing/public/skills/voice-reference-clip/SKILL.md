---
name: voice-reference-clip
description: >
  Make a reference clip for an Exponential custom voice: pick 8-15 seconds of
  calm, clean speech, cut it to a mono WAV, write its exact transcript, and
  enter both in Settings → Voice. Use when a member wants Exponential's German
  voice to speak in their own voice (or one they have consent to use).
---

# A reference clip for your Exponential voice

> **Coming soon:** Exponential's German voice and voice cloning arrive in an
> upcoming IDE update. You can prepare and enter your clip now; until that
> update, replies are spoken in English with the built-in voice, and German
> replies are shown, not spoken. The English voice does not clone a clip.

Exponential's German voice clones from ONE short clip: no training and no
upload. The clip stays on your voice device, and Settings → Voice only stores
its path and its transcript. A good clip matters more than a long one: a calm
9-second clip beat a 28-second one in our tests.

## Consent first

Use your own voice, or a voice whose owner agreed to this use. Do not clone
anyone without consent. The voice model's use policy (Qwen) forbids
impersonation and deception, and so do we.

## 1. Pick the take

- 8 to 15 seconds of ONE speaker, speaking calmly in German.
- No music, no background noise, no laughter, no effort sounds, no reverb.
- Whole sentences at a normal pace. A neutral tone clones better than an
  excited or whispered one.
- Record with a decent microphone 15 to 30 cm away, or pick a clean stretch
  from an existing recording.

## 2. Cut it to a WAV

With ffmpeg (start time and duration in seconds):

```bash
mkdir -p ~/voice
ffmpeg -ss 12.5 -t 10 -i recording.m4a -ac 1 -ar 24000 -c:a pcm_s16le ~/voice/ref.wav
```

- Mono, 24 kHz, 16-bit PCM.
- Start and end on silence, not mid-word. Leave about 0.2 s of silence at
  each end.
- Listen to the result once, all the way through.

## 3. Write the transcript

Type exactly what the clip says, word for word, with normal punctuation. Leave
out filler you can't hear clearly. A transcript that doesn't match the audio
is the most common reason a cloned voice sounds off.

## 4. Enter it in Exponential

In Settings → Voice, pick your desktop as the voice device. Then:

- **Language:** Deutsch.
- **Voice file:** the WAV's full path on that desktop, e.g.
  `/home/you/voice/ref.wav`.
- **Transcript:** the text from step 3.

Leave Voice file empty for the built-in German voice. German is spoken only on
a desktop with a GPU; without one, German replies are shown, not spoken.
