/* Tier data mirrors the in-app source of truth:
   apps/web/src/components/workspace/plan-comparison.tsx + apps/web/src/lib/billing.ts.
   Keep in sync when limits or prices change. */

export type Tier = {
  name: string
  price: string
  cadence: string
  tagline: string
  highlight?: boolean
  features: string[]
  cta: { label: string; href: string }
}

export type CompareCell = {
  value: string
  detail?: string
  good?: boolean
}

export type CompareRow = {
  label: string
  exponential: CompareCell
  linear: CompareCell
}

/* Linear pricing and features as published at linear.app, June 2026. */
export const linearComparison: CompareRow[] = [
  {
    label: `pricing model`,
    exponential: {
      value: `Per seat, agents free`,
      detail: `You never pay for a coding agent — only for people.`,
      good: true,
    },
    linear: { value: `Per user, per month` },
  },
  {
    label: `paid entry`,
    exponential: {
      value: `$5 / seat / month`,
      detail: `Pro, billed yearly — widget, unlimited projects & repos.`,
      good: true,
    },
    linear: { value: `$10 / user / month`, detail: `Basic, billed yearly.` },
  },
  {
    label: `team of 5, one year`,
    exponential: { value: `$300`, detail: `5 seats × $5 × 12 (Pro).`, good: true },
    linear: { value: `$600`, detail: `5 users × $10 × 12 months (Basic).` },
  },
  {
    label: `coding sessions`,
    exponential: {
      value: `Unlimited on every tier`,
      detail: `Projects, repos and Claude sessions are never capped.`,
      good: true,
    },
    linear: { value: `Cloud agents, metered` },
  },
  {
    label: `self-hosting`,
    exponential: {
      value: `Full stack, one docker compose`,
      detail: `SELF_HOSTED=true unlocks everything, free.`,
      good: true,
    },
    linear: { value: `Not available` },
  },
  {
    label: `ai agents`,
    exponential: {
      value: `Run locally on your machine`,
      detail: `Your Claude subscription, in a visible terminal, on every tier.`,
      good: true,
    },
    linear: { value: `Cloud-delegated agents` },
  },
  {
    label: `native desktop app`,
    exponential: { value: `Yes — Rust + gpui`, detail: `A real git IDE — clone, code, PR.`, good: true },
    linear: { value: `Electron wrapper` },
  },
  {
    label: `native apps`,
    exponential: {
      value: `Web, iOS, Android, macOS, Linux`,
      good: true,
    },
    linear: { value: `Web, desktop & mobile apps` },
  },
  {
    label: `real-time sync`,
    exponential: { value: `Yes — ElectricSQL, all clients`, good: true },
    linear: { value: `Yes` },
  },
  {
    label: `source & license`,
    exponential: { value: `Source-available, ELv2`, good: true },
    linear: { value: `Proprietary` },
  },
]
