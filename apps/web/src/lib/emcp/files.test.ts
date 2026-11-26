// @vitest-environment node
// #NEW-149 (packages/emcp fixtures/files.json, D5): the files extension on
// `/api/mcp` end to end through the gate's transport: the flag, the
// `x-mcp-file` declaration, inline files onto the upload op's inline path,
// `upload_required` to the signed staging URL, and the retry's checks
// before the op's finalize. The upload op itself is a recorder here.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"
import { z } from "zod"
import protocol from "@exp/emcp/fixtures/protocol.json" with { type: "json" }
import files from "@exp/emcp/fixtures/files.json" with { type: "json" }
import { clientContextOf } from "@exp/emcp/protocol"

const calls: Record<string, unknown>[] = []
const landed = new Map<string, { id: string; uploaderId: string; sizeBytes: number; contentType: string; storageKey: string }>()
const deleted: string[] = []
/** mcp_file_stagings: attachment id -> expiry. */
const stagings = new Map<string, Date>()

vi.mock(`@/lib/runtime-config`, () => ({ buildRuntimeConfig: () => ({}) }))
vi.mock(`@/lib/storage`, () => ({
  getObject: async () => ({ Body: { transformToByteArray: async () => new TextEncoder().encode(`hello`) } }),
  deleteObject: async (key: string) => void deleted.push(key),
}))
vi.mock(`@/db/connection`, async () => {
  const { getTableName } = await import(`drizzle-orm`)
  /** The ids a drizzle condition names (a walk over its chunks). */
  const named = (condition: unknown, known: (id: string) => boolean): string[] => {
    const seen = new Set<unknown>()
    const out: string[] = []
    const walk = (value: unknown) => {
      if (typeof value === `string`) return void (known(value) && out.push(value))
      if (!value || typeof value !== `object` || seen.has(value)) return
      seen.add(value)
      for (const child of Object.values(value)) walk(child)
    }
    walk(condition)
    return out
  }
  const isStaging = (table: unknown) => getTableName(table as never) === `mcp_file_stagings`
  return {
    db: {
      select: () => {
        let table: unknown = null
        let ids: string[] = []
        const chain = {
          from: (t: unknown) => ((table = t), chain),
          where: (condition: unknown) => {
            ids = isStaging(table) ? [...stagings.entries()].filter(([, at]) => at <= new Date()).map(([id]) => id) : named(condition, (id) => landed.has(id))
            return chain
          },
          limit: async () => (isStaging(table) ? ids.map((attachmentId) => ({ attachmentId })) : ids.slice(0, 1).map((id) => landed.get(id))),
        }
        return chain
      },
      insert: (table: unknown) => ({
        values: async (row: { attachmentId: string; expiresAt: Date }) => {
          if (isStaging(table)) stagings.set(row.attachmentId, row.expiresAt)
        },
      }),
      delete: (table: unknown) => {
        let ids: string[] = []
        const run = async () => {
          const rows = ids.map((id) => landed.get(id)).filter(Boolean)
          for (const id of ids) (isStaging(table) ? stagings : landed).delete(id)
          return isStaging(table) ? [] : rows
        }
        return {
          where: (condition: unknown) => {
            ids = named(condition, (id) => (isStaging(table) ? stagings.has(id) : landed.has(id)))
            let done: Promise<unknown[]> | null = null
            const once = () => (done ??= run())
            return { then: (resolve: (v: unknown) => void, reject: (e: unknown) => void) => once().then(resolve, reject), returning: once }
          },
        }
      },
    },
  }
})
vi.mock(`./tools`, async () => {
  const { mintAttachmentUploadToken } = await import(`@/lib/storage/attachment-upload-token`)
  return {
    registerExponentialTools: (server: { registerTool: (name: string, def: unknown, handler: unknown) => void }) => {
      const answer = (value: unknown) => ({ content: [{ type: `text`, text: JSON.stringify(value) }] })
      server.registerTool(
        `exponential_attachments_upload`,
        {
          inputSchema: z.strictObject({
            issueId: z.string().optional(),
            filename: z.string().optional(),
            contentType: z.string().optional(),
            dataBase64: z.string().min(1).optional(),
            alt: z.string().optional(),
            commentId: z.string().optional(),
            attachmentId: z.string().optional(),
          }),
        },
        async (args: Record<string, unknown>) => {
          calls.push(args)
          if (args.attachmentId) return answer({ id: args.attachmentId, finalized: true })
          if (args.dataBase64) return answer({ id: `inline` })
          const { token, expiresAt } = mintAttachmentUploadToken({
            attachmentId: `0b8f6c2e-3a51-4c07-9d1e-6f1b2a3c4d5e`,
            issueId: `11111111-1111-4111-8111-111111111111`,
            boardId: `22222222-2222-4222-8222-222222222222`,
            teamId: `t1`,
            userId: `u1`,
            filename: String(args.filename),
            contentType: String(args.contentType),
          })
          return answer({ attachmentId: `0b8f6c2e-3a51-4c07-9d1e-6f1b2a3c4d5e`, uploadUrl: `https://x.test/api/attachment-uploads/${token}`, expiresAt: expiresAt.toISOString() })
        }
      )
      server.registerTool(`exponential_attachments_list`, { annotations: { readOnlyHint: true }, inputSchema: z.strictObject({}) }, async () => answer([]))
      server.registerTool(`exponential_apps_get`, { annotations: { readOnlyHint: true }, inputSchema: z.strictObject({ slug: z.string() }) }, async () =>
        answer({ id: `a1`, slug: `standup`, name: `Standup`, teamId: `t1`, package: {} })
      )
    },
  }
})

process.env.BETTER_AUTH_SECRET ??= `local-test-secret-local-test-secret`
const { handleMcpPost } = await import(`./gate`)
const { createExponentialMcpServer } = await import(`./server`)
const { FULL_ACCESS } = await import(`./scope`)
const { callOf, memoryRedeemStore, setRedeemStore } = await import(`./mrtr`)
const { verifyAttachmentUploadToken } = await import(`@/lib/storage/attachment-upload-token`)
const { downloadMetaFor } = await import(`./files`)
const { HOST_CONFIRMS_META } = await import(`@exp/mcp-apps/genui-result`)

const MODERN = `2026-07-28`
const UPLOAD = { extensions: { [files.extension]: { upload: {} } } }

type Rpc = { result?: Record<string, unknown>; error?: { code: number; message: string; data?: unknown } }

async function send(message: Record<string, unknown>, userId = `u1`): Promise<Rpc> {
  const headers: Record<string, string> = { "content-type": `application/json`, accept: `application/json, text/event-stream`, "mcp-protocol-version": MODERN, "mcp-method": String(message.method) }
  const name = (message.params as { name?: unknown } | undefined)?.name
  if (typeof name === `string`) headers[`mcp-name`] = name
  const request = new Request(`https://x.test/api/mcp`, { method: `POST`, headers, body: JSON.stringify(message) })
  const call = callOf(message)
  const server = createExponentialMcpServer({ id: userId } as never, request, FULL_ACCESS, null, undefined, {
    client: clientContextOf(message),
    principal: { userId, grant: `oauth:c1` },
    ...(call ? { call } : {}),
  })
  const response = await handleMcpPost(server, request)
  const text = await response.text()
  return (text ? JSON.parse(text) : {}) as Rpc
}

const meta = (capabilities: Record<string, unknown>) => ({
  [protocol.metaKeys.protocolVersion]: MODERN,
  [protocol.metaKeys.clientInfo]: { name: `test`, version: `1` },
  [protocol.metaKeys.clientCapabilities]: capabilities,
})

let id = 1
const callTool = (args: Record<string, unknown>, capabilities: Record<string, unknown> = UPLOAD, extra: Record<string, unknown> = {}, userId = `u1`) =>
  send({ jsonrpc: `2.0`, id: id++, method: `tools/call`, params: { name: `exponential_attachments_write`, arguments: args, _meta: meta(capabilities), ...extra } }, userId)

const text = (rpc: Rpc) => ((rpc.result?.content as { text?: string }[] | undefined)?.[0]?.text ?? ``)
const PDF = { id: `file-1`, name: `Q3 report.pdf`, mimeType: `application/pdf`, size: 5 }
const BIG = { ...PDF, size: 2 * 1024 * 1024 }

beforeEach(() => {
  process.env.MCP_FILES_EXTENSION = `1`
  setRedeemStore(memoryRedeemStore())
  calls.length = 0
  landed.clear()
  stagings.clear()
  deleted.length = 0
})
afterEach(() => {
  delete process.env.MCP_FILES_EXTENSION
})

describe(`declaration`, () => {
  it(`flag on: server/discover names the extension, the write half its x-mcp-file argument`, async () => {
    const discover = await send({ jsonrpc: `2.0`, id: 1, method: `server/discover`, params: { _meta: meta({}) } })
    expect((discover.result?.capabilities as { extensions: Record<string, unknown> }).extensions[files.extension]).toEqual(files.capability)
    const list = await send({ jsonrpc: `2.0`, id: 2, method: `tools/list`, params: { _meta: meta({}) } })
    const write = (list.result?.tools as { name: string; inputSchema: { properties: Record<string, unknown> } }[]).find((t) => t.name === `exponential_attachments_write`)!
    expect(write.inputSchema.properties.file).toEqual(files.schemaCases[0]!.expected)
  })

  it(`flag off: neither, and a file argument is an unknown key`, async () => {
    delete process.env.MCP_FILES_EXTENSION
    const discover = await send({ jsonrpc: `2.0`, id: 1, method: `server/discover`, params: { _meta: meta({}) } })
    expect((discover.result?.capabilities as { extensions: Record<string, unknown> }).extensions[files.extension]).toBeUndefined()
    const list = await send({ jsonrpc: `2.0`, id: 2, method: `tools/list`, params: { _meta: meta({}) } })
    const write = (list.result?.tools as { name: string; inputSchema: { properties: Record<string, unknown> } }[]).find((t) => t.name === `exponential_attachments_write`)!
    expect(write.inputSchema.properties.file).toBeUndefined()
    const refused = await callTool({ op: `upload`, issueId: `EXP-12`, file: { ...PDF, data: `aGVsbG8=` } })
    expect(refused.result?.isError).toBe(true)
    expect(calls).toEqual([])
  })
})

describe(`first call`, () => {
  it(`inline bytes run the inline path`, async () => {
    const rpc = await callTool({ op: `upload`, issueId: `EXP-12`, file: { ...PDF, data: `aGVsbG8=` } }, {})
    expect(rpc.result?.isError).toBeFalsy()
    expect(calls).toEqual([{ issueId: `EXP-12`, filename: `Q3 report.pdf`, contentType: `application/pdf`, dataBase64: `aGVsbG8=` }])
  })

  it(`a refused file is the fixture's isError and runs nothing`, async () => {
    const rpc = await callTool({ op: `upload`, issueId: `EXP-12`, file: { ...PDF, size: 6, data: `aGVsbG8=` } })
    expect(rpc.result?.isError).toBe(true)
    expect(text(rpc)).toBe(files.errors.sizeMismatch.replace(`{id}`, `file-1`))
    expect(calls).toEqual([])
  })

  it(`the file carries name, type and bytes: no second source`, async () => {
    const rpc = await callTool({ op: `upload`, issueId: `EXP-12`, filename: `x`, file: { ...PDF, data: `aGVsbG8=` } })
    expect(rpc.result?.isError).toBe(true)
    expect(calls).toEqual([])
  })

  it(`out of band without upload declared: -32021`, async () => {
    const rpc = await callTool({ op: `upload`, issueId: `EXP-12`, file: BIG }, {})
    expect(rpc.error?.code).toBe(files.missingCapability.code)
    expect(rpc.error?.data).toEqual(files.capabilityCases[1]!.expected!.data)
    expect(calls).toEqual([])
  })

  it(`out of band: upload_required to the signed staging URL, capped at the declared size`, async () => {
    const rpc = await callTool({ op: `upload`, issueId: `EXP-12`, file: BIG })
    expect(rpc.result?.resultType).toBe(`upload_required`)
    const [upload] = rpc.result?.uploads as { fileId: string; url: string; method: string; headers: Record<string, string>; credentials: string; expiresAt: string }[]
    expect(upload).toMatchObject({ fileId: `file-1`, method: `PUT`, headers: { "Content-Type": `application/pdf` }, credentials: `omit` })
    expect(upload!.url.startsWith(`https://x.test${files.staging.path}`)).toBe(true)
    const token = verifyAttachmentUploadToken(upload!.url.slice(`https://x.test${files.staging.path}`.length))
    expect(token).toMatchObject({ a: `0b8f6c2e-3a51-4c07-9d1e-6f1b2a3c4d5e`, s: BIG.size, c: `application/pdf` })
    expect(Date.parse(upload!.expiresAt) - Date.now()).toBeGreaterThan(files.staging.ttlMs - 60_000)
    expect(typeof rpc.result?.requestState).toBe(`string`)
    expect(calls).toEqual([{ issueId: `EXP-12`, filename: `Q3 report.pdf`, contentType: `application/pdf` }])
    // Staged until the retry finalizes it.
    expect([...stagings.keys()]).toEqual([`0b8f6c2e-3a51-4c07-9d1e-6f1b2a3c4d5e`])
  })
})

describe(`retry`, () => {
  const args = { op: `upload`, issueId: `EXP-12`, file: BIG }
  const row = { id: `0b8f6c2e-3a51-4c07-9d1e-6f1b2a3c4d5e`, uploaderId: `u1`, sizeBytes: BIG.size, contentType: `application/pdf`, storageKey: `k1` }

  async function staged() {
    const first = await callTool(args)
    calls.length = 0
    return first.result?.requestState as string
  }

  it(`bytes arrived: the op finalizes the staged attachment`, async () => {
    const state = await staged()
    landed.set(row.id, row)
    const rpc = await callTool(args, UPLOAD, { requestState: state })
    expect(rpc.result?.isError).toBeFalsy()
    expect(calls).toEqual([{ attachmentId: row.id }])
    expect(stagings.size).toBe(0)
    // Single use.
    const again = await callTool(args, UPLOAD, { requestState: state })
    expect(again.error?.code).toBe(-32602)
  })

  it(`missing or short bytes: isError, never a second upload_required`, async () => {
    const state = await staged()
    landed.set(row.id, { ...row, sizeBytes: 1000 })
    const rpc = await callTool(args, UPLOAD, { requestState: state })
    expect(rpc.result?.isError).toBe(true)
    expect(text(rpc)).toBe(files.errors.missingBytes.replace(`{id}`, `file-1`))
    expect(calls).toEqual([])
  })

  it(`wrong bytes: digestMismatch, and the landed upload is discarded`, async () => {
    const withDigest = { ...args, file: { ...BIG, size: 5, digest: { algorithm: `sha-256`, value: `SG6kYiTRu0-2gPNPfJrZao8k7Ii-c-qOWmxlJg6cuKc` } } }
    // size 5 inline-sized but out of band (no data): still staged.
    const first = await callTool(withDigest)
    expect(first.result?.resultType).toBe(`upload_required`)
    calls.length = 0
    landed.set(row.id, { ...row, sizeBytes: 5 })
    const rpc = await callTool(withDigest, UPLOAD, { requestState: first.result?.requestState })
    expect(text(rpc)).toBe(files.errors.digestMismatch.replace(`{id}`, `file-1`))
    expect(deleted).toEqual([`k1`])
    expect(landed.has(row.id)).toBe(false)
    expect(calls).toEqual([])
  })

  it(`another type landed: refused and discarded`, async () => {
    const state = await staged()
    landed.set(row.id, { ...row, contentType: `text/html` })
    const rpc = await callTool(args, UPLOAD, { requestState: state })
    expect(text(rpc)).toBe(files.errors.invalid.replace(`{id}`, `file-1`))
    expect(deleted).toEqual([`k1`])
  })

  it(`changed arguments, another member: -32602, nothing runs`, async () => {
    const state = await staged()
    landed.set(row.id, row)
    expect((await callTool({ ...args, issueId: `EXP-13` }, UPLOAD, { requestState: state })).error?.code).toBe(-32602)
    expect((await callTool(args, UPLOAD, { requestState: state }, `u2`)).error?.code).toBe(-32602)
    expect(calls).toEqual([])
  })
})

describe(`the sweep`, () => {
  it(`a staged upload nobody finalized goes after the staging window, with its object`, async () => {
    const { sweepExpiredFileStagings } = await import(`./files`)
    landed.set(`gone`, { id: `gone`, uploaderId: `u1`, sizeBytes: 5, contentType: `application/pdf`, storageKey: `k-gone` })
    stagings.set(`gone`, new Date(Date.now() - 1))
    stagings.set(`live`, new Date(Date.now() + files.staging.ttlMs))
    expect(await sweepExpiredFileStagings()).toBe(1)
    expect(landed.has(`gone`)).toBe(false)
    expect([...stagings.keys()]).toEqual([`live`])
    expect(deleted).toEqual([`k-gone`])
  })
})

describe(`downloads`, () => {
  const attachment = files.downloadCases[0]!.attachment
  const ctx = (capabilities: Record<string, unknown>) => ({ client: clientContextOf({ method: `tools/call`, params: { _meta: meta(capabilities) } }) })

  it(`a client that declared download gets the DownloadTarget`, () => {
    expect(downloadMetaFor(ctx({ extensions: { [files.extension]: { download: {} } } }), attachment)).toEqual(files.downloadCases[0]!.expected)
  })

  it(`undeclared, or the flag off: nothing`, () => {
    expect(downloadMetaFor(ctx({}), attachment)).toBeNull()
    delete process.env.MCP_FILES_EXTENSION
    expect(downloadMetaFor(ctx({ extensions: { [files.extension]: { download: {} } } }), attachment)).toBeNull()
  })
})

describe(`the genUI view's confirm (#NEW-149)`, () => {
  const read = (capabilities: Record<string, unknown>) =>
    send({ jsonrpc: `2.0`, id: id++, method: `tools/call`, params: { name: `exponential_apps`, arguments: { op: `get`, slug: `standup` }, _meta: meta(capabilities) } })

  it(`an app read for a host that declared form elicitation carries the mark`, async () => {
    const rpc = await read({ elicitation: {} })
    expect((rpc.result?._meta as Record<string, unknown> | undefined)?.[HOST_CONFIRMS_META]).toBe(true)
  })

  it(`no elicitation, no mark: the view confirms in place`, async () => {
    const rpc = await read({})
    expect((rpc.result?._meta as Record<string, unknown> | undefined)?.[HOST_CONFIRMS_META]).toBeUndefined()
  })
})
