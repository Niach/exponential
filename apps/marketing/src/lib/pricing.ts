/* Comparison data mirrors the in-app source of truth:
   apps/web/src/components/team/plan-comparison.tsx + apps/web/src/lib/billing.ts.
   Keep in sync when limits or prices change. */

export type CompareCell = {
  value: string
  good?: boolean
}

export type CompareRow = {
  label: string
  exponential: CompareCell
  linear: CompareCell
}

/* Linear pricing and features as published at linear.app, 2026. Trimmed,
   plain wording — no technical/env-var strings. */
export const linearComparison: CompareRow[] = [
  {
    label: `Paid plans start at`,
    exponential: { value: `$5 / seat / mo`, good: true },
    linear: { value: `$10 / user / mo` },
  },
  {
    label: `Team of 5, one year`,
    exponential: { value: `$300`, good: true },
    linear: { value: `$600` },
  },
  {
    label: `AI coding`,
    exponential: {
      value: `Bring your own agents — run locally, free`,
      good: true,
    },
    linear: { value: `Cloud agents, billed per use` },
  },
  {
    label: `Self-hosting`,
    exponential: {
      value: `Full-featured — free under 10 people`,
      good: true,
    },
    linear: { value: `Not available` },
  },
  {
    label: `Desktop app`,
    exponential: { value: `Native Rust`, good: true },
    linear: { value: `Electron` },
  },
  {
    label: `Mobile apps`,
    exponential: { value: `Native iOS & Android`, good: true },
    linear: { value: `Yes` },
  },
  {
    label: `Source`,
    exponential: { value: `Source-available`, good: true },
    linear: { value: `Proprietary` },
  },
]
