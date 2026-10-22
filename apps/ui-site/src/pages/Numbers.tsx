/* Numbers (VAPP-105): the benchmark suite, rendered from the committed
   results. What every renderer counts as a pass, the scenarios (from the
   generated manifest), the numbers per machine (metric rows × renderer
   columns, per scenario), the weak points the numbers exposed with their
   follow-up issues, and how to run the suite and read the ratchet. */
import type { ReactNode } from "react"
import manifest from "@exponential-at/ui-bench/scenarios/manifest.json"
import budgets from "@exponential-at/ui-bench/budgets.json"
import findings from "@exponential-at/ui-bench/findings.json"
import { RESULTS } from "@exponential-at/ui-bench/results"
import { DocsCode, DocsLayout, DocsSection, type DocsSection as DocsSectionType } from "@exp/site-shell"
import { DocsTable } from "../components/Content"
import { LEARN_NAV } from "../lib/content"
import type { PageProps } from "../lib/routes"
import { LINKS } from "../lib/site"

const SECTIONS: DocsSectionType[] = [
  { id: `passes`, num: `01`, label: `What is measured` },
  { id: `scenarios`, num: `02`, label: `The scenarios` },
  { id: `numbers`, num: `03`, label: `The numbers` },
  { id: `weak-points`, num: `04`, label: `Weak points` },
  { id: `run`, num: `05`, label: `Run it, the ratchet` },
]

type Kind = `ms` | `count` | `bytes` | `mb`
type Metric = { id: string; kind: string; what: string; optional?: boolean }
type Finding = { renderer: string; scenario: string; metric: string; value: number; machine: string; issue: string; cause: string }

const benchLink = (file: string) => LINKS.source(`packages/exponential-ui-bench/${file}`)
const rendererLabel = (id: string) => manifest.renderers.find((r) => r.id === id)?.label ?? id

/** A number the way the manifest's kind reads it (en-US, prerender-safe). */
export function formatMetric(value: number | undefined, kind: Kind): string {
  if (value === undefined) return `—`
  switch (kind) {
    case `ms`:
      return value < 10 ? value.toFixed(2) : value < 100 ? value.toFixed(1) : Math.round(value).toLocaleString(`en-US`)
    case `mb`:
      return value.toFixed(1)
    case `bytes`:
      return value >= 1_048_576 ? `${(value / 1_048_576).toFixed(1)} MB` : value >= 1024 ? `${(value / 1024).toFixed(1)} kB` : `${Math.round(value)} B`
    default:
      return Math.round(value).toLocaleString(`en-US`)
  }
}

const unit = (kind: Kind) => (kind === `ms` ? `ms` : kind === `mb` ? `MB` : kind === `bytes` ? `` : ``)

/** The renderers a results file has, in the manifest's order. */
const renderersOf = (runs: Record<string, unknown>) => manifest.renderers.map((r) => r.id).filter((id) => id in runs)

export default function NumbersPage({ path }: PageProps) {
  const budgetedMachines = Object.keys(budgets.machines)
  return (
    <>
      <section className="docs-hero content-hero">
        <div className="shell docs-hero-content">
          <h1>Numbers</h1>
        </div>
      </section>

      <DocsLayout nav={LEARN_NAV} title="Exponential UI" sections={SECTIONS} currentPath={path}>
        <DocsSection id="passes" num="01" label="What is measured">
          <h2>What is measured</h2>
          <p>
            The same {manifest.scenarios.length} scenarios run through every renderer, from one set of fixtures (
            <a href={benchLink(`scenarios`)}>scenarios/</a>). Every number below is wall time on the main thread for one{` `}
            <em>pass</em>, and what a pass includes differs by renderer, so compare a renderer with itself across scenarios
            first, and renderers with each other only where the column says the same thing:
          </p>
          <DocsTable
            className="bench-renderers"
            head={[`Renderer`, `One pass is`]}
            rows={manifest.renderers.map((r) => [<strong key="r">{r.label}</strong>, r.pass])}
          />
          <p>
            Counts (nodes, measure calls, facade calls, bytes) come from counters that live behind a debug flag in each
            renderer and cost nothing in a release build; memory is the process footprint after the scenario. A renderer
            that cannot measure an optional metric leaves it out.
          </p>
        </DocsSection>

        <DocsSection id="scenarios" num="02" label="The scenarios">
          <h2>The scenarios</h2>
          <p>
            Generated from <a href={benchLink(`src/scenarios.ts`)}>src/scenarios.ts</a> into one JSON file each (the
            surface, the data or the spec to generate it, the viewport, the theme, the steps, the metrics), drift-gated.
          </p>
          <DocsTable
            className="bench-scenarios"
            head={[`Scenario`, `What it exercises`, `Metrics`]}
            rows={manifest.scenarios.map((s) => [
              <a key="id" href={benchLink(s.file)}>
                <strong>{s.title}</strong>
              </a>,
              s.what,
              <span key="m">
                {(s.metrics as Metric[]).map((m, i) => (
                  <span key={m.id} title={m.what}>
                    {i > 0 ? `, ` : ``}
                    <code>{m.id}</code>
                    {m.optional ? `*` : ``}
                  </span>
                ))}
              </span>,
            ])}
          />
          <p className="conf-total">* optional: a renderer without the counter leaves it out.</p>
        </DocsSection>

        <DocsSection id="numbers" num="03" label="The numbers">
          <h2>The numbers</h2>
          {RESULTS.length === 0 ? (
            <p>No results are committed yet: run the suite (below) and commit <code>results/&lt;machine&gt;.json</code>.</p>
          ) : (
            <p>
              One block per machine (<a href={benchLink(`results`)}>results/</a>): every renderer that ran there, column
              by column, metric rows per scenario. Milliseconds unless the row says otherwise; lower is better everywhere.
              Numbers are machine-dependent, so the machine is part of the record.
            </p>
          )}
          {RESULTS.map((file) => {
            const renderers = renderersOf(file.runs)
            return (
              <div key={file.machine.id} className="bench-machine">
                <h3>
                  <code>{file.machine.id}</code>
                </h3>
                <p className="conf-total">
                  {file.machine.cpu}, {file.machine.cores} cores, {file.machine.memoryGb} GB, {file.machine.os}.{` `}
                  {renderers.map((id, i) => {
                    const run = file.runs[id]
                    return (
                      <span key={id}>
                        {i > 0 ? ` · ` : ``}
                        {rendererLabel(id)}
                        {run.device ? ` on ${run.device}` : ``} ({run.date.slice(0, 10)}
                        {run.commit ? `, ${run.commit}` : ``})
                      </span>
                    )
                  })}
                </p>
                {manifest.scenarios.map((s) => {
                  const metrics = (s.metrics as Metric[]).filter((m) => renderers.some((r) => file.runs[r].scenarios[s.id]?.[m.id] !== undefined))
                  if (metrics.length === 0) return null
                  return (
                    <DocsTable
                      key={s.id}
                      className="bench-table"
                      head={[s.title, ...renderers.map((r) => rendererLabel(r))]}
                      rows={metrics.map((m) => [
                        <span key="m" title={m.what}>
                          <code>{m.id}</code>
                          {unit(m.kind as Kind) && m.kind !== `ms` ? ` (${unit(m.kind as Kind)})` : ``}
                        </span>,
                        ...renderers.map((r) => (
                          <span key={r} className="conf-num">
                            {formatMetric(file.runs[r].scenarios[s.id]?.[m.id], m.kind as Kind)}
                          </span>
                        )),
                      ])}
                    />
                  )
                })}
              </div>
            )
          })}
        </DocsSection>

        <DocsSection id="weak-points" num="04" label="Weak points">
          <h2>Weak points</h2>
          <p>
            Where a renderer loses the core's speed, with the number that showed it and the suspected cause. Each one is
            an issue; the optimisation lands there, with the budget it lowers.
          </p>
          {(findings as Finding[]).length === 0 ? (
            <p>None recorded yet.</p>
          ) : (
            <DocsTable
              className="bench-findings"
              head={[`Renderer`, `Scenario · metric`, `Value`, `Suspected cause`, `Issue`]}
              rows={(findings as Finding[]).map((f) => {
                const kind = ((manifest.scenarios.find((s) => s.id === f.scenario)?.metrics as Metric[] | undefined)?.find((m) => m.id === f.metric)?.kind ?? `ms`) as Kind
                return [
                  rendererLabel(f.renderer),
                  <span key="s">
                    {f.scenario} · <code>{f.metric}</code>
                  </span>,
                  <span key="v" className="conf-num">
                    {formatMetric(f.value, kind)}
                    {kind === `ms` ? ` ms` : ``} <span className="bench-machine-tag">({f.machine})</span>
                  </span>,
                  f.cause,
                  <a key="i" href={`https://app.exponential.at/t/feedback/boards/vapps/issues/${f.issue}`}>
                    <code>{f.issue}</code>
                  </a>,
                ]
              })}
            />
          )}
        </DocsSection>

        <DocsSection id="run" num="05" label="Run it, the ratchet">
          <h2>Run it, the ratchet</h2>
          <p>
            One command runs every runner the machine can (React in headless Chromium, the Rust core, gpui on Linux, SwiftUI
            on macOS and the iPhone Simulator, Compose on the JVM, and an attached iPhone or Android device when there is
            one), merges the fragments into <code>results/&lt;machine&gt;.json</code> and prints the ratchet's verdicts:
          </p>
          <DocsCode>{`bun run --filter @exponential-at/ui-bench bench                 # everything this machine can run
bun run --filter @exponential-at/ui-bench bench -- --renderers react,core --scenario list-100k --repeat 5
bun run --filter @exponential-at/ui-bench machine               # the id results are filed under`}</DocsCode>
          <p>
            CI (<a href={LINKS.source(`.github/workflows/exponential-ui.yml`)}>exponential-ui.yml</a>) runs each runner
            in its renderer's job and ratchets every fragment against{` `}
            <a href={benchLink(`budgets.json`)}>budgets.json</a>: a metric worse than its budget fails the build; a
            count that got better fails too, until its budget is lowered in the same change; a timing far under its budget
            prints a warning asking to lower it. A machine without budgets is reported, never gated: after a green run,
            record it from the artifacts.
          </p>
          <DocsCode>{`bun run --filter @exponential-at/ui-bench ratchet .bench/*.json           # the verdicts
bun run --filter @exponential-at/ui-bench ratchet .bench/*.json --write   # timings × headroom, counts exact`}</DocsCode>
          <p className="conf-total">
            Budgeted machines today: {budgetedMachines.length ? budgetedMachines.map((m) => <code key={m}>{m}</code>).reduce<ReactNode[]>((acc, el, i) => (i ? [...acc, `, `, el] : [el]), []) : `none yet`}.
          </p>
        </DocsSection>
      </DocsLayout>
    </>
  )
}
