# Built-in reference clips (NEW-74)

The voice the speech worker clones when the member's Voice file is empty, one
per spoken-reply language. Both are OUR OWN TTS output, so no speaker's voice
or consent is involved:

| file | made with | text |
|---|---|---|
| `builtin-de.wav` (9.36 s, 24 kHz mono) | Qwen3-TTS 12 Hz 0.6B **CustomVoice** Q8_0 (Apache-2.0), preset speaker `ryan`, seed 7, through CrispASR 0.8.45 | Guten Morgen. Ich habe mir die Änderungen angesehen, und die Tests laufen jetzt sauber durch. Sag mir einfach, was du als Nächstes brauchst. |
| `builtin-en.wav` (9.12 s, 24 kHz mono) | the same model and speaker, seed 7 | Good morning. I've looked through the changes, and the tests are passing cleanly now. Just tell me what you need next. |

Model output of an Apache-2.0 model; the clips carry CrispASR's AI-content
watermark and C2PA marking. The transcripts are `BUILTIN` in `src/builtin.rs`
and must stay the exact words of the clip (the clone reads them).
