// #EXP-1616: `/api/ecode/git/<op>`, the wire of eCode's own git. Its
// clients are `git-remote-exp` (a member's `git clone/fetch/push
// exp://<team>/<repo>`) and the device jobs; the bearer is a repo token
// (`repo-token.ts`), re-checked against the member's CURRENT membership on
// every call. The server moves no object bytes: it hands out presigned URLs
// (device ↔ bucket), records packs and moves refs by CAS.
//
//   GET  refs                → {head, refs}                         member
//   GET  packs?after=<seq>   → {packs: [{id, seq, bytes, url}]}     member
//   POST upload {kind, bytes}→ {key, url} (a presigned PUT)         member (bundle: job)
//   POST push {pack, updates}→ {ok} | 409 {rejected}                member
//   GET  object?key=         → {url} (a presigned GET)              job
//   POST bundle {key, refs}  → {ok} (a backup landed)               job
//   POST repack {pack, replaces} → {ok}                             job
//   POST restore {pack, refs}→ {ok} (replaces every pack and ref)   job
import { TRPCError } from "@trpc/server"
import { eq } from "drizzle-orm"
import { z } from "zod"
import { db } from "@/db/connection"
import { repositories } from "@/db/schema"
import { resolveTeamAccess } from "@/lib/auth/access"
import {
  HEADS,
  livePacks,
  listRefs,
  pruneSuperseded,
  recordBundle,
  recordPush,
  replaceRepository,
  repoObjectPrefix,
  swapPacks,
  validRef,
  validSha,
  type RefUpdate,
} from "@/lib/ecode/git-store"
import { verifyRepoToken, type RepoTokenPayload } from "@/lib/ecode/repo-token"

/** The largest object one upload may be (a repo's first push included). */
export const MAX_UPLOAD_BYTES = 2 * 1024 * 1024 * 1024
/** Past this many live packs a push queues a repack (fetches stay cheap). */
export const REPACK_AT = 48

interface Caller {
  token: RepoTokenPayload
  repo: { id: string; teamId: string; fullName: string; defaultBranch: string }
}

function fail(code: TRPCError[`code`], message: string): never {
  throw new TRPCError({ code, message })
}

async function authorize(request: Request): Promise<Caller> {
  const header = request.headers.get(`authorization`) ?? ``
  const token = verifyRepoToken(header.replace(/^Bearer\s+/i, ``))
  if (!token) fail(`UNAUTHORIZED`, `Unauthorized`)
  const [repo] = await db
    .select({
      id: repositories.id,
      teamId: repositories.teamId,
      fullName: repositories.fullName,
      defaultBranch: repositories.defaultBranch,
      override: repositories.defaultBranchOverride,
      provider: repositories.provider,
      archivedAt: repositories.archivedAt,
    })
    .from(repositories)
    .where(eq(repositories.id, token.r))
    .limit(1)
  if (!repo || repo.provider !== `ecode` || repo.archivedAt) fail(`NOT_FOUND`, `Repository not found`)
  // Members push (#EXP-1616 access); a removed member's token stops here.
  await resolveTeamAccess(token.u, repo.teamId)
  return {
    token,
    repo: { id: repo.id, teamId: repo.teamId, fullName: repo.fullName, defaultBranch: repo.override ?? repo.defaultBranch },
  }
}

function requireJob(caller: Caller) {
  if (caller.token.s !== `job`) fail(`FORBIDDEN`, `Only a device job may do this`)
}

/** A key this repo owns (an upload it was handed). */
function ownedKey(caller: Caller, key: string, kind: `pack` | `bundle`): string {
  const prefix = `${repoObjectPrefix(caller.repo.teamId, caller.repo.id)}${kind}s/`
  if (!key.startsWith(prefix) || key.includes(`..`)) fail(`BAD_REQUEST`, `Not an object of this repository`)
  return key
}

/** The stored size of an uploaded object (never what the client claims). */
async function storedBytes(key: string): Promise<number> {
  const { headObject } = await import(`@/lib/storage`)
  const head = await headObject(key)
  if (!head) fail(`PRECONDITION_FAILED`, `The upload ${key} is not in the bucket`)
  return head.sizeBytes
}

const sha = z.string().refine(validSha, `not a sha`)
const updateSchema = z.object({
  ref: z.string().refine(validRef, `not a valid ref`),
  old: sha.nullable(),
  new: sha.nullable(),
})
const refsSchema = z.record(z.string().refine(validRef), sha)

async function body<T>(request: Request, schema: z.ZodType<T>): Promise<T> {
  const parsed = schema.safeParse(await request.json().catch(() => null))
  if (!parsed.success) fail(`BAD_REQUEST`, parsed.error.issues.map((i) => i.message).join(`; `))
  return parsed.data
}

type Handler = (caller: Caller, request: Request, url: URL) => Promise<unknown>

const routes: Record<string, Partial<Record<`GET` | `POST`, Handler>>> = {
  refs: {
    GET: async (caller) => ({ head: `${HEADS}${caller.repo.defaultBranch}`, refs: await listRefs(caller.repo.id) }),
  },

  packs: {
    GET: async (caller, _request, url) => {
      const after = Number(url.searchParams.get(`after`) ?? 0) || 0
      const { presignGet } = await import(`@/lib/storage`)
      const packs = await livePacks(caller.repo.id, after)
      return { packs: await Promise.all(packs.map(async (p) => ({ id: p.id, seq: p.seq, bytes: p.bytes, url: await presignGet(p.key) }))) }
    },
  },

  upload: {
    POST: async (caller, request) => {
      const input = await body(request, z.object({ kind: z.enum([`pack`, `bundle`]), bytes: z.number().int().positive().max(MAX_UPLOAD_BYTES) }))
      if (input.kind === `bundle`) requireJob(caller)
      const { assertWithinStorageLimit } = await import(`@/lib/billing`)
      await assertWithinStorageLimit(caller.repo.teamId, input.bytes)
      const key = `${repoObjectPrefix(caller.repo.teamId, caller.repo.id)}${input.kind}s/${crypto.randomUUID()}.${input.kind}`
      const { presignPut } = await import(`@/lib/storage`)
      return { key, url: await presignPut(key, input.bytes) }
    },
  },

  push: {
    POST: async (caller, request) => {
      const input = await body(request, z.object({ pack: z.string().nullable().optional(), updates: z.array(updateSchema).max(500) }))
      const updates: RefUpdate[] = input.updates
      for (const u of updates) {
        if (!u.ref.startsWith(HEADS) && !u.ref.startsWith(`refs/tags/`)) fail(`BAD_REQUEST`, `Only branches and tags are pushed: ${u.ref}`)
        // The default branch never goes (`git push --delete main`).
        if (u.new === null && u.ref === `${HEADS}${caller.repo.defaultBranch}`) fail(`FORBIDDEN`, `The default branch cannot be deleted`)
      }
      const pack = input.pack ? { key: ownedKey(caller, input.pack, `pack`), bytes: 0 } : null
      if (pack) pack.bytes = await storedBytes(pack.key)
      const pushed = await recordPush({ teamId: caller.repo.teamId, repositoryId: caller.repo.id, pack, updates })
      if (!pushed.ok) return { status: 409, body: { ok: false, rejected: pushed.rejected } }
      await afterPush(caller, updates)
      return { ok: true }
    },
  },

  object: {
    GET: async (caller, _request, url) => {
      requireJob(caller)
      const key = url.searchParams.get(`key`) ?? ``
      const owned = key.includes(`/bundles/`) ? ownedKey(caller, key, `bundle`) : ownedKey(caller, key, `pack`)
      const { presignGet } = await import(`@/lib/storage`)
      return { url: await presignGet(owned) }
    },
  },

  bundle: {
    POST: async (caller, request) => {
      requireJob(caller)
      const input = await body(request, z.object({ key: z.string(), refs: refsSchema }))
      const key = ownedKey(caller, input.key, `bundle`)
      await recordBundle({ teamId: caller.repo.teamId, repositoryId: caller.repo.id, key, bytes: await storedBytes(key), refs: input.refs })
      return { ok: true }
    },
  },

  repack: {
    POST: async (caller, request) => {
      requireJob(caller)
      const input = await body(request, z.object({ pack: z.string(), replaces: z.array(z.string().uuid()).max(10_000) }))
      const key = ownedKey(caller, input.pack, `pack`)
      await swapPacks({ teamId: caller.repo.teamId, repositoryId: caller.repo.id, pack: { key, bytes: await storedBytes(key) }, replaces: input.replaces })
      await pruneSuperseded(caller.repo.id)
      return { ok: true }
    },
  },

  restore: {
    POST: async (caller, request) => {
      requireJob(caller)
      const input = await body(request, z.object({ pack: z.string(), refs: refsSchema }))
      const key = ownedKey(caller, input.pack, `pack`)
      await replaceRepository({ teamId: caller.repo.teamId, repositoryId: caller.repo.id, pack: { key, bytes: await storedBytes(key) }, refs: input.refs })
      return { ok: true }
    },
  },
}

/** What GitHub's `push` webhook does, plus the repack once packs pile up. */
async function afterPush(caller: Caller, updates: RefUpdate[]): Promise<void> {
  const { pgEcodeHost } = await import(`@/lib/ecode/ecode-host`)
  const repo = { id: caller.repo.id, fullName: caller.repo.fullName, provider: `ecode` as const }
  const events = updates
    .filter((u) => u.new !== null && u.ref.startsWith(HEADS))
    .map((u) => ({ kind: `pushed` as const, branch: u.ref.slice(HEADS.length), sha: u.new! }))
  await pgEcodeHost.publish(repo, events).catch((err: unknown) => console.error(`[ecode] push events failed`, err))
  if ((await livePacks(caller.repo.id)).length >= REPACK_AT) {
    const { queueMaintenance } = await import(`@/lib/ecode/jobs`)
    await queueMaintenance(caller.repo, { op: `repack` }).catch(() => null)
  }
}

/** The route's one handler: `/api/ecode/git/<op>`. */
export async function handleGitRequest(request: Request, op: string): Promise<Response> {
  const { errorToResponse } = await import(`@/lib/http-errors`)
  try {
    const handler = routes[op]?.[request.method as `GET` | `POST`]
    if (!handler) return Response.json({ error: `Not found` }, { status: 404 })
    const caller = await authorize(request)
    const out = (await handler(caller, request, new URL(request.url))) as { status?: number; body?: unknown }
    if (out && typeof out.status === `number` && `body` in out) return Response.json(out.body, { status: out.status })
    return Response.json(out)
  } catch (err) {
    return errorToResponse(err)
  }
}

/** For tests: the route table's ops. */
export const gitRouteOps = Object.keys(routes)
