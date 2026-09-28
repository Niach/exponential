/* ─── Proof: "Built inside itself" (MKT-9) ───
   This repo's own history, read from the COMMITTED
   lib/commit-stats.generated.json (bun run stats:marketing): a GitHub-style
   daily heatmap, the monthly human/agent split and stat tiles. Every number
   comes from the JSON, never hard-coded, and carries its "as of" date.
   Colors: the heatmap is ONE sequential ramp (white alphas, more = brighter
   on the dark ground); the split is a validated two-slot categorical pair
   (agent green / human blue, dataviz validator: all checks pass on dark).
   All date math is UTC string math so SSR and hydration agree. */
import { useState } from "react"
import { motion } from "motion/react"
import { sectionReveal } from "../lib/animations"
import STATS from "../lib/commit-stats.generated.json"

type Day = [date: string, total: number, agent: number]

const DAYS = STATS.days as Day[]
const BY_DATE = new Map(DAYS.map((d) => [d[0], d]))
const MONTHS_SHORT = [
  `Jan`,
  `Feb`,
  `Mar`,
  `Apr`,
  `May`,
  `Jun`,
  `Jul`,
  `Aug`,
  `Sep`,
  `Oct`,
  `Nov`,
  `Dec`,
]
const MONTHS_LONG = [
  `January`,
  `February`,
  `March`,
  `April`,
  `May`,
  `June`,
  `July`,
  `August`,
  `September`,
  `October`,
  `November`,
  `December`,
]
const WEEKDAYS = [`Sun`, `Mon`, `Tue`, `Wed`, `Thu`, `Fri`, `Sat`]

function parse(date: string): number {
  const [y, m, d] = date.split(`-`).map(Number)
  return Date.UTC(y, m - 1, d)
}
function iso(ms: number): string {
  return new Date(ms).toISOString().slice(0, 10)
}
function pretty(date: string): string {
  const ms = parse(date)
  const d = new Date(ms)
  return `${WEEKDAYS[d.getUTCDay()]}, ${MONTHS_SHORT[d.getUTCMonth()]} ${d.getUTCDate()}`
}
function longDate(date: string): string {
  const d = new Date(parse(date))
  return `${MONTHS_LONG[d.getUTCMonth()]} ${d.getUTCDate()}, ${d.getUTCFullYear()}`
}
const fmt = (n: number) => n.toLocaleString(`en-US`)

/* Sequential buckets (0 · 1–4 · 5–14 · 15–29 · 30+ commits a day). */
const LEVELS = [1, 5, 15, 30]
function level(total: number): number {
  let l = 0
  for (const t of LEVELS) if (total >= t) l += 1
  return l
}

const DAY_MS = 86_400_000
const CELL = 12
const GAP = 3
const STEP = CELL + GAP
const TOP = 18 /* month-label band */
const LEFT = 26 /* weekday-label band */

/* Monday-first weeks from the first commit's week to the stats date. */
const start = (() => {
  const ms = parse(STATS.firstDay ?? STATS.generatedAt)
  const dow = (new Date(ms).getUTCDay() + 6) % 7
  return ms - dow * DAY_MS
})()
const end = parse(STATS.lastDay ?? STATS.generatedAt)
const WEEKS = Math.floor((end - start) / DAY_MS / 7) + 1

type Cell = { date: string; x: number; y: number; total: number; agent: number }
const CELLS: Cell[] = []
const MONTH_TICKS: { label: string; x: number }[] = []
for (let w = 0; w < WEEKS; w++) {
  for (let r = 0; r < 7; r++) {
    const ms = start + (w * 7 + r) * DAY_MS
    if (ms > end) break
    const date = iso(ms)
    const d = BY_DATE.get(date)
    CELLS.push({
      date,
      x: LEFT + w * STEP,
      y: TOP + r * STEP,
      total: d?.[1] ?? 0,
      agent: d?.[2] ?? 0,
    })
    if (date.endsWith(`-01`) || (w === 0 && r === 0)) {
      MONTH_TICKS.push({
        label: MONTHS_SHORT[new Date(ms).getUTCMonth()],
        x: LEFT + w * STEP,
      })
    }
  }
}
/* A tick in week 0 that a month-start also lands in collapses to the latter. */
const TICKS = MONTH_TICKS.filter(
  (t, i) => !(i === 0 && MONTH_TICKS[1] && MONTH_TICKS[1].x - t.x < STEP * 3)
)
const WIDTH = LEFT + WEEKS * STEP
const HEIGHT = TOP + 7 * STEP

const MONTHS = STATS.months
const MAX_MONTH = Math.max(...MONTHS.map((m) => m.total))
const latest = MONTHS[MONTHS.length - 1]
const latestName = MONTHS_LONG[Number(latest.month.slice(5, 7)) - 1]
const latestIsPartial = STATS.generatedAt.slice(0, 7) === latest.month
const agentPct = Math.round((STATS.agent / STATS.total) * 100)

function Heatmap() {
  const [hover, setHover] = useState<Cell | null>(null)
  const readout = hover
    ? `${pretty(hover.date)}: ${fmt(hover.total)} commit${hover.total === 1 ? `` : `s`}${hover.total ? `, ${fmt(hover.agent)} by agents` : ``}`
    : `${fmt(STATS.total)} commits since ${longDate(STATS.firstDay ?? STATS.generatedAt)}`

  return (
    <figure className={`bp-heat`}>
      <svg
        className={`bp-heat-svg`}
        viewBox={`0 0 ${WIDTH} ${HEIGHT}`}
        role={`img`}
        aria-label={`Daily commits to the Exponential repository. ${readout}.`}
        onMouseLeave={() => setHover(null)}
      >
        {TICKS.map((t) => (
          <text key={`${t.label}-${t.x}`} className={`bp-axis`} x={t.x} y={11}>
            {t.label}
          </text>
        ))}
        {[`Mon`, `Wed`, `Fri`].map((d, i) => (
          <text
            key={d}
            className={`bp-axis`}
            x={0}
            y={TOP + (i * 2) * STEP + CELL - 2}
          >
            {d}
          </text>
        ))}
        {CELLS.map((c) => (
          <rect
            key={c.date}
            className={`bp-cell is-l${level(c.total)}${hover?.date === c.date ? ` is-hover` : ``}`}
            x={c.x}
            y={c.y}
            width={CELL}
            height={CELL}
            rx={2.5}
            onMouseEnter={() => setHover(c)}
          />
        ))}
      </svg>
      <figcaption className={`bp-heat-caption`}>
        <span className={`bp-readout`} aria-live={`polite`}>
          {readout}
        </span>
        <span className={`bp-scale`} aria-hidden>
          Less
          {[0, 1, 2, 3, 4].map((l) => (
            <span key={l} className={`bp-swatch is-l${l}`} />
          ))}
          More
        </span>
      </figcaption>
    </figure>
  )
}

function MonthlySplit() {
  return (
    <figure className={`bp-split`}>
      <div className={`bp-legend`}>
        <span>
          <span className={`bp-key is-agent`} /> Agent runs in Exponential
        </span>
        <span>
          <span className={`bp-key is-human`} /> Human
        </span>
      </div>
      <ul className={`bp-rows`}>
        {MONTHS.map((m) => {
          const human = m.total - m.agent
          return (
            <li key={m.month} className={`bp-row`}>
              <span className={`bp-row-label`}>
                {MONTHS_SHORT[Number(m.month.slice(5, 7)) - 1]}
              </span>
              <span className={`bp-row-track`}>
                <span
                  className={`bp-bar`}
                  style={{ width: `${(m.total / MAX_MONTH) * 100}%` }}
                >
                  {m.agent > 0 && (
                    <span
                      className={`bp-seg is-agent`}
                      style={{ flexGrow: m.agent }}
                      title={`${fmt(m.agent)} by agents`}
                    />
                  )}
                  {human > 0 && (
                    <span
                      className={`bp-seg is-human`}
                      style={{ flexGrow: human }}
                      title={`${fmt(human)} by hand`}
                    />
                  )}
                </span>
              </span>
              <span className={`bp-row-value`}>
                {m.agent > 0
                  ? `${fmt(m.agent)} of ${fmt(m.total)}`
                  : fmt(m.total)}
              </span>
            </li>
          )
        })}
      </ul>
      <details className={`bp-table`}>
        <summary>Show as table</summary>
        <table>
          <thead>
            <tr>
              <th scope={`col`}>Month</th>
              <th scope={`col`}>Commits</th>
              <th scope={`col`}>By agents</th>
              <th scope={`col`}>Share</th>
            </tr>
          </thead>
          <tbody>
            {MONTHS.map((m) => (
              <tr key={m.month}>
                <th scope={`row`}>
                  {MONTHS_LONG[Number(m.month.slice(5, 7)) - 1]}{` `}
                  {m.month.slice(0, 4)}
                </th>
                <td>{fmt(m.total)}</td>
                <td>{fmt(m.agent)}</td>
                <td>{Math.round((m.agent / m.total) * 100)}%</td>
              </tr>
            ))}
          </tbody>
        </table>
      </details>
    </figure>
  )
}

export function BuildProof() {
  return (
    <section className={`bp-section`} id={`proof`}>
      <div className={`shell`}>
        <motion.div {...sectionReveal}>
          <div className={`section-eyebrow`}>Proof</div>
          <h2 className={`section-title`}>Built inside itself.</h2>
          <p className={`section-sub`}>
            Exponential is developed in Exponential. When a run inside it
            pushes code, the commit is authored by the Exponential agent, so
            the repository shows exactly how much of the work the loop does.
          </p>
        </motion.div>

        <motion.div className={`bp-stats`} {...sectionReveal}>
          <div className={`bp-stat`}>
            <span className={`bp-stat-value`}>{agentPct}%</span>
            <span className={`bp-stat-label`}>
              of all {fmt(STATS.total)} commits were written by agent runs
              inside Exponential
            </span>
          </div>
          <div className={`bp-stat`}>
            <span className={`bp-stat-value`}>
              {fmt(latest.agent)}
              <span className={`bp-stat-of`}> of {fmt(latest.total)}</span>
            </span>
            <span className={`bp-stat-label`}>
              commits in {latestName}
              {latestIsPartial ? ` so far` : ``} came from agent runs
            </span>
          </div>
          <div className={`bp-stat`}>
            <span className={`bp-stat-value`}>4</span>
            <span className={`bp-stat-label`}>
              native codebases shipped side by side: web, Rust desktop, Swift
              and Kotlin
            </span>
          </div>
        </motion.div>

        <motion.div className={`bp-charts`} {...sectionReveal}>
          <Heatmap />
          <MonthlySplit />
        </motion.div>
        <p className={`bp-asof`}>
          From the public git history of github.com/Niach/exponential, as of{` `}
          {longDate(STATS.generatedAt)}.
        </p>

        <div className={`bp-bottom`}>
          <motion.blockquote className={`bp-quote`} {...sectionReveal}>
            <p>
              “I started with a basic Linear clone, but I kept changing it to
              fit my workflow. A few weeks in, I made the complete switch and
              now I code everything with this one tool. Using it all day showed
              me a ton of things that make me faster. I can handle four
              codebases at once: a web app, a native Rust desktop app and
              Kotlin and Swift mobile apps, all in the same style, synced in
              realtime and usable as a team.”
            </p>
            <footer>Dennis, who builds Exponential</footer>
          </motion.blockquote>

          <motion.aside className={`bp-next`} {...sectionReveal}>
            <span className={`bp-next-badge`}>Exploring</span>
            <h3>Next: Autopilot</h3>
            <p>
              We are exploring a mode where workflows code a board, release,
              test the result and loop. It is early and not a promise yet, and
              today every merge still waits for a person.
            </p>
          </motion.aside>
        </div>
      </div>
    </section>
  )
}
