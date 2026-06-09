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

export const FOUNDING_CODE = `FOUNDING`

export const exponentialRows = [
  { label: `Price`, free: `$0`, pro: `$18 / year`, business: `$60 / year` },
  { label: `Members per workspace`, free: `1`, pro: `5`, business: `25` },
  { label: `Projects`, free: `3`, pro: `10`, business: `Unlimited` },
  { label: `Attachment storage`, free: `50 MB`, pro: `1 GB`, business: `10 GB` },
  { label: `Owned workspaces`, free: `1`, pro: `3`, business: `10` },
  { label: `Local AI agents`, free: `✓`, pro: `✓`, business: `✓` },
  { label: `Push notifications`, free: `—`, pro: `✓`, business: `✓` },
] as const

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
      value: `Flat per workspace, per year`,
      detail: `The bill doesn't grow with your team.`,
      good: true,
    },
    linear: { value: `Per user, per month` },
  },
  {
    label: `paid entry`,
    exponential: {
      value: `$18 / workspace / year`,
      detail: `Pro — 5 members, 10 projects, push.`,
      good: true,
    },
    linear: { value: `$10 / user / month`, detail: `Basic, billed yearly.` },
  },
  {
    label: `team of 5, one year`,
    exponential: { value: `$18 total`, good: true },
    linear: { value: `$600`, detail: `5 users × $10 × 12 months (Basic).` },
  },
  {
    label: `business tier`,
    exponential: {
      value: `$60 / workspace / year`,
      detail: `25 members, unlimited projects.`,
      good: true,
    },
    linear: { value: `$16 / user / month` },
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
      detail: `Your claude/codex subscription, in a visible terminal, on every tier.`,
      good: true,
    },
    linear: { value: `Cloud-delegated agents` },
  },
  {
    label: `native linux app`,
    exponential: { value: `Yes — Zig + GTK4`, good: true },
    linear: { value: `No` },
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
