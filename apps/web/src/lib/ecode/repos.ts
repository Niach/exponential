// #EXP-1616: the repositories of eCode's own git (provider `ecode`), behind
// `repositories.*`. Access (resolveTeamAccess): OWNERS create and delete
// repos and change their settings (mirror, restore); MEMBERS read, push and
// get a credential. A repo is born empty: its first `git push exp://…`
// creates its branches.
import { TRPCError } from "@trpc/server"
import { eq } from "drizzle-orm"
import { parseRemote } from "@exp/ecode"
import { db } from "@/db/connection"
import { boards, repositories, teams } from "@/db/schema"
import { resolveTeamAccess } from "@/lib/auth/access"
import { HEADS, collectRepoStorageKeys, listBundles, listRefs, repositoryByRemote } from "@/lib/ecode/git-store"
import { queueMaintenance } from "@/lib/ecode/jobs"
import { validMirrorUrl } from "@/lib/ecode/mirror"
import { mintRepoToken } from "@/lib/ecode/repo-token"

function fail(code: TRPCError[`code`], message: string): never {
  throw new TRPCError({ code, message })
}

async function assertOwner(userId: string, teamId: string) {
  await resolveTeamAccess(userId, teamId, `mutate_resources`, { roles: [`owner`] })
}

/** The `exp://<team>/<name>` remote of a repo row. */
export function ecodeRemoteUrl(teamSlug: string, name: string): string {
  return `exp://${teamSlug}/${name}`
}

async function loadEcodeRepo(repositoryId: string) {
  const [repo] = await db
    .select({
      id: repositories.id,
      teamId: repositories.teamId,
      provider: repositories.provider,
      fullName: repositories.fullName,
      remoteUrl: repositories.remoteUrl,
      defaultBranch: repositories.defaultBranch,
      defaultBranchOverride: repositories.defaultBranchOverride,
      providerConfig: repositories.providerConfig,
      archivedAt: repositories.archivedAt,
    })
    .from(repositories)
    .where(eq(repositories.id, repositoryId))
    .limit(1)
  if (!repo) fail(`NOT_FOUND`, `Repository not found`)
  if (repo.provider !== `ecode`) fail(`PRECONDITION_FAILED`, `${repo.fullName} is not an Exponential repository`)
  return repo
}

/** Owner: a new, empty repo on eCode's own git. */
export async function createEcodeRepository(
  userId: string,
  input: { teamId: string; name: string; defaultBranch?: string; mirrorUrl?: string | null }
) {
  await assertOwner(userId, input.teamId)
  if (input.mirrorUrl && !validMirrorUrl(input.mirrorUrl)) fail(`BAD_REQUEST`, `The mirror must be an https git URL`)
  const [team] = await db.select({ slug: teams.slug }).from(teams).where(eq(teams.id, input.teamId)).limit(1)
  if (!team) fail(`NOT_FOUND`, `Team not found`)
  const fullName = `${team.slug}/${input.name}`
  const [row] = await db
    .insert(repositories)
    .values({
      teamId: input.teamId,
      provider: `ecode`,
      fullName,
      remoteUrl: ecodeRemoteUrl(team.slug, input.name),
      defaultBranch: input.defaultBranch ?? `main`,
      private: true,
      providerConfig: input.mirrorUrl ? { mirrorUrl: input.mirrorUrl, mirrorEnabled: true } : {},
    })
    .onConflictDoNothing()
    .returning()
  if (!row) fail(`CONFLICT`, `This team already has a repository named ${input.name}`)
  return row
}

/** Owner: deletes the repo, its refs, its PR rows and every bucket object. */
export async function removeEcodeRepository(userId: string, repositoryId: string): Promise<void> {
  const repo = await loadEcodeRepo(repositoryId)
  await assertOwner(userId, repo.teamId)
  const backing = await db.select({ id: boards.id }).from(boards).where(eq(boards.repositoryId, repo.id))
  if (backing.length > 0) {
    fail(`CONFLICT`, `Can't remove this repository. It backs ${backing.length} board${backing.length === 1 ? `` : `s`}. Retarget or delete those boards first.`)
  }
  const keys = await collectRepoStorageKeys(db, { repositoryIds: [repo.id] })
  await db.delete(repositories).where(eq(repositories.id, repo.id))
  const { deleteObject } = await import(`@/lib/storage`)
  for (const key of keys) await deleteObject(key).catch((err: unknown) => console.warn(`[ecode] deleting ${key} failed`, err))
}

/** Member: the credential git-remote-exp uses (`repositories.credentials`). */
export async function ecodeCredentials(userId: string, repositoryId: string) {
  const repo = await loadEcodeRepo(repositoryId)
  if (repo.archivedAt) fail(`PRECONDITION_FAILED`, `${repo.fullName} is archived for this team`)
  await resolveTeamAccess(userId, repo.teamId)
  const { token, expiresAt } = mintRepoToken({ repositoryId: repo.id, userId })
  const [teamSlug, name] = repo.fullName.split(`/`)
  return {
    token,
    expiresAt,
    remoteUrl: repo.remoteUrl ?? ecodeRemoteUrl(teamSlug!, name!),
    username: `exp`,
    fullName: repo.fullName,
    defaultBranch: repo.defaultBranchOverride ?? repo.defaultBranch,
  }
}

/** The repo an `exp://<team>/<name>` URL names (git-remote-exp's first call). */
export async function ecodeRepositoryIdForRemote(remote: string): Promise<string> {
  const parsed = parseRemote(remote)
  if (!parsed || parsed.scheme !== `exp`) fail(`BAD_REQUEST`, `Expected exp://<team>/<repo>`)
  const repo = await repositoryByRemote(parsed.host, parsed.path)
  if (!repo) fail(`NOT_FOUND`, `${remote} is not a repository`)
  return repo.id
}

/** Member: the repo's branches, from its refs. */
export async function ecodeBranches(userId: string, repositoryId: string): Promise<Array<{ name: string; sha: string }>> {
  const repo = await loadEcodeRepo(repositoryId)
  await resolveTeamAccess(userId, repo.teamId)
  const refs = await listRefs(repo.id)
  return Object.entries(refs)
    .filter(([ref]) => ref.startsWith(HEADS))
    .map(([ref, sha]) => ({ name: ref.slice(HEADS.length), sha }))
}

/** Owner: the default branch (an empty repo takes any valid name). */
export async function setEcodeDefaultBranch(userId: string, repositoryId: string, branch: string) {
  const repo = await loadEcodeRepo(repositoryId)
  await assertOwner(userId, repo.teamId)
  const refs = await listRefs(repo.id)
  if (Object.keys(refs).length > 0 && !refs[`${HEADS}${branch}`]) fail(`BAD_REQUEST`, `'${branch}' is not a branch of ${repo.fullName}`)
  const [updated] = await db
    .update(repositories)
    .set({ defaultBranch: branch, defaultBranchOverride: null })
    .where(eq(repositories.id, repo.id))
    .returning()
  return updated!
}

/** Owner: the optional mirror (`provider_config.mirrorUrl`). */
export async function setEcodeMirror(userId: string, input: { repositoryId: string; mirrorUrl: string | null; enabled: boolean }) {
  const repo = await loadEcodeRepo(input.repositoryId)
  await assertOwner(userId, repo.teamId)
  const url = input.mirrorUrl?.trim() || null
  if (url && !validMirrorUrl(url)) fail(`BAD_REQUEST`, `The mirror must be an https git URL`)
  const config = { ...((repo.providerConfig ?? {}) as Record<string, unknown>) }
  if (url) config.mirrorUrl = url
  else delete config.mirrorUrl
  config.mirrorEnabled = input.enabled && Boolean(url)
  await db.update(repositories).set({ providerConfig: config }).where(eq(repositories.id, repo.id))
  return { mirrorUrl: url, mirrorEnabled: config.mirrorEnabled as boolean }
}

/** Member: the repo's settings as its page shows them. */
export async function ecodeRepositorySettings(userId: string, repositoryId: string) {
  const repo = await loadEcodeRepo(repositoryId)
  await resolveTeamAccess(userId, repo.teamId)
  const config = (repo.providerConfig ?? {}) as Record<string, unknown>
  const backups = await listBundles(repo.id)
  return {
    id: repo.id,
    fullName: repo.fullName,
    remoteUrl: repo.remoteUrl,
    defaultBranch: repo.defaultBranchOverride ?? repo.defaultBranch,
    mirrorUrl: typeof config.mirrorUrl === `string` ? config.mirrorUrl : null,
    mirrorEnabled: config.mirrorEnabled === true,
    backups: backups.map((b) => ({ id: b.id, createdAt: b.createdAt, bytes: b.bytes, refs: Object.keys(b.refs).length })),
  }
}

/** Owner: restore a backup (a device job replaces every pack and ref). */
export async function restoreEcodeBackup(userId: string, input: { repositoryId: string; backupId: string }) {
  const repo = await loadEcodeRepo(input.repositoryId)
  await assertOwner(userId, repo.teamId)
  const backup = (await listBundles(repo.id)).find((b) => b.id === input.backupId)
  if (!backup) fail(`NOT_FOUND`, `Backup not found`)
  const commandId = await queueMaintenance(repo, { op: `restore`, bundle: backup.key })
  if (!commandId) fail(`PRECONDITION_FAILED`, `No team device with eCode jobs is online to restore ${repo.fullName}`)
  return { commandId }
}
