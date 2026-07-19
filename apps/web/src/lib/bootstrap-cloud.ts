import { and, eq, inArray, notExists, sql } from "drizzle-orm"
import { db } from "@/db/connection"
import {
  githubInstallationLinks,
  githubInstallations,
  issues,
  projects,
  repositories,
  widgetConfigs,
  workspaceMembers,
  workspaces,
} from "@/db/schema"
import { users } from "@/db/auth-schema"
import { emailEnabled } from "@/lib/email"
import { generateWidgetKey } from "@/lib/widget/key"
import { createWidgetUser } from "@/lib/widget/widget-user"
// Vite's ?raw suffix inlines file contents as a string at build time. We
// do this so the server bundle ships the SQL alongside the JS, no fs reads
// required at runtime (which Vite also can't tree-shake for browser builds).
import triggersSql from "@/db/out/custom/0001_triggers.sql?raw"

const FEEDBACK_WORKSPACE_SLUG = `feedback`
const FEEDBACK_WORKSPACE_NAME = `Exponential Feedback`
// The feedback workspace's canonical dogfood project: "Exponential". Private
// like every project since EXP-180 (public boards are gone) — the widget is
// the only inbound feedback path. Feedback and dogfood coding share it.
const PUBLIC_PROJECT_SLUG = `exponential`
const PUBLIC_PROJECT_NAME = `Exponential`
const PUBLIC_PROJECT_PREFIX = `EXP`
// The canonical dogfood project is always backed by this repo (repos are
// OPTIONAL on projects). On this internal bootstrap path we upsert the
// registry row DIRECTLY, with no GitHub App validation — `installation_id`
// starts null; ensurePublicRepositoryInstallation backfills it (and the
// feedback workspace's installation link) from a live GitHub lookup on every
// boot, best-effort.
const PUBLIC_REPO_FULL_NAME = `Niach/exponential`
// Pre-collapse deployments seeded this second project next to the dogfood one;
// collapseLegacyFeedbackProject folds it away.
const LEGACY_FEEDBACK_PROJECT_SLUG = `feedback`
// The former dogfood helpdesk board (EXP-162, retired by EXP-180): tickets
// are standalone workspace-level threads now, so the Support project is just
// a normal board holding its historical ticket-issues.
// releaseLegacySupportProject un-protects it so admins can archive/trash it.
const LEGACY_SUPPORT_PROJECT_SLUG = `support`

function parseAdminEmails(): string[] {
  const raw = process.env.INITIAL_ADMIN_EMAILS
  if (!raw) return []
  return raw
    .split(`,`)
    .map((email) => email.trim().toLowerCase())
    .filter(Boolean)
}

// The (now private, v7) workspace hosting the public dogfood feedback board.
// Membership is bootstrap-managed owners (admins) plus regular invites — the
// migration purged the old self-joined members once; no recurring purge here,
// because invited triagers are legitimate members now.
async function ensureFeedbackWorkspace() {
  const [existing] = await db
    .select({ id: workspaces.id })
    .from(workspaces)
    .where(eq(workspaces.slug, FEEDBACK_WORKSPACE_SLUG))
    .limit(1)
  if (existing) return existing.id

  const [workspace] = await db
    .insert(workspaces)
    .values({
      name: FEEDBACK_WORKSPACE_NAME,
      slug: FEEDBACK_WORKSPACE_SLUG,
    })
    .returning({ id: workspaces.id })
  return workspace.id
}

// Cached id of the bootstrap feedback workspace (cloud-only; null on
// self-hosted instances, which have no bootstrap board). This replaces the old
// `workspaces.isPublic` column as the "shared infra workspace" marker used by
// personal-workspace resolution, onboarding evidence, billing workspace
// counts, and the delete guards.
let feedbackWorkspaceIdPromise: Promise<string | null> | null = null

export function getFeedbackWorkspaceId(): Promise<string | null> {
  if (!isCloudInstance()) return Promise.resolve(null)
  if (!feedbackWorkspaceIdPromise) {
    feedbackWorkspaceIdPromise = (async () => {
      const [row] = await db
        .select({ id: workspaces.id })
        .from(workspaces)
        .where(eq(workspaces.slug, FEEDBACK_WORKSPACE_SLUG))
        .limit(1)
      const id = row?.id ?? null
      // A null just means the async bootstrap hasn't inserted the workspace
      // yet — don't memoize it, or every guard keyed on this id stays
      // disabled for the process lifetime.
      if (id === null) {
        feedbackWorkspaceIdPromise = null
      }
      return id
    })().catch((err) => {
      feedbackWorkspaceIdPromise = null
      throw err
    })
  }
  return feedbackWorkspaceIdPromise
}

// The feedback workspace's single canonical `exponential` project — the
// private, protected, repo-backed dogfood board. Runs on every boot: the
// widget config targets it and collapseLegacyFeedbackProject folds the legacy
// project into it — all of which must work on already-bootstrapped DBs where
// the project was never seeded. Idempotent; returns the project id.
async function ensureDogfoodProject(
  publicWorkspaceId: string
): Promise<string> {
  const [existing] = await db
    .select({
      id: projects.id,
      isProtected: projects.isProtected,
    })
    .from(projects)
    .where(
      and(
        eq(projects.workspaceId, publicWorkspaceId),
        eq(projects.slug, PUBLIC_PROJECT_SLUG)
      )
    )
    .limit(1)
  if (existing) {
    // Idempotently stamp the non-deletable marker — this is what marks the
    // ops-restored prod row protected on first boot.
    if (!existing.isProtected) {
      await db
        .update(projects)
        .set({ isProtected: true })
        .where(eq(projects.id, existing.id))
    }
    return existing.id
  }

  // The dogfood board is repo-backed — upsert the dogfood repo first
  // (idempotent) and pass its id into project creation.
  const repositoryId = await ensurePublicRepository(publicWorkspaceId)

  const [project] = await db
    .insert(projects)
    .values({
      workspaceId: publicWorkspaceId,
      name: PUBLIC_PROJECT_NAME,
      slug: PUBLIC_PROJECT_SLUG,
      prefix: PUBLIC_PROJECT_PREFIX,
      isProtected: true,
      repositoryId,
    })
    .returning({ id: projects.id })
  return project.id
}

// Upsert the dogfood repositories row and return its id. Runs on the internal
// bootstrap path only — no GitHub App validation, `installation_id` left null
// (self-heal fills it). Idempotent via the (workspace_id, full_name) unique.
async function ensurePublicRepository(
  publicWorkspaceId: string
): Promise<string> {
  const [inserted] = await db
    .insert(repositories)
    .values({
      workspaceId: publicWorkspaceId,
      fullName: PUBLIC_REPO_FULL_NAME,
      installationId: null,
    })
    .onConflictDoNothing({
      target: [repositories.workspaceId, repositories.fullName],
    })
    .returning({ id: repositories.id })
  if (inserted) return inserted.id

  const [row] = await db
    .select({ id: repositories.id })
    .from(repositories)
    .where(
      and(
        eq(repositories.workspaceId, publicWorkspaceId),
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
// to the feedback workspace so its pickers/token mints pass the workspace
// link-gate. Idempotent; a GitHub outage or unconfigured App just logs and
// retries next boot.
async function ensurePublicRepositoryInstallation(publicWorkspaceId: string) {
  const {
    githubAppConfigured,
    getInstallation,
    installationIdForRepo,
  } = await import(`@/lib/integrations/github-app`)
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
          eq(repositories.workspaceId, publicWorkspaceId),
          eq(repositories.fullName, PUBLIC_REPO_FULL_NAME),
          sql`${repositories.installationId} IS DISTINCT FROM ${installationId}`
        )
      )
    await db
      .insert(githubInstallationLinks)
      .values({
        workspaceId: publicWorkspaceId,
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
// (assertCanUseHelpdesk is Pro+ on cloud) — comp the feedback workspace to
// `business` via the existing admin comp floor instead of special-casing it
// in billing. One-shot (only when comp_tier IS NULL) so a deliberate admin
// change sticks. Side effect: business limits (storage/widgets/seats) apply
// to the shared dogfood workspace — intended.
async function ensureFeedbackWorkspaceComp(publicWorkspaceId: string) {
  await db
    .update(workspaces)
    .set({ compTier: `business` })
    .where(
      and(
        eq(workspaces.id, publicWorkspaceId),
        sql`${workspaces.compTier} IS NULL`
      )
    )
}

// The dogfood workspace always offers support: force the workspace helpdesk
// flag on (mirrors the old per-project forcing — a deliberate admin disable
// would be undone next boot, which is intended for the shared dogfood
// workspace).
async function ensureWorkspaceHelpdesk(publicWorkspaceId: string) {
  await db
    .update(workspaces)
    .set({ helpdeskEnabled: true })
    .where(
      and(
        eq(workspaces.id, publicWorkspaceId),
        eq(workspaces.helpdeskEnabled, false)
      )
    )
}

// EXP-180 retired the dedicated Support project (tickets are standalone
// workspace-level threads; the migration converted the old issue-anchored
// ones). Its historical ticket-issues stay as normal issues — just clear the
// bootstrap protection so admins can archive or trash the board at their own
// pace. One-shot in effect: once cleared, nothing re-protects it.
async function releaseLegacySupportProject(publicWorkspaceId: string) {
  await db
    .update(projects)
    .set({ isProtected: false })
    .where(
      and(
        eq(projects.workspaceId, publicWorkspaceId),
        eq(projects.slug, LEGACY_SUPPORT_PROJECT_SLUG),
        eq(projects.isProtected, true)
      )
    )
}

const FEEDBACK_WIDGET_NAME = `Exponential App`

// The dogfood widget: the Exponential web app itself embeds the feedback
// widget — feedback lands on the dogfood board, support tickets in the
// workspace support inbox. Domains stay open (allow-all) on purpose —
// self-hosted instances with arbitrary hostnames load this same cloud widget,
// and the widget is the ONLY anonymous write path; rate limiting is the abuse
// control. Existing configs get a ONE-SHOT modes heal, gated on
// `formConfig.modes` being ABSENT: the modes-aware settings UI always writes
// a modes array on save, so its absence proves the config was never
// deliberately configured.
async function ensureFeedbackWidgetConfig(publicWorkspaceId: string) {
  const [existing] = await db
    .select({
      id: widgetConfigs.id,
      formConfig: widgetConfigs.formConfig,
    })
    .from(widgetConfigs)
    .where(
      and(
        eq(widgetConfigs.workspaceId, publicWorkspaceId),
        eq(widgetConfigs.name, FEEDBACK_WIDGET_NAME)
      )
    )
    .limit(1)
  if (existing) {
    const form = existing.formConfig ?? {}
    if (Array.isArray(form.modes)) return
    await db
      .update(widgetConfigs)
      .set({ formConfig: { ...form, modes: [`feedback`, `support`] } })
      .where(eq(widgetConfigs.id, existing.id))
    return
  }

  const [project] = await db
    .select({ id: projects.id })
    .from(projects)
    .where(
      and(
        eq(projects.workspaceId, publicWorkspaceId),
        eq(projects.slug, PUBLIC_PROJECT_SLUG)
      )
    )
    .limit(1)
  if (!project) return

  await db.transaction(async (tx) => {
    const widgetUserId = await createWidgetUser(tx, {
      workspaceId: publicWorkspaceId,
      configName: FEEDBACK_WIDGET_NAME,
    })
    await tx.insert(widgetConfigs).values({
      workspaceId: publicWorkspaceId,
      projectId: project.id,
      name: FEEDBACK_WIDGET_NAME,
      publicKey: generateWidgetKey(),
      allowedDomains: [],
      formConfig: { modes: [`feedback`, `support`] },
      widgetUserId,
    })
  })
}

// Pre-collapse cloud deployments carried TWO projects in the public workspace:
// the seeded `feedback` project (the widget's target) and the DOGFOOD_REPO-
// gated `exponential` project. Fold the legacy one away on already-bootstrapped
// DBs. Order matters: repoint widget configs FIRST (widget_configs.project_id
// cascades on project delete, and the live widget may still be filing into the
// legacy project), then drop the legacy project only while it holds zero
// issues (issues cascade — never delete user data; the NOT EXISTS guard keeps
// the emptiness check and the delete in one atomic statement). A non-empty
// legacy project just stays until it's triaged empty — next boot retries.
async function collapseLegacyFeedbackProject(publicWorkspaceId: string) {
  const rows = await db
    .select({ id: projects.id, slug: projects.slug })
    .from(projects)
    .where(
      and(
        eq(projects.workspaceId, publicWorkspaceId),
        inArray(projects.slug, [
          LEGACY_FEEDBACK_PROJECT_SLUG,
          PUBLIC_PROJECT_SLUG,
        ])
      )
    )
  const legacy = rows.find((r) => r.slug === LEGACY_FEEDBACK_PROJECT_SLUG)
  const canonical = rows.find((r) => r.slug === PUBLIC_PROJECT_SLUG)
  if (!legacy || !canonical) return

  await db
    .update(widgetConfigs)
    .set({ projectId: canonical.id })
    .where(eq(widgetConfigs.projectId, legacy.id))

  await db
    .delete(projects)
    .where(
      and(
        eq(projects.id, legacy.id),
        notExists(
          db
            .select({ id: issues.id })
            .from(issues)
            .where(eq(issues.projectId, legacy.id))
        )
      )
    )
}

async function addAdminsAsPublicWorkspaceOwners(publicWorkspaceId: string) {
  const adminRows = await db
    .select({ id: users.id })
    .from(users)
    .where(eq(users.isAdmin, true))
  if (adminRows.length === 0) return
  for (const admin of adminRows) {
    await db
      .insert(workspaceMembers)
      .values({
        workspaceId: publicWorkspaceId,
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
  for (const [name, content] of [
    [`0001_triggers.sql`, triggersSql],
  ] as const) {
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
        const publicWorkspaceId = await ensureFeedbackWorkspace()
        await ensureFeedbackWorkspaceComp(publicWorkspaceId)
        await addAdminsAsPublicWorkspaceOwners(publicWorkspaceId)
        // Canonical project first — it upserts its backing repository row
        // inline (the dogfood board is repo-backed). Then fold the legacy
        // feedback project into the canonical one, flip the workspace
        // helpdesk on, release the retired Support project, and seed the
        // widget config (feedback target = the Exponential board).
        await ensureDogfoodProject(publicWorkspaceId)
        await ensurePublicRepositoryInstallation(publicWorkspaceId)
        await collapseLegacyFeedbackProject(publicWorkspaceId)
        await ensureWorkspaceHelpdesk(publicWorkspaceId)
        await releaseLegacySupportProject(publicWorkspaceId)
        await ensureFeedbackWidgetConfig(publicWorkspaceId)
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
// the freshly-promoted admin as an owner of the bootstrap feedback workspace.
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

  const feedbackWorkspaceId = await getFeedbackWorkspaceId()
  if (feedbackWorkspaceId) {
    await db
      .insert(workspaceMembers)
      .values({ workspaceId: feedbackWorkspaceId, userId, role: `owner` })
      .onConflictDoNothing()
  }
}

// Useful for tests / admin tools.
export async function listInitialAdminEmails() {
  return parseAdminEmails()
}
