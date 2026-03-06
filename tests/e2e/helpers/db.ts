import { inArray, like, sql } from "drizzle-orm"
import { users, verifications } from "../../../src/db/auth-schema"
import { db } from "../../../src/db/connection"
import { workspaceMembers, workspaces } from "../../../src/db/schema"

function getElectricBaseUrl() {
  return process.env.ELECTRIC_URL || `http://localhost:30000`
}

export async function assertDatabaseReachable() {
  try {
    await db.execute(sql`select 1`)
  } catch (error) {
    throw new Error(
      `Playwright e2e prerequisites failed: could not reach Postgres using DATABASE_URL.`,
      { cause: error }
    )
  }
}

export async function assertElectricReachable() {
  const controller = new AbortController()
  const timeoutId = setTimeout(() => controller.abort(), 5_000)

  try {
    await fetch(getElectricBaseUrl(), {
      method: `GET`,
      signal: controller.signal,
    })
  } catch (error) {
    throw new Error(
      `Playwright e2e prerequisites failed: could not reach Electric at ${getElectricBaseUrl()}.`,
      { cause: error }
    )
  } finally {
    clearTimeout(timeoutId)
  }
}

export async function assertIssueNumberTriggerExists() {
  const result = await db.execute(sql`
    select t.tgname
    from pg_trigger t
    inner join pg_class c on c.oid = t.tgrelid
    where t.tgname = 'generate_issue_number'
      and c.relname = 'issues'
      and not t.tgisinternal
    limit 1
  `)

  if (result.rows.length === 0) {
    throw new Error(
      `Playwright e2e prerequisites failed: missing generate_issue_number trigger on issues.`
    )
  }
}

export async function cleanupNamespace(emailPrefix: string) {
  await db
    .delete(verifications)
    .where(like(verifications.identifier, `${emailPrefix}%`))

  const testUsers = await db
    .select({ id: users.id })
    .from(users)
    .where(like(users.email, `${emailPrefix}%`))

  if (testUsers.length === 0) {
    return
  }

  const userIds = testUsers.map((user) => user.id)
  const memberships = await db
    .select({ workspaceId: workspaceMembers.workspaceId })
    .from(workspaceMembers)
    .where(inArray(workspaceMembers.userId, userIds))

  const workspaceIds = [...new Set(memberships.map((membership) => membership.workspaceId))]

  if (workspaceIds.length > 0) {
    await db.delete(workspaces).where(inArray(workspaces.id, workspaceIds))
  }

  await db.delete(users).where(inArray(users.id, userIds))
}
