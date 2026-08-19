# @exp/emoji

The one emoji dataset (EXP-551). `scripts/generate.ts` projects
[emojibase-data](https://github.com/milesj/emojibase) (MIT, `LICENSE-emojibase.txt`)
into a compact JSON committed byte-identically into all four clients:

| Client  | Output                                        | Loaded via              |
| ------- | --------------------------------------------- | ----------------------- |
| Web     | `apps/web/src/lib/emoji.generated.json`       | lazy `import()` chunk   |
| iOS     | `apps/ios/Exponential/Resources/emoji.json`   | `Bundle.main` resource  |
| Android | `apps/android/app/src/main/assets/emoji.json` | `assets.open`           |
| Desktop | `apps/desktop/assets/emoji.json`              | `include_str!`          |

Regenerate with `bun run --filter @exp/emoji generate` (needs `bun install`);
`apps/web/src/lib/codegen-drift.test.ts` fails when the outputs are stale.

## Shape

```jsonc
{
  "version": "16.0.3",            // emojibase-data version
  "groups": ["Smileys & emotion", …, "Flags"],   // 9 labels, indexed by g
  "emojis": [                     // display order
    { "u": "👍", "l": "thumbs up", "g": 1, "s": ["+1", "thumbsup"], "t": ["…"],
      "k": ["👍🏻", "👍🏼", "👍🏽", "👍🏾", "👍🏿"] }   // k only when all 5 uniform tones exist
  ]
}
```

Pickers insert the unicode (`u`, or `k[tone]`), never a `:shortcode:` — the
markdown bodies are shared plain GFM and only unicode renders on every client.
Shared semantics every client implements: search ranks shortcode-prefix over
label-prefix over tag/substring; the `:shortcode` typeahead triggers after
whitespace with at least two characters (`(?:^|\s):([a-z0-9_+-]{2,})(:?)$`) and
auto-commits an exact `:code:`; recents keep the last 24 picks.
