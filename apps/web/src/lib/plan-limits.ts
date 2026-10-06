// The plan table, client-safe (no db import): `billing.ts` enforces it on the
// server, and copy that quotes the Free plan (the cancel-subscription prompt)
// reads the same numbers instead of restating them.

export type PlanTier = `free` | `team` | `unlimited`

// Per-seat model (EXP-286 rebrand). The ONLY monetized axes are
// seats (team size), storage per team, and feedback-widget configs.
// Boards, repositories, and coding-session capacity are unlimited on every
// tier. Push + email notification delivery and remote steer are FREE on every
// tier and are never plan-gated — do NOT add booleans for them here.
export type PlanLimits = {
  // Purchased seats a team may fill with members. Free = 3;
  // the paid tier overrides this placeholder with the subscription's purchased
  // quantity (see planFromSubscription).
  seats: number
  // Attachment storage budget per team, in megabytes.
  storageMb: number
  // Feedback-widget configs a team may create. Free = 1 (EXP-180),
  // Team = unlimited.
  widgetConfigs: number
  // Widget submissions per hour, aggregated per TEAM (the billing boundary —
  // paid teams have unlimited widget configs, so a per-key ceiling would be
  // trivially bypassed). Enforced only by the widget submit path on cloud
  // (lib/widget/submit-limit.ts); the per-IP abuse bucket stays global and
  // plan-independent.
  widgetSubmissionsPerHour: number
}

// NOTE: the `seats` value on the paid tier is only a placeholder — the real
// seat allowance is the subscription's purchased quantity, applied in
// planFromSubscription. Free stays at a hard 3 (enough to experience the
// realtime-collaboration core single-team, EXP-286).
export const PLAN_LIMITS: Record<PlanTier, PlanLimits> = {
  free: {
    seats: 3,
    storageMb: 250,
    widgetConfigs: 1,
    widgetSubmissionsPerHour: 60,
  },
  team: {
    seats: 1,
    storageMb: 10240,
    widgetConfigs: Infinity,
    widgetSubmissionsPerHour: Infinity,
  },
  unlimited: {
    seats: Infinity,
    storageMb: Infinity,
    widgetConfigs: Infinity,
    widgetSubmissionsPerHour: Infinity,
  },
}
