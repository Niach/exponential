/* Commit stats for the frontpage's "Built inside itself" proof (MKT-9).

     bun run stats:marketing   (repo root)

   Reads this repo's full history and writes src/lib/commit-stats.generated.json:
   daily totals + agent counts, the monthly human/agent split and the totals.
   The JSON is COMMITTED: deploy and CI clones may be shallow, so the build
   never recomputes it. An "agent" commit is one authored by the Exponential
   GitHub App, i.e. a PR opened by a run inside Exponential. */

import { execFileSync } from "node:child_process"
import { writeFileSync } from "node:fs"
import { dirname, resolve } from "node:path"
import { fileURLToPath } from "node:url"

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), `..`)
const OUT = resolve(ROOT, `src/lib/commit-stats.generated.json`)
const AGENT_AUTHOR = `exponential-agent[bot]`

if (
  execFileSync(`git`, [`rev-parse`, `--is-shallow-repository`], {
    cwd: ROOT,
    encoding: `utf8`,
  }).trim() === `true`
) {
  throw new Error(`commit-stats: shallow clone, run git fetch --unshallow`)
}

const log = execFileSync(
  `git`,
  [`log`, `--format=%ad|%an`, `--date=short`],
  { cwd: ROOT, encoding: `utf8`, maxBuffer: 64 * 1024 * 1024 }
)

const days = new Map<string, { total: number; agent: number }>()
for (const line of log.split(`\n`)) {
  if (!line) continue
  const sep = line.indexOf(`|`)
  const date = line.slice(0, sep)
  const author = line.slice(sep + 1)
  const day = days.get(date) ?? { total: 0, agent: 0 }
  day.total += 1
  if (author === AGENT_AUTHOR) day.agent += 1
  days.set(date, day)
}

const sortedDays = [...days.entries()].sort(([a], [b]) => a.localeCompare(b))
const months = new Map<string, { total: number; agent: number }>()
for (const [date, day] of sortedDays) {
  const key = date.slice(0, 7)
  const month = months.get(key) ?? { total: 0, agent: 0 }
  month.total += day.total
  month.agent += day.agent
  months.set(key, month)
}

const total = sortedDays.reduce((n, [, d]) => n + d.total, 0)
const agent = sortedDays.reduce((n, [, d]) => n + d.agent, 0)

const stats = {
  generatedAt: new Date().toISOString().slice(0, 10),
  firstDay: sortedDays[0]?.[0] ?? null,
  lastDay: sortedDays.at(-1)?.[0] ?? null,
  total,
  agent,
  /* [date, total, agent] — tuples keep the committed file small. */
  days: sortedDays.map(([date, d]) => [date, d.total, d.agent]),
  months: [...months.entries()].map(([month, m]) => ({ month, ...m })),
}

writeFileSync(OUT, `${JSON.stringify(stats)}\n`)
console.log(
  `wrote ${OUT}: ${total} commits, ${agent} by agents (${((agent / total) * 100).toFixed(1)}%)`
)
