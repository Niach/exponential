import { and, eq, sql } from "drizzle-orm"
import { db } from "@/db/connection"
import { boards, widgetConfigs, teamMembers, teams } from "@/db/schema"
import { users } from "@/db/auth-schema"
import { emailEnabled } from "@/lib/email-enabled"
import { invalidateMembershipCaches } from "@/lib/auth/membership-cache"
import { generateWidgetKey } from "@/lib/widget/key"
// Vite's ?raw suffix inlines file contents as a string at build time. We
// do this so the server bundle ships the SQL alongside the JS, no fs reads
// required at runtime (which Vite also can't tree-shake for browser builds).
import triggersSql from "@/db/out/custom/0001_triggers.sql?raw"

const FEEDBACK_TEAM_SLUG = `feedback`
const FEEDBACK_TEAM_NAME = `Exponential Feedback`
// The feedback team's historical dogfood board slug. EXP-363 removed board
// seeding entirely — the bootstrap no longer creates, protects, or repo-backs
// any board. The slug remains only so ensureFeedbackWidgetConfig can target
// an existing board when first creating the widget config.
const DOGFOOD_BOARD_SLUG = `exponential`

function parseAdminEmails(): string[] {
  const raw = process.env.INITIAL_ADMIN_EMAILS
  if (!raw) return []
  return raw
    .split(`,`)
    .map((email) => email.trim().toLowerCase())
    .filter(Boolean)
}

// The shared dogfood/feedback team (widget submissions + support land here).
// Membership is bootstrap-managed owners (admins) plus regular invites — the
// migration purged the old self-joined members once; no recurring purge here,
// because invited triagers are legitimate members now.
async function ensureFeedbackTeam() {
  const [existing] = await db
    .select({ id: teams.id })
    .from(teams)
    .where(eq(teams.slug, FEEDBACK_TEAM_SLUG))
    .limit(1)
  if (existing) return existing.id

  const [team] = await db
    .insert(teams)
    .values({
      name: FEEDBACK_TEAM_NAME,
      slug: FEEDBACK_TEAM_SLUG,
    })
    .returning({ id: teams.id })
  return team.id
}

// Cached id of the bootstrap feedback team (cloud-only; null on
// self-hosted instances, which have no bootstrap board). This replaces the old
// `teams.isPublic` column as the "shared infra team" marker used by
// personal-team resolution, onboarding evidence, billing team
// counts, and the delete guards.
let feedbackTeamIdPromise: Promise<string | null> | null = null

export function getFeedbackTeamId(): Promise<string | null> {
  if (!isCloudInstance()) return Promise.resolve(null)
  if (!feedbackTeamIdPromise) {
    feedbackTeamIdPromise = (async () => {
      const [row] = await db
        .select({ id: teams.id })
        .from(teams)
        .where(eq(teams.slug, FEEDBACK_TEAM_SLUG))
        .limit(1)
      const id = row?.id ?? null
      // A null just means the async bootstrap hasn't inserted the team
      // yet — don't memoize it, or every guard keyed on this id stays
      // disabled for the process lifetime.
      if (id === null) {
        feedbackTeamIdPromise = null
      }
      return id
    })().catch((err) => {
      feedbackTeamIdPromise = null
      throw err
    })
  }
  return feedbackTeamIdPromise
}

// EXP-363: nothing stamps `is_protected` anymore — the dogfood board behaves
// like any other board. Clear the flag wherever a previous deployment's
// bootstrap left it (only ever the feedback team). One-shot in effect: once
// cleared, nothing re-protects.
async function releaseProtectedBoards(publicTeamId: string) {
  await db
    .update(boards)
    .set({ isProtected: false })
    .where(
      and(eq(boards.teamId, publicTeamId), eq(boards.isProtected, true))
    )
}

// The dogfood helpdesk gate rides the normal plan machinery
// (assertCanUseHelpdesk is paid-only on cloud) — comp the feedback team to
// `team` via the existing admin comp floor instead of special-casing it
// in billing. One-shot (only when comp_tier IS NULL) so a deliberate admin
// change sticks. Side effect: Team limits (storage/widgets/seats) apply
// to the shared dogfood team — intended.
async function ensureFeedbackTeamComp(publicTeamId: string) {
  await db
    .update(teams)
    .set({ compTier: `team` })
    .where(and(eq(teams.id, publicTeamId), sql`${teams.compTier} IS NULL`))
}

// The dogfood team always offers support: force the team helpdesk
// flag on (mirrors the old per-board forcing — a deliberate admin disable
// would be undone next boot, which is intended for the shared dogfood
// team).
async function ensureTeamHelpdesk(publicTeamId: string) {
  await db
    .update(teams)
    .set({ helpdeskEnabled: true })
    .where(and(eq(teams.id, publicTeamId), eq(teams.helpdeskEnabled, false)))
}

const FEEDBACK_WIDGET_NAME = `Exponential App`

// The dogfood key's allowlist: this instance's own hostname (the widget
// mounts inside the app — a port-less pattern matches any port, so dev
// `localhost` covers both :3000 and :5173) plus the marketing site, which
// embeds the prod key from exponential.at (apps/marketing/src/lib/links.ts).
function dogfoodAllowedDomains(): string[] {
  const domains = [`exponential.at`, `www.exponential.at`]
  try {
    const host = new URL(process.env.BETTER_AUTH_URL ?? ``).hostname
    if (host && !domains.includes(host)) domains.unshift(host)
  } catch {
    // No/invalid BETTER_AUTH_URL — the marketing domains still apply.
  }
  return domains
}

// The dogfood widget: the Exponential web app itself embeds the feedback
// widget — feedback lands on the dogfood board, support tickets in the
// team support inbox. The key is domain-allowlisted like every widget
// (EXP-209 removed allow-all; an empty list blocks the key at serve time).
// Existing configs get two ONE-SHOT heals, each gated on the field proving
// it was never deliberately configured: modes (gated on `formConfig.modes`
// being ABSENT — the modes-aware settings UI always writes a modes array on
// save) and allowedDomains (gated on the list being EMPTY — the settings UI
// refuses to save an empty allowlist since EXP-209).
async function ensureFeedbackWidgetConfig(publicTeamId: string) {
  const [existing] = await db
    .select({
      id: widgetConfigs.id,
      formConfig: widgetConfigs.formConfig,
      allowedDomains: widgetConfigs.allowedDomains,
    })
    .from(widgetConfigs)
    .where(
      and(
        eq(widgetConfigs.teamId, publicTeamId),
        eq(widgetConfigs.name, FEEDBACK_WIDGET_NAME)
      )
    )
    .limit(1)
  if (existing) {
    const form = existing.formConfig ?? {}
    const heal = {
      ...(Array.isArray(form.modes)
        ? {}
        : { formConfig: { ...form, modes: [`feedback`, `support`] } }),
      ...(existing.allowedDomains.length === 0
        ? { allowedDomains: dogfoodAllowedDomains() }
        : {}),
    }
    if (Object.keys(heal).length === 0) return
    await db
      .update(widgetConfigs)
      .set(heal)
      .where(eq(widgetConfigs.id, existing.id))
    return
  }

  const [board] = await db
    .select({ id: boards.id })
    .from(boards)
    .where(
      and(eq(boards.teamId, publicTeamId), eq(boards.slug, DOGFOOD_BOARD_SLUG))
    )
    .limit(1)
  if (!board) return

  await db.insert(widgetConfigs).values({
    teamId: publicTeamId,
    boardId: board.id,
    name: FEEDBACK_WIDGET_NAME,
    publicKey: generateWidgetKey(),
    allowedDomains: dogfoodAllowedDomains(),
    formConfig: { modes: [`feedback`, `support`] },
  })
}

async function addAdminsAsPublicTeamOwners(publicTeamId: string) {
  const adminRows = await db
    .select({ id: users.id })
    .from(users)
    .where(eq(users.isAdmin, true))
  if (adminRows.length === 0) return
  for (const admin of adminRows) {
    await db
      .insert(teamMembers)
      .values({
        teamId: publicTeamId,
        userId: admin.id,
        role: `owner`,
      })
      .onConflictDoNothing()
  }
}

async function promoteInitialAdmins() {
  const emails = parseAdminEmails()
  if (emails.length === 0) return

  await db
    .update(users)
    .set({ isAdmin: true, updatedAt: new Date() })
    .where(
      and(
        sql`lower(${users.email}) IN (${sql.join(
          emails.map((email) => sql`${email}`),
          sql`, `
        )})`,
        // With open sign-up anyone can create a row for an admin email, so
        // promotion must wait for proven mailbox ownership. Skip the gate when
        // email flows are off (no way to ever verify on such instances).
        emailEnabled ? eq(users.emailVerified, true) : undefined
      )
    )
}

// Drizzle migrations don't run our hand-written triggers + partial unique
// index. Apply them on every boot — every statement is idempotent
// (CREATE OR REPLACE / CREATE … IF NOT EXISTS).
async function applyCustomSql() {
  for (const [name, content] of [[`0001_triggers.sql`, triggersSql]] as const) {
    if (!content) continue
    try {
      await db.execute(sql.raw(content))
    } catch (err) {
      // Triggers may already exist; surface but don't abort.
      console.warn(`[bootstrap-cloud] applying ${name} produced:`, err)
    }
  }
}

export function isCloudInstance(): boolean {
  return process.env.SELF_HOSTED !== `true`
}

let bootstrapPromise: Promise<void> | null = null

export function bootstrapCloud(): Promise<void> {
  if (bootstrapPromise) return bootstrapPromise
  bootstrapPromise = (async () => {
    try {
      await applyCustomSql()
      await promoteInitialAdmins()
      if (isCloudInstance()) {
        const publicTeamId = await ensureFeedbackTeam()
        await ensureFeedbackTeamComp(publicTeamId)
        await addAdminsAsPublicTeamOwners(publicTeamId)
        // No board seeding since EXP-363 — just clear leftover protection,
        // flip the team helpdesk on, and seed/heal the widget config.
        await releaseProtectedBoards(publicTeamId)
        await ensureTeamHelpdesk(publicTeamId)
        await ensureFeedbackWidgetConfig(publicTeamId)
      }
    } catch (err) {
      console.error(`[bootstrap-cloud] failed:`, err)
      bootstrapPromise = null
      throw err
    }
  })()
  return bootstrapPromise
}

// Promote a single user if their email matches the admin list. Used by Better
// Auth's user.create.after hook (so first-sign-in promotion doesn't need to
// wait for a server restart) and again from afterEmailVerification. Also adds
// the freshly-promoted admin as an owner of the bootstrap feedback team.
//
// When email flows are enabled, promotion requires a verified email: sign-up
// is open on the cloud and does not prove mailbox ownership, so an attacker
// could otherwise register an INITIAL_ADMIN_EMAILS address before its real
// owner and walk away with a global-admin session.
export async function maybePromoteNewUser(
  userId: string,
  email: string,
  emailVerified: boolean
) {
  if (emailEnabled && !emailVerified) return
  const emails = parseAdminEmails()
  if (emails.length === 0) return
  if (!emails.includes(email.toLowerCase())) return
  await db
    .update(users)
    .set({ isAdmin: true, updatedAt: new Date() })
    .where(eq(users.id, userId))

  const feedbackTeamId = await getFeedbackTeamId()
  if (feedbackTeamId) {
    await db
      .insert(teamMembers)
      .values({ teamId: feedbackTeamId, userId, role: `owner` })
      .onConflictDoNothing()
    // This runs post-boot (from afterEmailVerification) for possibly-warm
    // accounts, so the membership cache may hold the pre-insert set.
    // (addAdminsAsPublicTeamOwners above needs no invalidation — it runs
    // once at boot, before any traffic could populate the cache.)
    invalidateMembershipCaches()
  }
}
