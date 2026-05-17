import { eq, sql } from "drizzle-orm"
import { db } from "@/db/connection"
import { projects, workspaces } from "@/db/schema"
import { users } from "@/db/auth-schema"
import { invalidatePublicWorkspaceCache } from "@/lib/workspace-membership"

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
    .where(eq(workspaces.isPublic, true))
    .limit(1)

  if (existing) {
    return existing.id
  }

  const [workspace] = await db
    .insert(workspaces)
    .values({
      name: PUBLIC_WORKSPACE_NAME,
      slug: PUBLIC_WORKSPACE_SLUG,
      isPublic: true,
    })
    .returning({ id: workspaces.id })

  await db.insert(projects).values({
    workspaceId: workspace.id,
    name: PUBLIC_PROJECT_NAME,
    slug: PUBLIC_PROJECT_SLUG,
    prefix: PUBLIC_PROJECT_PREFIX,
  })

  invalidatePublicWorkspaceCache()
  return workspace.id
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

let bootstrapPromise: Promise<void> | null = null

export function bootstrapCloud(): Promise<void> {
  if (bootstrapPromise) return bootstrapPromise
  bootstrapPromise = (async () => {
    try {
      await ensurePublicWorkspace()
      await promoteInitialAdmins()
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
// doesn't need to wait for a server restart.
export async function maybePromoteNewUser(userId: string, email: string) {
  const emails = parseAdminEmails()
  if (emails.length === 0) return
  if (!emails.includes(email.toLowerCase())) return
  await db
    .update(users)
    .set({ isAdmin: true, updatedAt: new Date() })
    .where(eq(users.id, userId))
}

// Useful for tests / admin tools.
export async function listInitialAdminEmails() {
  return parseAdminEmails()
}
