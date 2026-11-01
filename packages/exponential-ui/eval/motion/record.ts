// EXP-1603: records the motion eval (prompts.json × {dialect, lottie})
// through `claude -p` on the machine's Claude login (no tools, the guide as
// the whole system prompt, from an empty dir), like eval/record.ts
// `--via claude-cli`. CI never runs this; tests/eval.rs (eui-motion) scores
// the fixtures.
//
//   bun eval/motion/record.ts [--only id,id] [--variant dialect|lottie] [--model sonnet] [--force] [--jobs 6]

import { spawn } from "node:child_process"
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs"
import { tmpdir } from "node:os"
import { dirname, join } from "node:path"

const here = dirname(new URL(import.meta.url).pathname)
const argv = process.argv.slice(2)
const opt = (name: string, fallback: string) => {
  const i = argv.indexOf(`--${name}`)
  return i >= 0 && argv[i + 1] ? argv[i + 1] : fallback
}
const model = opt(`model`, `sonnet`)
const only = opt(`only`, ``).split(`,`).filter(Boolean)
const variants = opt(`variant`, `dialect,lottie`).split(`,`)
const force = argv.includes(`--force`)
const jobs = Number(opt(`jobs`, `6`))

const prompts = (JSON.parse(readFileSync(join(here, `prompts.json`), `utf8`)) as { prompts: { id: string; prompt: string }[] }).prompts.filter(
  (p) => only.length === 0 || only.includes(p.id)
)
const guides: Record<string, string> = {
  dialect: readFileSync(join(here, `guide.md`), `utf8`),
  lottie: readFileSync(join(here, `lottie-guide.md`), `utf8`),
}
const SYSTEM = (variant: string) =>
  `You write animations for an app's UI. Reply with ONE JSON document in a \`\`\`json fenced block and nothing else.\n\n${guides[variant]}`

function record(variant: string, id: string, prompt: string): Promise<void> {
  const out = join(here, `fixtures`, variant, `${id}.json`)
  if (existsSync(out) && !force) return Promise.resolve()
  mkdirSync(dirname(out), { recursive: true })
  const scratch = mkdtempSync(join(tmpdir(), `motion-eval-`))
  const systemFile = join(scratch, `system.txt`)
  writeFileSync(systemFile, SYSTEM(variant))
  const args = [`-p`, `--model`, model, `--tools`, ``, `--strict-mcp-config`, `--no-session-persistence`, `--setting-sources`, ``, `--system-prompt-file`, systemFile, `--output-format`, `json`, prompt]
  return new Promise((resolve, reject) => {
    const child = spawn(`claude`, args, { cwd: scratch })
    let stdout = ``
    let stderr = ``
    child.stdout.on(`data`, (d) => (stdout += d))
    child.stderr.on(`data`, (d) => (stderr += d))
    child.on(`close`, (code) => {
      rmSync(scratch, { recursive: true, force: true })
      if (code !== 0) return reject(new Error(`${variant}/${id}: claude -p exited ${code}: ${(stderr || stdout).slice(0, 400)}`))
      const r = JSON.parse(stdout) as { result?: string; is_error?: boolean; usage: Record<string, number>; modelUsage?: Record<string, unknown> }
      if (r.is_error || typeof r.result !== `string`) return reject(new Error(`${variant}/${id}: ${stdout.slice(0, 400)}`))
      writeFileSync(
        out,
        JSON.stringify(
          {
            id,
            variant,
            prompt,
            model: Object.keys(r.modelUsage ?? {})[0] ?? model,
            recordedAt: new Date().toISOString(),
            recordedBy: `claude-cli`,
            usage: { inputTokens: r.usage.input_tokens ?? 0, outputTokens: r.usage.output_tokens ?? 0 },
            text: r.result,
          },
          null,
          2
        ) + `\n`
      )
      console.log(`${variant}/${id}: ${r.usage.output_tokens} output tokens`)
      resolve()
    })
  })
}

const queue = variants.flatMap((v) => prompts.map((p) => () => record(v, p.id, p.prompt)))
let failed = 0
await Promise.all(
  Array.from({ length: jobs }, async () => {
    for (let task = queue.shift(); task; task = queue.shift()) {
      try {
        await task()
      } catch (error) {
        failed += 1
        console.error(String(error))
      }
    }
  })
)
if (failed) process.exit(1)
