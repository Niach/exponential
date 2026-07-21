import { and, eq, inArray, notExists, sql } from "drizzle-orm"
import { db } from "@/db/connection"
import {
  githubInstallationLinks,
  githubInstallations,
  issues,
  boards,
  repositories,
  widgetConfigs,
  teamMembers,
  teams,
} from "@/db/schema"
import { users } from "@/db/auth-schema"
import { emailEnabled } from "@/lib/email"
import { invalidateMembershipCaches } from "@/lib/auth/membership-cache"
import { generateWidgetKey } from "@/lib/widget/key"
// Vite's ?raw suffix inlines file contents as a string at build time. We
// do this so the server bundle ships the SQL alongside the JS, no fs reads
// required at runtime (which Vite also can't tree-shake for browser builds).
import triggersSql from "@/db/out/custom/0001_triggers.sql?raw"

const FEEDBACK_TEAM_SLUG = `feedback`
const FEEDBACK_TEAM_NAME = `Exponential Feedback`
// The feedback team's canonical dogfood board: "Exponential". Private
// like every board since EXP-180 (public boards are gone) — the widget is
// the only inbound feedback path. Feedback and dogfood coding share it.
const DOGFOOD_BOARD_SLUG = `exponential`
const DOGFOOD_BOARD_NAME = `Exponential`
const DOGFOOD_BOARD_PREFIX = `EXP`
// The canonical dogfood board is always backed by this repo (repos are
// OPTIONAL on boards). On this internal bootstrap path we upsert the
// registry row DIRECTLY, with no GitHub App validation — `installation_id`
// starts null; ensurePublicRepositoryInstallation backfills it (and the
// feedback team's installation link) from a live GitHub lookup on every
// boot, best-effort.
const PUBLIC_REPO_FULL_NAME = `Niach/exponential`
// Pre-collapse deployments seeded this second board next to the dogfood one;
// collapseLegacyFeedbackBoard folds it away.
const LEGACY_FEEDBACK_BOARD_SLUG = `feedback`
// The former dogfood helpdesk board (EXP-162, retired by EXP-180): tickets
// are standalone team-level threads now, so the Support board is just
// a normal board holding its historical ticket-issues.
// releaseLegacySupportBoard un-protects it so admins can archive/trash it.
const LEGACY_SUPPORT_BOARD_SLUG = `support`

function parseAdminEmails(): string[] {
  const raw = process.env.INITIAL_ADMIN_EMAILS
  if (!raw) return []
  return raw
    .split(`,`)
    .map((email) => email.trim().toLowerCase())
    .filter(Boolean)
}

// The (now private, v7) team hosting the public dogfood feedback board.
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

// The feedback team's single canonical `exponential` board — the
// private, protected, repo-backed dogfood board. Runs on every boot: the
// widget config targets it and collapseLegacyFeedbackBoard folds the legacy
// board into it — all of which must work on already-bootstrapped DBs where
// the board was never seeded. Idempotent; returns the board id.
async function ensureDogfoodBoard(publicTeamId: string): Promise<string> {
  const [existing] = await db
    .select({
      id: boards.id,
      isProtected: boards.isProtected,
    })
    .from(boards)
    .where(
      and(eq(boards.teamId, publicTeamId), eq(boards.slug, DOGFOOD_BOARD_SLUG))
    )
    .limit(1)
  if (existing) {
    // Idempotently stamp the non-deletable marker — this is what marks the
    // ops-restored prod row protected on first boot.
    if (!existing.isProtected) {
      await db
        .update(boards)
        .set({ isProtected: true })
        .where(eq(boards.id, existing.id))
    }
    return existing.id
  }

  // The dogfood board is repo-backed — upsert the dogfood repo first
  // (idempotent) and pass its id into board creation.
  const repositoryId = await ensurePublicRepository(publicTeamId)

  const [board] = await db
    .insert(boards)
    .values({
      teamId: publicTeamId,
      name: DOGFOOD_BOARD_NAME,
      slug: DOGFOOD_BOARD_SLUG,
      prefix: DOGFOOD_BOARD_PREFIX,
      isProtected: true,
      repositoryId,
    })
    .returning({ id: boards.id })
  return board.id
}

// Upsert the dogfood repositories row and return its id. Runs on the internal
// bootstrap path only — no GitHub App validation, `installation_id` left null
// (self-heal fills it). Idempotent via the (team_id, full_name) unique.
async function ensurePublicRepository(publicTeamId: string): Promise<string> {
  const [inserted] = await db
    .insert(repositories)
    .values({
      teamId: publicTeamId,
      fullName: PUBLIC_REPO_FULL_NAME,
      installationId: null,
    })
    .onConflictDoNothing({
      target: [repositories.teamId, repositories.fullName],
    })
    .returning({ id: repositories.id })
  if (inserted) return inserted.id

  const [row] = await db
    .select({ id: repositories.id })
    .from(repositories)
    .where(
      and(
        eq(repositories.teamId, publicTeamId),
        eq(repositories.fullName, PUBLIC_REPO_FULL_NAME)
      )
    )
    .limit(1)
  // A concurrent delete between the onConflictDoNothing INSERT and this SELECT
  // leaves no row to read — throw rather than dereferencing undefined; the next
  // boot re-runs bootstrap idempotently.
  if (!row) {
    throw new Error(
      `Dogfood repository row vanished concurrently during bootstrap — retry.`
    )
  }
  return row.id
}

// Best-effort self-heal for the dogfood repo's GitHub wiring: resolve the
// repo's installation live (App JWT), mirror the installation row, backfill
// the bootstrap repo row's null `installation_id`, and link the installation
// to the feedback team so its pickers/token mints pass the team
// link-gate. Idempotent; a GitHub outage or unconfigured App just logs and
// retries next boot.
async function ensurePublicRepositoryInstallation(publicTeamId: string) {
  const { githubAppConfigured, getInstallation, installationIdForRepo } =
    await import(`@/lib/integrations/github-app`)
  if (!githubAppConfigured()) return
  try {
    const installationId = await installationIdForRepo(PUBLIC_REPO_FULL_NAME)
    if (installationId == null) {
      console.warn(
        `[bootstrap-cloud] GitHub App not installed on ${PUBLIC_REPO_FULL_NAME} — dogfood repo left unlinked`
      )
      return
    }
    const info = await getInstallation(installationId)
    const [installationRow] = await db
      .insert(githubInstallations)
      .values({
        installationId,
        accountLogin: info?.account ?? null,
        accountType: info?.accountType ?? null,
      })
      .onConflictDoUpdate({
        target: githubInstallations.installationId,
        set: {
          accountLogin: info?.account ?? null,
          accountType: info?.accountType ?? null,
          updatedAt: new Date(),
        },
      })
      .returning({ id: githubInstallations.id })
    if (!installationRow) return

    await db
      .update(repositories)
      .set({ installationId })
      .where(
        and(
          eq(repositories.teamId, publicTeamId),
          eq(repositories.fullName, PUBLIC_REPO_FULL_NAME),
          sql`${repositories.installationId} IS DISTINCT FROM ${installationId}`
        )
      )
    await db
      .insert(githubInstallationLinks)
      .values({
        teamId: publicTeamId,
        githubInstallationId: installationRow.id,
      })
      .onConflictDoNothing()
  } catch (err) {
    console.warn(
      `[bootstrap-cloud] dogfood repo installation self-heal failed:`,
      err
    )
  }
}

// The dogfood helpdesk gate rides the normal plan machinery
// (assertCanUseHelpdesk is Pro+ on cloud) — comp the feedback team to
// `business` via the existing admin comp floor instead of special-casing it
// in billing. One-shot (only when comp_tier IS NULL) so a deliberate admin
// change sticks. Side effect: business limits (storage/widgets/seats) apply
// to the shared dogfood team — intended.
async function ensureFeedbackTeamComp(publicTeamId: string) {
  await db
    .update(teams)
    .set({ compTier: `business` })
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

// EXP-180 retired the dedicated Support board (tickets are standalone
// team-level threads; the migration converted the old issue-anchored
// ones). Its historical ticket-issues stay as normal issues — just clear the
// bootstrap protection so admins can archive or trash the board at their own
// pace. One-shot in effect: once cleared, nothing re-protects it.
async function releaseLegacySupportBoard(publicTeamId: string) {
  await db
    .update(boards)
    .set({ isProtected: false })
    .where(
      and(
        eq(boards.teamId, publicTeamId),
        eq(boards.slug, LEGACY_SUPPORT_BOARD_SLUG),
        eq(boards.isProtected, true)
      )
    )
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

// Pre-collapse cloud deployments carried TWO boards in the public team:
// the seeded `feedback` board (the widget's target) and the DOGFOOD_REPO-
// gated `exponential` board. Fold the legacy one away on already-bootstrapped
// DBs. Order matters: repoint widget configs FIRST (widget_configs.board_id
// cascades on board delete, and the live widget may still be filing into the
// legacy board), then drop the legacy board only while it holds zero
// issues (issues cascade — never delete user data; the NOT EXISTS guard keeps
// the emptiness check and the delete in one atomic statement). A non-empty
// legacy board just stays until it's triaged empty — next boot retries.
async function collapseLegacyFeedbackBoard(publicTeamId: string) {
  const rows = await db
    .select({ id: boards.id, slug: boards.slug })
    .from(boards)
    .where(
      and(
        eq(boards.teamId, publicTeamId),
        inArray(boards.slug, [LEGACY_FEEDBACK_BOARD_SLUG, DOGFOOD_BOARD_SLUG])
      )
    )
  const legacy = rows.find((r) => r.slug === LEGACY_FEEDBACK_BOARD_SLUG)
  const canonical = rows.find((r) => r.slug === DOGFOOD_BOARD_SLUG)
  if (!legacy || !canonical) return

  await db
    .update(widgetConfigs)
    .set({ boardId: canonical.id })
    .where(eq(widgetConfigs.boardId, legacy.id))

  await db
    .delete(boards)
    .where(
      and(
        eq(boards.id, legacy.id),
        notExists(
          db
            .select({ id: issues.id })
            .from(issues)
            .where(eq(issues.boardId, legacy.id))
        )
      )
    )
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
        // Canonical board first — it upserts its backing repository row
        // inline (the dogfood board is repo-backed). Then fold the legacy
        // feedback board into the canonical one, flip the team
        // helpdesk on, release the retired Support board, and seed the
        // widget config (feedback target = the Exponential board).
        await ensureDogfoodBoard(publicTeamId)
        await ensurePublicRepositoryInstallation(publicTeamId)
        await collapseLegacyFeedbackBoard(publicTeamId)
        await ensureTeamHelpdesk(publicTeamId)
        await releaseLegacySupportBoard(publicTeamId)
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
