/* The plans (from apps/marketing/src/lib/plans.ts, which mirrors
   apps/web/src/lib/billing.ts PLAN_LIMITS and the in-app comparison;
   keep the three in sync when prices or limits change). */
import { LINKS } from "./links.ts"

export interface Plan {
  id: `free` | `team` | `selfhost`
  name: string
  amount: string
  /** Set only for self-serve cloud tiers: drives the schema.org offers. */
  priceNumber?: number
  priceDescription?: string
  cadence: string
  /** The monthly-billed price next to the yearly headline price. */
  monthly?: string
  highlight?: boolean
  features: string[]
  /** A technical limitation, drawn with a warning glyph. */
  caveat?: string
  cta: { label: string; href: string }
}

export const EVERY_PLAN_INCLUDES = `Every plan includes unlimited boards, repos and coding sessions, all native apps, real-time sync, and email & remote steer.`

export const PLANS: Plan[] = [
  {
    id: `free`,
    name: `Free`,
    amount: `€0`,
    priceNumber: 0,
    cadence: `forever`,
    features: [`3 seats`, `250 MB attachment storage`, `1 feedback widget`, `Mobile push notifications`],
    cta: { label: `Sign up free`, href: LINKS.app.login },
  },
  {
    id: `team`,
    name: `Team`,
    amount: `€12`,
    priceNumber: 12,
    priceDescription: `Per seat, per month, billed yearly. €15 billed monthly.`,
    cadence: `/seat/mo, yearly`,
    monthly: `€15 monthly`,
    highlight: true,
    features: [`Everything in Free`, `10 GB attachment storage`, `Unlimited feedback widgets`, `Priority support`],
    cta: { label: `Start with Team`, href: LINKS.app.login },
  },
  {
    id: `selfhost`,
    name: `Self-hosted`,
    amount: `Free`,
    cadence: `your hardware`,
    features: [`Unlimited users`, `No plan limits`, `Unlimited storage`, `One docker compose`, `Open source (Apache-2.0)`],
    caveat: `No mobile push`,
    cta: { label: `Self-host`, href: `/docs/get-started/self-host/` },
  },
]

/* Exponential against Linear (published pricing, 2026). */
export const COMPARISON: { label: string; exponential: string; linear: string }[] = [
  { label: `Free tier`, exponential: `3 seats, full product`, linear: `Limited issues` },
  { label: `Paid plan`, exponential: `€12 / seat / mo billed yearly, everything included`, linear: `$10–14 / user / mo, features split across tiers` },
  { label: `Feedback widget`, exponential: `Built in, one seat price`, linear: `Separate tools, separately billed` },
  { label: `AI coding`, exponential: `Bring your own agents, run them locally`, linear: `Cloud agents, billed per use` },
  { label: `Self-hosting`, exponential: `Full-featured, free for everyone`, linear: `Not available` },
  { label: `Desktop app`, exponential: `Native Rust`, linear: `Electron` },
  { label: `Mobile apps`, exponential: `Native iOS & Android`, linear: `Yes` },
  { label: `Source`, exponential: `Open source (Apache-2.0)`, linear: `Proprietary` },
]
