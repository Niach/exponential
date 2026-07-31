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
    label: `Free tier`,
    exponential: { value: `3 seats, full product`, good: true },
    linear: { value: `Limited issues` },
  },
  {
    label: `One paid plan`,
    exponential: {
      value: `€12 / seat / mo billed yearly — everything included`,
      good: true,
    },
    linear: { value: `$10–14 / user / mo, features split across tiers` },
  },
  {
    label: `Helpdesk & feedback widget`,
    exponential: { value: `Built in — one seat price`, good: true },
    linear: { value: `Separate tools, separately billed` },
  },
  {
    label: `AI coding`,
    exponential: {
      value: `Bring your own agents, run them locally`,
      good: true,
    },
    linear: { value: `Cloud agents, billed per use` },
  },
  {
    label: `Self-hosting`,
    exponential: {
      value: `Full-featured — free for everyone`,
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
    exponential: { value: `Open source (Apache-2.0)`, good: true },
    linear: { value: `Proprietary` },
  },
]
