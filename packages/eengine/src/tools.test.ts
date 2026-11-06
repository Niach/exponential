// VAPP-265: capabilities v1 replays (validateCases, bridgeCases, pdfCases).
import { createHash } from "node:crypto"
import { mkdirSync, writeFileSync } from "node:fs"

import { describe, expect, test } from "bun:test"

import capabilities from "../fixtures/capabilities.json" with { type: "json" }
import {
  nativeCaller,
  base64Decode,
  base64Encode,
  bridgeCall,
  CAPABILITIES,
  CAPABILITY_TOOLS,
  newNotificationId,
  pdfNumber,
  renderPdf,
  schemaViolation,
  toolDef,
  validateToolArguments,
  type BlobStore,
  type JsonSchema,
  type PdfArguments,
} from "./index"

const { v1 } = capabilities

function fakeBlobs(seed: Record<string, string> = {}) {
  const files = new Map(Object.entries({ ...v1.blobs, ...seed }).map(([id, b64]) => [id, base64Decode(b64) as Uint8Array]))
  const puts: { name: string; mimeType: string; base64: string }[] = []
  const store: BlobStore = {
    get: async (app, id) => (app === v1.context.app ? (files.get(id) ?? null) : null),
    put: async (app, name, mimeType, bytes) => {
      expect(app).toBe(v1.context.app)
      puts.push({ name, mimeType, base64: base64Encode(bytes) })
      const id = v1.blobIds.replace(`<n>`, String(puts.length))
      files.set(id, bytes)
      return id
    },
  }
  return { store, puts }
}

describe(`tools`, () => {
  test(`every v1 tool is a catalog tool, sorted; only engine tools lack a native schema`, () => {
    const names = CAPABILITY_TOOLS.map((t) => t.name)
    expect(names).toEqual([...names].sort())
    const catalog = CAPABILITIES.flatMap((c) => c.scopes.filter((s) => !s.hostOnly).flatMap((s) => s.tools.map((t) => `${c.name}.${t}`)))
    for (const name of names) expect(catalog).toContain(name)
    for (const t of CAPABILITY_TOOLS) expect(t.nativeOutputSchema === null).toBe(t.runs === `engine`)
  })
  test(`base64 is strict`, () => {
    expect(base64Encode(new TextEncoder().encode(`hello`))).toBe(`aGVsbG8=`)
    for (const bad of [`a b`, `aGk`, `aGk==`, `aGl=`, `a===`, `====`, `aGVsbG8=\n`]) expect(base64Decode(bad)).toBeNull()
    expect(base64Decode(``)).toEqual(new Uint8Array())
  })
  test(`numbers print in their shortest form`, () => {
    expect([12, 12.5, 12.345, 0.004, 730.0000001, 1 / 3].map(pdfNumber)).toEqual([`12`, `12.5`, `12.35`, `0`, `730`, `0.33`])
  })
  test(`a generated notification id is n- + 32 lowercase hex`, () => {
    const a = newNotificationId()
    expect(a).toMatch(/^n-[0-9a-f]{32}$/)
    expect(newNotificationId()).not.toBe(a)
  })
  test(`a post without an id generates one and answers it`, async () => {
    const { store } = fakeBlobs()
    let seen: Record<string, unknown> | null = null
    const out = await bridgeCall({ tool: `notifications.post`, arguments: { title: `Hi` }, context: v1.context, blobs: store, native: async (_, args) => ((seen = args), { structuredContent: {} }) })
    const id = (out.result.structuredContent as { id: string }).id
    expect(id).toMatch(/^n-[0-9a-f]{32}$/)
    expect(seen as unknown as Record<string, unknown>).toEqual({ id: `acme.album/${id}`, title: `Hi`, body: null, at: null, tap: null })
  })
  test(`a throwing native is an rpcError`, async () => {
    const { store } = fakeBlobs()
    const out = await bridgeCall({ tool: `photos.pick`, arguments: {}, context: v1.context, blobs: store, native: async () => Promise.reject(new Error(`gone`)) })
    expect(out.outcome).toBe(`rpcError`)
    expect(out.result.isError).toBe(true)
  })
  for (const c of capabilities.validateCases) {
    test(`validate ${c.tool} ${JSON.stringify(c.arguments)}`, () => expect(validateToolArguments(c.tool, c.arguments)).toEqual(c.expected as never))
  }
  for (const c of capabilities.bridgeCases) {
    test(`bridge: ${c.name}`, async () => {
      const { store, puts } = fakeBlobs((c as { blobs?: Record<string, string> }).blobs)
      let calls = 0
      const native = async (tool: string, args: Record<string, unknown>) => {
        calls++
        expect(tool).toBe(c.tool)
        expect(args).toEqual(c.native as never)
        const answer = c.nativeResult as Record<string, unknown>
        return typeof answer.isError === `string` ? { content: [{ type: `text`, text: answer.isError }], isError: true } : answer
      }
      const out = await bridgeCall({ tool: c.tool, arguments: c.arguments, context: v1.context, blobs: store, native })
      expect(calls).toBe(c.native === null ? 0 : 1)
      expect(puts).toEqual(c.puts)
      const expected = c.expected as { structuredContent?: unknown; isError?: string; outcome?: string }
      if (expected.isError !== undefined) {
        expect(out.result.isError).toBe(true)
        expect(out.result.content).toEqual([{ type: `text`, text: expected.isError }])
        if (expected.outcome) expect(out.outcome).toBe(expected.outcome as never)
      } else {
        expect(out.outcome).toBe(`ok`)
        expect(out.result.isError).toBeUndefined()
        expect(out.result.structuredContent).toEqual(expected.structuredContent)
        expect(out.result.content).toEqual([{ type: `text`, text: JSON.stringify(expected.structuredContent) }])
        expect(schemaViolation(toolDef(c.tool)?.outputSchema as JsonSchema, out.result.structuredContent)).toBeNull()
      }
    })
  }
})

describe(`pdf`, () => {
  const dir = `/tmp/vapp265/pdf-ts`
  mkdirSync(dir, { recursive: true })
  capabilities.pdfCases.forEach((c, n) => {
    test(`pdf ${n}: ${c.name}`, async () => {
      const seed = (c as { blobs?: Record<string, string> }).blobs
      const { store, puts } = fakeBlobs(seed)
      const expected = c.expected as { name?: string; pages?: number; size?: number; sha256?: string; isError?: string }
      const rendered = await renderPdf(c.arguments as PdfArguments, (id) => store.get(v1.context.app, id))
      const out = await bridgeCall({ tool: `print.pdf`, arguments: c.arguments, context: v1.context, blobs: store, native: async () => expect.unreachable() })
      if (expected.isError !== undefined) {
        expect(rendered).toEqual({ error: expected.isError })
        expect(out.result.content).toEqual([{ type: `text`, text: expected.isError }])
        expect(out.outcome).toBe(`isError`)
        expect(puts).toEqual([])
        return
      }
      if (!(`bytes` in rendered)) throw new Error(rendered.error)
      writeFileSync(`${dir}/${n}.pdf`, rendered.bytes)
      const sha256 = createHash(`sha256`).update(rendered.bytes).digest(`hex`)
      console.log(`pdf ${n} ${c.name}: sha256 ${sha256} size ${rendered.bytes.length} pages ${rendered.pages}`)
      expect(rendered.pages).toBe(expected.pages as number)
      if (expected.sha256 !== `TBD`) {
        expect(sha256).toBe(expected.sha256 as string)
        expect(rendered.bytes.length).toBe(expected.size as number)
      }
      expect(out.outcome).toBe(`ok`)
      expect(out.result.structuredContent).toEqual({ file: { name: expected.name, mimeType: `application/pdf`, size: rendered.bytes.length, fileId: `blob-1` }, pages: expected.pages })
      expect(puts).toEqual([{ name: expected.name as string, mimeType: `application/pdf`, base64: base64Encode(rendered.bytes) }])
    })
  })
})

describe(`nativeCaller`, () => {
  test(`routes the full name to the capability's bare tool`, async () => {
    const seen: string[] = []
    const call = nativeCaller([{ name: `photos`, call: async (tool) => (seen.push(tool), { structuredContent: { photos: [] } }) }])
    expect(await call(`photos.pick`, { limit: 1 })).toEqual({ structuredContent: { photos: [] } })
    expect(seen).toEqual([`pick`])
    expect(await call(`share.send`, {})).toEqual({ content: [{ type: `text`, text: `unavailable` }], isError: true })
  })
})
