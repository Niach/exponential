# The ergonomics eval (VAPP-108)

How well an agent builds app screens from `catalogPrompt()`, measured: a
corpus of app-building prompts, the model's replies cached as fixtures, every
reply validated, rendered and scored. CI replays the fixtures and never calls
a model.

| file | what |
|---|---|
| `corpus.json` | the prompts: id, the prompt text, the web widths it renders at, the app screen it mirrors (`views.json` id), what the rubric expects of it |
| `fixtures/<id>.json` | one recorded reply per prompt: the model, the prompt variant, the usage, `recordedBy` (`api` = record.ts over the Anthropic API; `session` = the session's own model from the same prompt when no key was available), and the reply `text` verbatim |
| `lib.ts` | the pure half: the reply parser (JSONL, a fenced block, an array, prose around them), the surface, the validator pass, the rubric, the spacing stats, the aggregate |
| `record.ts` | `bun run eval:record [--only id] [--force] [--model] [--variant] [--effort]`: records the corpus (Anthropic SDK, `claude-opus-5-5` by default, streaming, the system prompt cached, `fallbacks: "default"`); credentials from the environment |
| `score.ts` | `bun run eval`: scores every fixture → `report.json` (the numbers the issue, the site's `/eval/` page and `eval.test.ts` read) |
| `render.ts` | `bun run eval:render`: paints every surface in the React harness (Playwright) at each width → `out/<id>/web-<w>.png` (gitignored) and the measured boxes into `report.json` |
| `parity.ts` | `bun run eval:parity`: the Devices template vs the native Devices page on the stored shots (web + desktop): band height, title inset, first row inset, the gap under the band, a pixel diff of the band → `report.json` |
| `rewrite-gaps.json` | every app screen classified expressible / macro / extension / native, with the candidate macros; `gaps.test.ts` keeps it in lockstep with `views.json` |
| `report.json` | the eval on record (generated; `eval.test.ts` fails when the rows drift from the fixtures) |

Rerun order after a prompt or catalog change: `eval:record` (by hand, with a
key) → `eval` → `eval:render` → `eval:parity` (after a shots refresh). The
natives render through the shots pipeline's specimen lane once a corpus
surface is a view (follow-up on VAPP-108).
