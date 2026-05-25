import { eq, sql } from "drizzle-orm"
import { db } from "@/db/connection"
import { projects, workspaceMembers, workspaces } from "@/db/schema"
import { users } from "@/db/auth-schema"
import { invalidatePublicWorkspaceCache } from "@/lib/workspace-membership"
// Vite's ?raw suffix inlines file contents as a string at build time. We
// do this so the server bundle ships the SQL alongside the JS, no fs reads
// required at runtime (which Vite also can't tree-shake for browser builds).
import triggersSql from "@/db/out/custom/0001_triggers.sql?raw"
import publicWorkspaceSql from "@/db/out/custom/0002_public_workspace.sql?raw"

const PUBLIC_WORKSPACE_SLUG = `feedback`
const PUBLIC_WORKSPACE_NAME = `Exponential Feedback`
const PUBLIC_PROJECT_SLUG = `feedback`
const PUBLIC_PROJECT_NAME = `Feedback`
const PUBLIC_PROJECT_PREFIX = `FB`

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

    await db.insert(projects).values({
      workspaceId: workspace.id,
      name: PUBLIC_PROJECT_NAME,
      slug: PUBLIC_PROJECT_SLUG,
      prefix: PUBLIC_PROJECT_PREFIX,
    })

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
      sql`lower(${users.email}) IN (${sql.join(
        emails.map((email) => sql`${email}`),
        sql`, `
      )})`
    )
}

// Drizzle migrations don't run our hand-written triggers + partial unique
// index. Apply them on every boot — every statement is idempotent
// (CREATE OR REPLACE / CREATE … IF NOT EXISTS).
async function applyCustomSql() {
  for (const [name, content] of [
    [`0001_triggers.sql`, triggersSql],
    [`0002_public_workspace.sql`, publicWorkspaceSql],
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

function isCloudInstance(): boolean {
  return !process.env.PUBLIC_FEEDBACK_URL?.trim()
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
      }
    } catch (err) {
      console.error(`[bootstrap-cloud] failed:`, err)
      bootstrapPromise = null
      throw err
    }
  })()
  return bootstrapPromise
}

// Promote a single newly-created user if their email matches the admin list.
// Used by Better Auth's user.create.after hook so first-sign-in promotion
// doesn't need to wait for a server restart. Also adds the freshly-promoted
// admin as an owner of every public workspace.
export async function maybePromoteNewUser(userId: string, email: string) {
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
