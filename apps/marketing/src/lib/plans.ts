/* ─── Canonical plan data — the ONE marketing-side source of truth ───
   Consumed by PlanCards.tsx (the full cards, rendered on BOTH the home
   pricing section and /pricing/ — EXP-207), PricingPage.tsx (footnote)
   and seo.ts (JSON-LD offers derive from priceNumber, so contact-sales
   cards stay out automatically).

   Values mirror apps/web/src/lib/billing.ts PLAN_LIMITS and the in-app grid
   in apps/web/src/components/team/plan-comparison.tsx — keep the three in
   sync when prices, limits or the canonical bullets change (EXP-176 unified
   them: cards list ONLY the monetized axes — seats, storage, widgets,
   helpdesk, priority support; everything never-gated lives in the shared
   EVERY_PLAN_INCLUDES sentence). EXP-286 collapsed the cloud grid to two
   cards (Free + Team) — Enterprise is a text line, not a card. */
import { LINKS } from "./links"

export type CloudPlan = {
  id: `free` | `team` | `commercial`
  name: string
  amount: string
  /* Set only for self-serve tiers — drives the schema.org Offer list. */
  priceNumber?: number
  priceDescription?: string
  cadence?: string
  note?: string
  tagline: string
  highlight?: boolean
  enterprise?: boolean
  features: string[]
  cta: { label: string; href: string }
}

/* Shown under every plan grid — marketing home, /pricing and the in-app
   comparison carry the same sentence verbatim. */
export const EVERY_PLAN_INCLUDES = `Every plan includes unlimited boards, repos and coding sessions, all native apps, real-time sync, and push, email & remote steer.`

/* Enterprise stopped being a pricing card (EXP-286) — it is a sales
   motion, rendered as one line under the cloud grid. */
export const ENTERPRISE_LINE = `Need SSO, SLA, DPA or a self-host contract?`

export const CLOUD_PLANS: CloudPlan[] = [
  {
    id: `free`,
    name: `Free`,
    amount: `€0`,
    priceNumber: 0,
    cadence: `forever`,
    tagline: `For you and two teammates.`,
    features: [`3 seats`, `250 MB attachment storage`, `1 feedback widget`],
    cta: { label: `Sign up free`, href: LINKS.app.login },
  },
  {
    id: `team`,
    name: `Team`,
    amount: `€12`,
    priceNumber: 12,
    priceDescription: `Per seat, per month, billed yearly. €15 billed monthly.`,
    cadence: `/seat/mo`,
    note: `billed yearly · €15 monthly`,
    tagline: `Everything, for teams that ship together.`,
    highlight: true,
    features: [
      `Everything in Free`,
      `10 GB attachment storage`,
      `Unlimited feedback widgets`,
      `Helpdesk & support inbox`,
      `Priority support`,
    ],
    cta: { label: `Start with Team`, href: LINKS.app.login },
  },
]

export type SelfHostPlan = CloudPlan & { selfHost?: boolean }

/* Run-it-yourself tiers — self-host is free under the Exponential Small
   Team License 1.0 while you are under 10 people; at 10 or more a
   commercial license is required, at a published annual price (EXP-286).
   A different offer from the cloud grid, so its bullets stay independent. */
export const SELF_HOST_PLANS: SelfHostPlan[] = [
  {
    id: `free`,
    name: `Self-hosted`,
    amount: `Free`,
    cadence: `your hardware`,
    tagline: `Free for individuals and small businesses — under 10 people.`,
    selfHost: true,
    features: [
      `Every feature unlocked`,
      `Unlimited seats & storage`,
      `One docker compose`,
      `Source-available (ESTL-1.0)`,
    ],
    cta: { label: `Read self-host docs`, href: `/docs/self-host/` },
  },
  {
    id: `commercial`,
    name: `Commercial license`,
    amount: `€590`,
    cadence: `/year`,
    note: `up to 25 people`,
    tagline: `For companies of 10 or more running it in-house.`,
    enterprise: true,
    features: [
      `€590/yr up to 25 people`,
      `€1,900/yr up to 100 people`,
      `Custom above 100`,
      `One annual invoice`,
      `Extended support`,
    ],
    /* Dedicated contact page with the sales form (EXP-39). */
    cta: { label: `Contact sales`, href: `/contact/` },
  },
]
