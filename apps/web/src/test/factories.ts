// Shared DB-backed test factories. v4 collapses board↔repository to a single
// mandatory `boards.repository_id`, so a board can no longer be inserted
// without a backing `repositories` row. `createTestBoard` auto-creates one
// via `ensureTestRepository` when the caller doesn't pass a `repositoryId`, so
// existing tests that only care about a board keep working unchanged.
import { and, eq } from "drizzle-orm"
import { db } from "@/db/connection"
import { boards, repositories } from "@/db/schema"

let repoCounter = 0
let boardCounter = 0

// Upsert a `repositories` row for the team and return its id. Idempotent
// via the (team_id, full_name) unique — safe to call repeatedly with an
// explicit `fullName`. Defaults to a unique synthetic name so unrelated tests
// never collide. `installationId` is null (self-heal fills it in production).
export async function ensureTestRepository(
  teamId: string,
  opts?: { fullName?: string; installationId?: number | null }
): Promise<string> {
  const fullName =
    opts?.fullName ?? `test-org/repo-${++repoCounter}-${Date.now()}`
  const [inserted] = await db
    .insert(repositories)
    .values({
      teamId,
      fullName,
      installationId: opts?.installationId ?? null,
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
        eq(repositories.teamId, teamId),
        eq(repositories.fullName, fullName)
      )
    )
    .limit(1)
  return row.id
}

// Insert a board, auto-creating its mandatory backing repository when the
// caller doesn't supply one. Returns both ids so tests can assert the 1:1 link.
export async function createTestBoard(input: {
  teamId: string
  name?: string
  slug?: string
  prefix?: string
  repositoryId?: string
}): Promise<{ id: string; repositoryId: string }> {
  const repositoryId =
    input.repositoryId ?? (await ensureTestRepository(input.teamId))
  const suffix = `${++boardCounter}-${Date.now()}`
  const [board] = await db
    .insert(boards)
    .values({
      teamId: input.teamId,
      name: input.name ?? `Test Board`,
      slug: input.slug ?? `proj-${suffix}`,
      prefix: input.prefix ?? `TST`,
      repositoryId,
    })
    .returning({ id: boards.id })
  return { id: board.id, repositoryId }
}
