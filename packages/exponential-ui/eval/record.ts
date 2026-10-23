#!/usr/bin/env bun
// VAPP-108: records the corpus against Claude, ONCE, into eval/fixtures.
// CI never calls the model: score.ts and the tests read the fixtures. Run it
// by hand when the prompt or the corpus changes:
//
//   bun eval/record.ts [--only <id,id>] [--force] [--model claude-opus-5-5]
//                      [--variant full|terse|lite|lite-terse|index] [--effort medium]
//                      [--via sdk|claude-cli] [--out <dir>]
//
// Credentials come from the environment (ANTHROPIC_API_KEY, or an
// `ant auth login` profile the SDK reads on its own). `--via claude-cli`
// (VAPP-127) records through `claude -p` on the machine's Claude login
// instead, for a machine with no key: the same system prompt (replacing the
// CLI's own), model and effort, no tools, run from an empty directory; the
// usage is what the API counted (it includes ≈ 0.5k tokens of CLI context),
// the fixture says `recordedBy: "claude-cli"`. A fixture that exists
// is kept unless --force; a fixture recorded by the session (recordedBy:
// session, no key at the time) is replaced whenever it is in scope.

import Anthropic from "@anthropic-ai/sdk"
import { spawnSync } from "node:child_process"
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs"
import { tmpdir } from "node:os"
import { join } from "node:path"
import { catalogPrompt } from "../src/prompt"
import type { PromptOptions } from "../src/prompt"
import corpus from "./corpus.json" with { type: "json" }
import type { Corpus, EvalFixture } from "./lib"

const argv = process.argv.slice(2)
const flag = (name: string) => argv.includes(`--${name}`)
const opt = (name: string, fallback: string) => {
  const i = argv.indexOf(`--${name}`)
  return i >= 0 && argv[i + 1] ? argv[i + 1] : fallback
}
const only = new Set(opt(`only`, ``).split(`,`).filter(Boolean))
/** `--out <dir>`: record somewhere else (a measuring run, e.g. `--variant index`, that must not replace the fixtures). */
const DIR = opt(`out`, join(import.meta.dir, `fixtures`))
const force = flag(`force`)
const model = opt(`model`, `claude-opus-5-5`)
const variant = opt(`variant`, `full`) as EvalFixture[`promptVariant`]
const effort = opt(`effort`, `medium`) as `low` | `medium` | `high` | `xhigh` | `max`
const via = opt(`via`, `sdk`) as `sdk` | `claude-cli`

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

/** `claude -p --output-format json` (one turn, no tools, the system prompt
 *  replaced), from an empty scratch dir so no CLAUDE.md or memory rides along. */
function recordViaCli(id: string, prompt: string): EvalFixture {
  const scratch = mkdtempSync(join(tmpdir(), `xui-eval-record-`))
  try {
    const systemFile = join(scratch, `system.txt`)
    writeFileSync(systemFile, SYSTEM)
    const r = spawnSync(
      `claude`,
      [`-p`, `--model`, model, `--effort`, effort, `--tools`, ``, `--strict-mcp-config`, `--no-session-persistence`, `--setting-sources`, ``, `--system-prompt-file`, systemFile, `--output-format`, `json`, prompt],
      { cwd: scratch, encoding: `utf8`, maxBuffer: 64 * 1024 * 1024, timeout: 15 * 60_000 }
    )
    if (r.status !== 0) throw new Error(`${id}: claude -p exited ${r.status}: ${(r.stderr || r.stdout).slice(0, 500)}`)
    const out = JSON.parse(r.stdout) as {
      is_error?: boolean
      result?: string
      stop_reason?: string
      usage: { input_tokens: number; output_tokens: number; cache_creation_input_tokens?: number; cache_read_input_tokens?: number }
      modelUsage?: Record<string, unknown>
    }
    if (out.is_error || typeof out.result !== `string`) throw new Error(`${id}: claude -p failed: ${r.stdout.slice(0, 500)}`)
    if (out.stop_reason === `max_tokens`) console.warn(`${id}: hit max_tokens; the reply is truncated`)
    if (out.stop_reason === `refusal`) throw new Error(`${id}: refused`)
    const u = out.usage
    return {
      id,
      prompt,
      model: Object.keys(out.modelUsage ?? {})[0] ?? model,
      promptVariant: variant,
      recordedAt: new Date().toISOString(),
      recordedBy: `claude-cli`,
      usage: {
        inputTokens: u.input_tokens + (u.cache_creation_input_tokens ?? 0) + (u.cache_read_input_tokens ?? 0),
        outputTokens: u.output_tokens,
        cacheReadInputTokens: u.cache_read_input_tokens ?? 0,
        estimated: false,
      },
      text: out.result,
    }
  } finally {
    rmSync(scratch, { recursive: true, force: true })
  }
}

async function main(): Promise<void> {
  const client = via === `sdk` ? new Anthropic() : undefined
  mkdirSync(DIR, { recursive: true })
  const entries = (corpus as Corpus).prompts.filter((p) => !only.size || only.has(p.id))
  let recorded = 0
  for (const entry of entries) {
    const file = join(DIR, `${entry.id}.json`)
    if (existsSync(file) && !force) {
      const existing = JSON.parse(readFileSync(file, `utf8`)) as EvalFixture
      if (existing.recordedBy === (via === `sdk` ? `api` : `claude-cli`) && existing.promptVariant === variant && existing.model === model) {
        console.log(`${entry.id}: kept (${existing.model}, ${existing.recordedAt})`)
        continue
      }
    }
    process.stdout.write(`${entry.id}: recording on ${model} (${variant}, effort ${effort})… `)
    const fixture = client ? await record(client, entry.id, entry.prompt) : recordViaCli(entry.id, entry.prompt)
    const out = {
      $comment: `VAPP-108 eval fixture: the reply to corpus prompt \`${entry.id}\` given catalogPrompt() (${variant}) as the system prompt, recorded by eval/record.ts ${via === `claude-cli` ? `through \`claude -p\` (the Claude Code CLI on the machine's Claude login, VAPP-127)` : `over the Anthropic API`}. Re-record with \`bun eval/record.ts --force --only ${entry.id}${via === `claude-cli` ? ` --via claude-cli` : ``}\`.`,
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
