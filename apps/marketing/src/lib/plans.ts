/* ─── Canonical plan data — the ONE marketing-side source of truth ───
   Consumed by PlanCards.tsx (the full cards, rendered on BOTH the home
   pricing section and /pricing/ — EXP-207), PricingPage.tsx (footnote +
   licensing section) and seo.ts (JSON-LD offers derive from priceNumber, so
   the self-host card stays out automatically).

   Values mirror apps/web/src/lib/billing.ts PLAN_LIMITS and the in-app grid
   in apps/web/src/components/team/plan-comparison.tsx — keep the three in
   sync when prices, limits or the canonical bullets change (EXP-176 unified
   them; everything never-gated lives in the shared EVERY_PLAN_INCLUDES
   sentence). EXP-286 collapsed the cloud grid to two cards (Free + Team) —
   Enterprise is a text line, not a card. EXP-338 moved the Self-hosted card
   INTO the main grid (Free · Team · Self-hosted, home and /pricing alike)
   and made push notifications an explicit CLOUD bullet: the store mobile
   apps are compiled against the first-party Firebase project, so a
   self-hosted instance cannot push to them — the one capability the
   self-host card does not get. */
import { LINKS } from "./links"

export type Plan = {
  id: `free` | `team` | `selfhost`
  name: string
  amount: string
  /* Set only for self-serve cloud tiers — drives the schema.org Offer list. */
  priceNumber?: number
  priceDescription?: string
  cadence?: string
  note?: string
  /* When set, the card renders a monthly/yearly billing toggle (EXP-341) and
     `amount` above is the YEARLY per-seat price — the default, toggle-on
     state. Flipping the toggle off swaps in this monthly price. */
  monthlyAmount?: string
  tagline: string
  highlight?: boolean
  selfHost?: boolean
  features: string[]
  /* Rendered after the feature list with a warning glyph instead of a check —
     an honest technical limitation, not a restriction (EXP-352: the self-host
     card's "no mobile push"). */
  caveat?: string
  cta: { label: string; href: string }
}

/* Back-compat alias — some call sites still name the type CloudPlan. */
export type CloudPlan = Plan

/* Shown under every plan grid — marketing home, /pricing and the in-app
   comparison carry the same sentence verbatim. (Push moved out of this
   sentence and into the cloud cards' bullets — EXP-338: the self-host card
   in the same grid can't claim it.) */
export const EVERY_PLAN_INCLUDES = `Every plan includes unlimited boards, repos and coding sessions, all native apps, real-time sync, and email & remote steer.`

/* Enterprise stopped being a pricing card (EXP-286) — it is a sales
   motion, rendered as one line under the plan grid. Since EXP-352 the
   product it sells is Enterprise Support, an optional add-on — the
   Apache-2.0 license removed every mandatory contract. */
export const ENTERPRISE_LINE = `Need SSO, SLA or DPA?`

/* The main grid: Free · Team · Self-hosted (EXP-338). */
export const PLANS: Plan[] = [
  {
    id: `free`,
    name: `Free`,
    amount: `€0`,
    priceNumber: 0,
    cadence: `forever`,
    tagline: `For you and two teammates.`,
    features: [
      `3 seats`,
      `250 MB attachment storage`,
      `1 feedback widget`,
      `Mobile push notifications`,
    ],
    cta: { label: `Sign up free`, href: LINKS.app.login },
  },
  {
    id: `team`,
    name: `Team`,
    amount: `€12`,
    priceNumber: 12,
    priceDescription: `Per seat, per month, billed yearly. €15 billed monthly.`,
    cadence: `/seat/mo`,
    /* The cadence toggle replaces the old `billed yearly · €15 monthly`
       note — it says the same thing and lets you see the monthly price. */
    monthlyAmount: `€15`,
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
  {
    id: `selfhost`,
    name: `Self-hosted`,
    amount: `Free`,
    cadence: `your hardware`,
    tagline: `Free forever, open source under Apache-2.0.`,
    selfHost: true,
    features: [
      `Free forever, unlimited users`,
      `No plan limits`,
      `Unlimited storage`,
      `One docker compose`,
      `Open source (Apache-2.0)`,
    ],
    caveat: `No mobile push`,
    cta: { label: `Read self-host docs`, href: `/docs/self-host/` },
  },
]

/* Enterprise Support is an OPTIONAL add-on for self-hosters (EXP-352: the
   Apache-2.0 switch deleted the mandatory 10+-people commercial license).
   EXP-218: its pricing is NOT published — self-hosting reads as free, full
   stop, and support is a conversation. Never reintroduce price points for
   it here or on any marketing page; route people to /contact/. */
