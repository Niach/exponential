/* The benchmark data the Numbers page renders (packages/exponential-ui-bench):
   the scenario manifest, the committed results per machine, the budgets and
   the findings, plus the number format the manifest's kinds read as. */
import manifest from "@exponential_app/eui-bench/scenarios/manifest.json"
import budgets from "@exponential_app/eui-bench/budgets.json"
import findings from "@exponential_app/eui-bench/findings.json"
import { RESULTS } from "@exponential_app/eui-bench/results"

export { budgets, findings, manifest, RESULTS }

export type Kind = `ms` | `count` | `bytes` | `mb`
export type Metric = { id: string; kind: string; what: string; optional?: boolean }
export type Finding = { renderer: string; scenario: string; metric: string; value: number; machine: string; cause: string }

export const benchLink = (file: string) => `https://github.com/Niach/exponential/blob/master/packages/exponential-ui-bench/${file}`
export const rendererLabel = (id: string) => manifest.renderers.find((r) => r.id === id)?.label ?? id
export const metricsOf = (scenario: { metrics: unknown }) => scenario.metrics as Metric[]

/** A number the way the manifest's kind reads it (en-US). */
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

/** The renderers a results file has, in the manifest's order. */
export const renderersOf = (runs: Record<string, unknown>) => manifest.renderers.map((r) => r.id).filter((id) => id in runs)
