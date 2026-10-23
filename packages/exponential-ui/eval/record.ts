#!/usr/bin/env bun
// VAPP-108: records the corpus against Claude, ONCE, into eval/fixtures.
// CI never calls the model: score.ts and the tests read the fixtures. Run it
// by hand when the prompt or the corpus changes:
//
//   bun eval/record.ts [--only <id,id>] [--force] [--model claude-opus-5-5]
//                      [--variant full|terse|lite|lite-terse] [--effort medium]
//
// Credentials come from the environment (ANTHROPIC_API_KEY, or an
// `ant auth login` profile the SDK reads on its own). A fixture that exists
// is kept unless --force; a fixture recorded by the session (recordedBy:
// session, no key at the time) is replaced whenever it is in scope.

import Anthropic from "@anthropic-ai/sdk"
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs"
import { join } from "node:path"
import { catalogPrompt } from "../src/prompt"
import type { PromptOptions } from "../src/prompt"
import corpus from "./corpus.json" with { type: "json" }
import type { Corpus, EvalFixture } from "./lib"

const DIR = join(import.meta.dir, `fixtures`)
const argv = process.argv.slice(2)
const flag = (name: string) => argv.includes(`--${name}`)
const opt = (name: string, fallback: string) => {
  const i = argv.indexOf(`--${name}`)
  return i >= 0 && argv[i + 1] ? argv[i + 1] : fallback
}
const only = new Set(opt(`only`, ``).split(`,`).filter(Boolean))
const force = flag(`force`)
const model = opt(`model`, `claude-opus-5-5`)
const variant = opt(`variant`, `full`) as EvalFixture[`promptVariant`]
const effort = opt(`effort`, `medium`) as `low` | `medium` | `high` | `xhigh` | `max`

const PROMPT_OPTIONS: Record<EvalFixture[`promptVariant`], PromptOptions> = {
  full: {},
  terse: { terse: true },
  lite: { lite: true },
  "lite-terse": { lite: true, terse: true },
  index: { sections: [`rules`, `index`, `shapes`, `functions`, `style`] },
}

const SYSTEM = `${catalogPrompt(PROMPT_OPTIONS[variant])}\nReply with the A2UI messages only, one JSON object per line (JSONL), no prose and no code fence. Use real-looking sample data in updateDataModel so the surface renders.`

async function record(client: Anthropic, id: string, prompt: string): Promise<EvalFixture> {
  // Streaming: a surface can run a few thousand output tokens. The system
  // prompt is identical for every prompt of the run, so it is cached.
  // Opus 5.5 / Fable 5.1: thinking is always on; effort is the dial.
  // `fallbacks: "default"` re-runs a safeguards decline on a fallback model.
  const stream = client.beta.messages.stream({
    model,
    max_tokens: 16000,
    betas: [`server-side-fallback-2026-07-01`],
    fallbacks: `default`,
    output_config: { effort },
    system: [{ type: `text`, text: SYSTEM, cache_control: { type: `ephemeral` } }],
    messages: [{ role: `user`, content: prompt }],
  } as Parameters<typeof client.beta.messages.stream>[0])
  const message = await stream.finalMessage()
  if (message.stop_reason === `refusal`) throw new Error(`${id}: refused (${message.stop_details?.category ?? `?`})`)
  if (message.stop_reason === `max_tokens`) console.warn(`${id}: hit max_tokens; the reply is truncated`)
  const text = message.content
    .filter((b): b is Extract<(typeof message.content)[number], { type: `text` }> => b.type === `text`)
    .map((b) => b.text)
    .join(`\n`)
  return {
    id,
    prompt,
    model: message.model,
    promptVariant: variant,
    recordedAt: new Date().toISOString(),
    recordedBy: `api`,
    usage: {
      inputTokens: message.usage.input_tokens + (message.usage.cache_creation_input_tokens ?? 0) + (message.usage.cache_read_input_tokens ?? 0),
      outputTokens: message.usage.output_tokens,
      cacheReadInputTokens: message.usage.cache_read_input_tokens ?? 0,
      estimated: false,
    },
    text,
  }
}

async function main(): Promise<void> {
  const client = new Anthropic()
  mkdirSync(DIR, { recursive: true })
  const entries = (corpus as Corpus).prompts.filter((p) => !only.size || only.has(p.id))
  let recorded = 0
  for (const entry of entries) {
    const file = join(DIR, `${entry.id}.json`)
    if (existsSync(file) && !force) {
      const existing = JSON.parse(readFileSync(file, `utf8`)) as EvalFixture
      if (existing.recordedBy === `api` && existing.promptVariant === variant && existing.model === model) {
        console.log(`${entry.id}: kept (${existing.model}, ${existing.recordedAt})`)
        continue
      }
    }
    process.stdout.write(`${entry.id}: recording on ${model} (${variant}, effort ${effort})… `)
    const fixture = await record(client, entry.id, entry.prompt)
    const out = {
      $comment: `VAPP-108 eval fixture: the reply to corpus prompt \`${entry.id}\` given catalogPrompt() (${variant}) as the system prompt, recorded by eval/record.ts over the Anthropic API. Re-record with \`bun eval/record.ts --force --only ${entry.id}\`.`,
      ...fixture,
    }
    writeFileSync(file, `${JSON.stringify(out, null, 2)}\n`)
    recorded++
    console.log(`${fixture.usage.outputTokens} output tokens, ${fixture.usage.cacheReadInputTokens} cached input`)
  }
  console.log(`recorded ${recorded} of ${entries.length}; now: bun eval/score.ts`)
}

main().catch((error: unknown) => {
  if (error instanceof Anthropic.AuthenticationError) console.error(`No credentials: set ANTHROPIC_API_KEY or run \`ant auth login\`.`)
  else if (error instanceof Anthropic.RateLimitError) console.error(`Rate limited; rerun with --only for the missing ids.`)
  else if (error instanceof Anthropic.APIError) console.error(`API error ${error.status}: ${error.message}`)
  else console.error(error)
  process.exit(1)
})
