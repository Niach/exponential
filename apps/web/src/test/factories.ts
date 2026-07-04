// Shared DB-backed test factories. v4 collapses project↔repository to a single
// mandatory `projects.repository_id`, so a project can no longer be inserted
// without a backing `repositories` row. `createTestProject` auto-creates one
// via `ensureTestRepository` when the caller doesn't pass a `repositoryId`, so
// existing tests that only care about a project keep working unchanged.
import { and, eq } from "drizzle-orm"
import { db } from "@/db/connection"
import { projects, repositories } from "@/db/schema"

let repoCounter = 0
let projectCounter = 0

// Upsert a `repositories` row for the workspace and return its id. Idempotent
// via the (workspace_id, full_name) unique — safe to call repeatedly with an
// explicit `fullName`. Defaults to a unique synthetic name so unrelated tests
// never collide. `installationId` is null (self-heal fills it in production).
export async function ensureTestRepository(
  workspaceId: string,
  opts?: { fullName?: string; installationId?: number | null }
): Promise<string> {
  const fullName =
    opts?.fullName ?? `test-org/repo-${++repoCounter}-${Date.now()}`
  const [inserted] = await db
    .insert(repositories)
    .values({
      workspaceId,
      fullName,
      installationId: opts?.installationId ?? null,
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
        eq(repositories.workspaceId, workspaceId),
        eq(repositories.fullName, fullName)
      )
    )
    .limit(1)
  return row.id
}

// Insert a project, auto-creating its mandatory backing repository when the
// caller doesn't supply one. Returns both ids so tests can assert the 1:1 link.
export async function createTestProject(input: {
  workspaceId: string
  name?: string
  slug?: string
  prefix?: string
  repositoryId?: string
}): Promise<{ id: string; repositoryId: string }> {
  const repositoryId =
    input.repositoryId ?? (await ensureTestRepository(input.workspaceId))
  const suffix = `${++projectCounter}-${Date.now()}`
  const [project] = await db
    .insert(projects)
    .values({
      workspaceId: input.workspaceId,
      name: input.name ?? `Test Project`,
      slug: input.slug ?? `proj-${suffix}`,
      prefix: input.prefix ?? `TST`,
      repositoryId,
    })
    .returning({ id: projects.id })
  return { id: project.id, repositoryId }
}
