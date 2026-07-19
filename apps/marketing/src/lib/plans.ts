/* ─── Canonical plan data — the ONE marketing-side source of truth ───
   Consumed by HomePricing.tsx (compact cards), PlanCards.tsx (full cards),
   PricingPage.tsx (footnote) and seo.ts (JSON-LD offers derive from
   priceNumber, so contact-sales Enterprise stays out automatically).

   Values mirror apps/web/src/lib/billing.ts PLAN_LIMITS and the in-app grid
   in apps/web/src/components/team/plan-comparison.tsx — keep the three in
   sync when prices, limits or the canonical bullets change (EXP-176 unified
   them: cards list ONLY the monetized axes — seats, storage, widgets,
   helpdesk, priority support; everything never-gated lives in the shared
   EVERY_PLAN_INCLUDES sentence). */
import { LINKS } from "./links"

export type CloudPlan = {
  id: `free` | `pro` | `business` | `enterprise`
  name: string
  amount: string
  /* Set only for self-serve tiers — drives the schema.org Offer list. */
  priceNumber?: number
  priceDescription?: string
  cadence?: string
  note?: string
  tagline: string
  /* One-liner for the compact home-page card. */
  homeTagline: string
  highlight?: boolean
  enterprise?: boolean
  features: string[]
  cta: { label: string; href: string }
}

/* Shown under every plan grid — marketing home, /pricing and the in-app
   comparison carry the same sentence verbatim. */
export const EVERY_PLAN_INCLUDES = `Every plan includes unlimited boards, repos and coding sessions, all native apps, real-time sync, and push, email & remote steer.`

export const CLOUD_PLANS: CloudPlan[] = [
  {
    id: `free`,
    name: `Free`,
    amount: `$0`,
    priceNumber: 0,
    cadence: `forever`,
    tagline: `For you and your side projects.`,
    homeTagline: `For you and your side projects.`,
    features: [`1 seat`, `250 MB storage`, `1 feedback widget`],
    cta: { label: `Sign up free`, href: LINKS.app.login },
  },
  {
    id: `pro`,
    name: `Pro`,
    amount: `$5`,
    priceNumber: 5,
    priceDescription: `Per seat, per month, billed yearly.`,
    cadence: `/seat/mo`,
    note: `· billed yearly`,
    tagline: `For teams that ship together.`,
    homeTagline: `Adds the helpdesk and more widgets.`,
    highlight: true,
    features: [
      `Everything in Free`,
      `5 GB storage`,
      `3 feedback widgets`,
      `Helpdesk & support inbox`,
    ],
    cta: { label: `Start with Pro`, href: LINKS.app.login },
  },
  {
    id: `business`,
    name: `Business`,
    amount: `$10`,
    priceNumber: 10,
    priceDescription: `Per seat, per month, billed monthly or yearly.`,
    cadence: `/seat/mo`,
    note: `monthly or yearly`,
    tagline: `For orgs with room to grow.`,
    homeTagline: `More storage, unlimited widgets.`,
    features: [
      `Everything in Pro`,
      `50 GB storage`,
      `Unlimited feedback widgets`,
      `Priority support`,
    ],
    cta: { label: `Start with Business`, href: LINKS.app.login },
  },
  {
    id: `enterprise`,
    name: `Enterprise`,
    amount: `Custom`,
    cadence: `let's talk`,
    tagline: `For companies that need guarantees.`,
    homeTagline: `For companies that need guarantees.`,
    enterprise: true,
    features: [
      `Everything in Business`,
      `SSO / OIDC (coming soon)`,
      `SLA & DPA`,
      `Dedicated support channel`,
      `Onboarding & migration help`,
    ],
    /* No self-serve checkout — sales form on the contact page (EXP-39). */
    cta: { label: `Contact sales`, href: `/contact/` },
  },
]

export type SelfHostPlan = CloudPlan & { selfHost?: boolean }

/* Run-it-yourself tiers — self-host is free & unlimited; Enterprise is
   contact-sales. A different offer from the cloud grid, so its bullets
   stay independent. */
export const SELF_HOST_PLANS: SelfHostPlan[] = [
  {
    id: `free`,
    name: `Self-hosted`,
    amount: `Free`,
    cadence: `your hardware`,
    tagline: `Free for individuals and small businesses — under 10 people.`,
    homeTagline: `Free for individuals and small businesses.`,
    selfHost: true,
    features: [
      `Every feature unlocked`,
      `Unlimited seats & storage`,
      `One docker compose`,
      `Source-available (ELv2)`,
    ],
    cta: { label: `Read self-host docs`, href: `/docs/self-host/` },
  },
  {
    id: `enterprise`,
    name: `Enterprise`,
    amount: `Custom`,
    cadence: `self-hosted, supported`,
    tagline: `For teams of 10 or more running it in-house.`,
    homeTagline: `For teams of 10 or more running it in-house.`,
    enterprise: true,
    features: [
      `Everything in self-hosted`,
      `Prioritized support`,
      `Deployment & upgrade help`,
    ],
    /* Dedicated contact page with the sales form (EXP-39). */
    cta: { label: `Contact sales`, href: `/contact/` },
  },
]
