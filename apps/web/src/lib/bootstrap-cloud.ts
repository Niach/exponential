import { and, eq, inArray, notExists, sql } from "drizzle-orm"
import { db } from "@/db/connection"
import {
  issues,
  projectRepositories,
  projects,
  repositories,
  widgetConfigs,
  workspaceMembers,
  workspaces,
} from "@/db/schema"
import type { ProjectPreviewMirror } from "@exp/db-schema/domain"
import { users } from "@/db/auth-schema"
import { invalidatePublicWorkspaceCache } from "@/lib/workspace-membership"
import { emailEnabled } from "@/lib/email"
import { generateWidgetKey } from "@/lib/widget/key"
import { createWidgetUser } from "@/lib/widget/widget-user"
// Vite's ?raw suffix inlines file contents as a string at build time. We
// do this so the server bundle ships the SQL alongside the JS, no fs reads
// required at runtime (which Vite also can't tree-shake for browser builds).
import triggersSql from "@/db/out/custom/0001_triggers.sql?raw"

const PUBLIC_WORKSPACE_SLUG = `feedback`
const PUBLIC_WORKSPACE_NAME = `Exponential Feedback`
// The public workspace holds exactly ONE project: the dogfood "Exponential"
// project. Feedback (widget + /feedback route) and dogfood coding share it —
// a separate `feedback` project only split the same triage board in two.
const PUBLIC_PROJECT_SLUG = `exponential`
const PUBLIC_PROJECT_NAME = `Exponential`
const PUBLIC_PROJECT_PREFIX = `EXP`
// Pre-collapse deployments seeded this second project next to the dogfood one;
// collapseLegacyFeedbackProject folds it away.
const LEGACY_FEEDBACK_PROJECT_SLUG = `feedback`

function parseAdminEmails(): string[] {
  const raw = process.env.INITIAL_ADMIN_EMAILS
  if (!raw) return []
  return raw
    .split(`,`)
    .map((email) => email.trim().toLowerCase())
    .filter(Boolean)
}

async function ensurePublicWorkspace() {
  const [existing] = await db
    .select({ id: workspaces.id })
    .from(workspaces)
    .where(eq(workspaces.slug, PUBLIC_WORKSPACE_SLUG))
    .limit(1)

  let workspaceId: string
  if (existing) {
    workspaceId = existing.id
  } else {
    const [workspace] = await db
      .insert(workspaces)
      .values({
        name: PUBLIC_WORKSPACE_NAME,
        slug: PUBLIC_WORKSPACE_SLUG,
        isPublic: true,
        publicWritePolicy: `everyone`,
      })
      .returning({ id: workspaces.id })

    invalidatePublicWorkspaceCache()
    workspaceId = workspace.id
  }

  // Idempotently align the Feedback workspace's flags with the intended state
  // even if it predates the publicWritePolicy column (or was migrated with the
  // default 'members'). Public + everyone is the feedback workspace's contract.
  await db
    .update(workspaces)
    .set({ isPublic: true, publicWritePolicy: `everyone` })
    .where(eq(workspaces.id, workspaceId))

  return workspaceId
}

// The public workspace's single canonical `exponential` project. Runs on
// every boot and is deliberately INDEPENDENT of DOGFOOD_REPO: the /feedback
// route redirects to this slug unconditionally, the widget config targets it,
// and collapseLegacyFeedbackProject folds the legacy project into it — all of
// which must work on already-bootstrapped pre-collapse DBs where the project
// was never seeded (it used to be created only alongside a fresh workspace or
// behind DOGFOOD_REPO). Idempotent; returns the project id.
async function ensurePublicProject(publicWorkspaceId: string): Promise<string> {
  const [existing] = await db
    .select({ id: projects.id })
    .from(projects)
    .where(
      and(
        eq(projects.workspaceId, publicWorkspaceId),
        eq(projects.slug, PUBLIC_PROJECT_SLUG)
      )
    )
    .limit(1)
  if (existing) return existing.id

  const [project] = await db
    .insert(projects)
    .values({
      workspaceId: publicWorkspaceId,
      name: PUBLIC_PROJECT_NAME,
      slug: PUBLIC_PROJECT_SLUG,
      prefix: PUBLIC_PROJECT_PREFIX,
    })
    .returning({ id: projects.id })
  return project.id
}

const FEEDBACK_WIDGET_NAME = `Exponential App`

// The dogfood widget: the Exponential web app itself embeds the feedback
// widget pointed at the public feedback workspace. Domains stay open
// (allow-all) on purpose — self-hosted instances with arbitrary hostnames
// load this same cloud widget, and the workspace is already
// publicWritePolicy=everyone; rate limiting is the abuse control.
async function ensureFeedbackWidgetConfig(publicWorkspaceId: string) {
  const [existing] = await db
    .select({ id: widgetConfigs.id })
    .from(widgetConfigs)
    .where(
      and(
        eq(widgetConfigs.workspaceId, publicWorkspaceId),
        eq(widgetConfigs.name, FEEDBACK_WIDGET_NAME)
      )
    )
    .limit(1)
  if (existing) return

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
      widgetUserId,
    })
  })
}

// Display mirror seeded for the dogfood project. The canonical build/run shell
// commands live in the repo's committed `.exponential/config.json` (read only
// from the cloned working tree by the desktop apps) — this mirror is the safe
// metadata the web settings UI + pre-clone discovery read, and matches the
// targets that file declares. Feedback routes back into this same project.
const DOGFOOD_TARGETS: ProjectPreviewMirror[`targets`] = [
  { id: `web`, name: `Web`, platform: `web` },
  { id: `android`, name: `Android`, platform: `android` },
  { id: `ios-staging`, name: `iOS Staging`, platform: `ios` },
  { id: `ios-prod`, name: `iOS Prod`, platform: `ios` },
]

// Upsert a `repositories` row for the dogfood repo and make it the dogfood
// project's PRIMARY link, so the coding launcher can resolve a clone target and
// dogfood coding works end-to-end. Replaces the removed `projects.githubRepo`
// wiring. Idempotent.
async function ensureDogfoodRepoLink(
  workspaceId: string,
  projectId: string,
  repoFullName: string
) {
  const [inserted] = await db
    .insert(repositories)
    .values({
      workspaceId,
      fullName: repoFullName,
      defaultBranch: `main`,
      private: false,
    })
    .onConflictDoNothing({
      target: [repositories.workspaceId, repositories.fullName],
    })
    .returning({ id: repositories.id })

  let repositoryId = inserted?.id
  if (!repositoryId) {
    const [row] = await db
      .select({ id: repositories.id })
      .from(repositories)
      .where(
        and(
          eq(repositories.workspaceId, workspaceId),
          eq(repositories.fullName, repoFullName)
        )
      )
      .limit(1)
    repositoryId = row?.id
  }
  if (!repositoryId) return

  await db.transaction(async (tx) => {
    // Enforce one primary per project (partial unique index).
    await tx
      .update(projectRepositories)
      .set({ isPrimary: false })
      .where(
        and(
          eq(projectRepositories.projectId, projectId),
          eq(projectRepositories.isPrimary, true)
        )
      )
    await tx
      .insert(projectRepositories)
      .values({ projectId, repositoryId, isPrimary: true })
      .onConflictDoUpdate({
        target: [
          projectRepositories.projectId,
          projectRepositories.repositoryId,
        ],
        set: { isPrimary: true },
      })
  })
}

// Dogfood: bind the public workspace's single Exponential project to the repo
// named by DOGFOOD_REPO, so the team previews + tests Exponential inside
// Exponential and files straight into this project. Cloud-only + gated behind
// DOGFOOD_REPO (self-hosters never inherit a repo binding). Purely about the
// repo binding — the project itself always exists by now (ensurePublicProject
// runs first, unconditionally). Idempotent: it seeds the preview mirror once,
// links the repo registry row, and never clobbers a hand-edited previewConfig
// later.
async function ensureDogfoodProject(
  publicWorkspaceId: string,
  publicProjectId: string
) {
  const repo = process.env.DOGFOOD_REPO?.trim()
  if (!repo) return

  const [existing] = await db
    .select({ previewConfig: projects.previewConfig })
    .from(projects)
    .where(eq(projects.id, publicProjectId))
    .limit(1)
  if (!existing) return

  // Never overwrite a hand-edited mirror — only seed it if it's still missing.
  if (!existing.previewConfig) {
    await db
      .update(projects)
      .set({
        previewConfig: {
          targets: DOGFOOD_TARGETS,
          feedbackProjectId: publicProjectId,
        },
      })
      .where(eq(projects.id, publicProjectId))
  }
  await ensureDogfoodRepoLink(publicWorkspaceId, publicProjectId, repo)
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
        const publicWorkspaceId = await ensurePublicWorkspace()
        await addAdminsAsPublicWorkspaceOwners(publicWorkspaceId)
        // Canonical project first (created regardless of DOGFOOD_REPO), then
        // the optional dogfood repo binding, then fold the legacy feedback
        // project into the canonical one, then seed the widget config — which
        // needs the Exponential project to exist as its target.
        const publicProjectId = await ensurePublicProject(publicWorkspaceId)
        await ensureDogfoodProject(publicWorkspaceId, publicProjectId)
        await collapseLegacyFeedbackProject(publicWorkspaceId)
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
// the freshly-promoted admin as an owner of every public workspace.
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

  const publicWorkspaces = await db
    .select({ id: workspaces.id })
    .from(workspaces)
    .where(eq(workspaces.isPublic, true))
  for (const ws of publicWorkspaces) {
    await db
      .insert(workspaceMembers)
      .values({ workspaceId: ws.id, userId, role: `owner` })
      .onConflictDoNothing()
  }
}

// Useful for tests / admin tools.
export async function listInitialAdminEmails() {
  return parseAdminEmails()
}
