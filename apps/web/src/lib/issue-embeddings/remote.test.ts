// @vitest-environment node
//
// #EXP-2133: the host's one embedder (embedder/server.ts) and its client,
// over a real socket with a fake model.
import type { AddressInfo } from "node:net"
import type { Server } from "node:http"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"
import embeddingModel from "../../../embedder/model.json"
import { localEmbedderStats } from "../../../embedder/local"
import { createEmbedderServer } from "../../../embedder/server"
import { embedQuery, embedText, embedderConfigured, stopEmbedder } from "./embedder"
import { isHostLocal, remoteEmbedderFrom } from "./remote"

const SECRET = `s`.repeat(40)
const vectorOf = (value: number) => new Float32Array(embeddingModel.dimensions).fill(value)

let server: Server | null = null
const embed = vi.fn<(kind: string, text: string) => Promise<Float32Array | null>>()

async function listen(): Promise<string> {
  const made = createEmbedderServer({ secret: SECRET, embed })
  server = made
  await new Promise<void>((resolve) => made.listen(0, `127.0.0.1`, resolve))
  return `http://127.0.0.1:${(made.address() as AddressInfo).port}`
}

const post = (url: string, body: unknown, headers: Record<string, string> = {}) =>
  fetch(`${url}/embed`, {
    method: `POST`,
    headers: { "content-type": `application/json`, authorization: `Bearer ${SECRET}`, ...headers },
    body: typeof body === `string` ? body : JSON.stringify(body),
  })
const valid = { kind: `passage`, text: `hello`, model: embeddingModel.id }

beforeEach(() => {
  embed.mockReset()
  embed.mockResolvedValue(vectorOf(0.25))
  vi.spyOn(console, `error`).mockImplementation(() => {})
})
afterEach(async () => {
  delete process.env.ISSUE_EMBEDDER_URL
  delete process.env.ISSUE_EMBEDDER_SECRET
  delete process.env.ISSUE_EMBEDDINGS
  stopEmbedder()
  vi.restoreAllMocks()
  await new Promise<void>((resolve) => (server ? server.close(() => resolve()) : resolve()))
  server = null
})

function useRemote(url: string, secret = SECRET) {
  process.env.ISSUE_EMBEDDER_URL = url
  process.env.ISSUE_EMBEDDER_SECRET = secret
  stopEmbedder()
}

describe(`the embedder server`, () => {
  it(`needs a real secret`, () => {
    expect(() => createEmbedderServer({ secret: `` })).toThrow(/ISSUE_EMBEDDER_SECRET/)
    expect(() => createEmbedderServer({ secret: `short` })).toThrow(/ISSUE_EMBEDDER_SECRET/)
  })

  it(`answers nothing without the secret, and never runs the model for it`, async () => {
    const url = await listen()
    expect((await post(url, valid, { authorization: `Bearer ${`x`.repeat(40)}` })).status).toBe(401)
    expect((await post(url, valid, { authorization: `` })).status).toBe(401)
    expect(embed).not.toHaveBeenCalled()
    expect((await post(url, valid)).status).toBe(200)
  })

  it(`refuses everything that came through a proxy, the health check included`, async () => {
    const url = await listen()
    for (const name of [`x-forwarded-for`, `x-forwarded-host`, `x-forwarded-proto`, `forwarded`, `via`, `cf-ray`]) {
      expect((await post(url, valid, { [name]: `x` })).status, name).toBe(404)
      expect((await fetch(`${url}/api/health`, { headers: { [name]: `x` } })).status, name).toBe(404)
    }
    expect(embed).not.toHaveBeenCalled()
    expect(await (await fetch(`${url}/api/health`)).json()).toEqual({ ok: true })
  })

  it(`refuses bad bodies, other models and other routes`, async () => {
    const url = await listen()
    expect((await post(url, `{`)).status).toBe(400)
    expect((await post(url, `null`)).status).toBe(400)
    expect((await post(url, { ...valid, kind: `other` })).status).toBe(400)
    expect((await post(url, { ...valid, text: 1 })).status).toBe(400)
    expect((await post(url, { ...valid, model: `other/1` })).status).toBe(409)
    expect((await post(url, { ...valid, text: `x`.repeat(500_000) })).status).toBe(413)
    expect((await fetch(`${url}/`)).status).toBe(404)
    expect(embed).not.toHaveBeenCalled()
  })
})

describe(`the client`, () => {
  it(`embeds through the service and loads nothing here`, async () => {
    useRemote(await listen())
    expect(embedderConfigured()).toBe(true)
    expect(await embedText(`passage`, `über 🙂 text`)).toEqual(vectorOf(0.25))
    expect(await embedQuery(`  find   me `)).toEqual(vectorOf(0.25))
    expect(embed.mock.calls).toEqual([[`passage`, `über 🙂 text`], [`query`, `find me`]])
    expect(localEmbedderStats()).toMatchObject({ loaded: false, loads: 0 })
  })

  it(`an unreachable embedder is null at once, then rests; this process still loads nothing`, async () => {
    const url = await listen()
    await new Promise<void>((resolve) => server!.close(() => resolve()))
    server = null
    useRemote(url)
    const fetchSpy = vi.spyOn(globalThis, `fetch`)
    expect(await embedQuery(`login`)).toBeNull()
    expect(await embedQuery(`login`)).toBeNull()
    expect(await embedText(`passage`, `login`)).toBeNull()
    expect(fetchSpy).toHaveBeenCalledTimes(1)
    expect(embedderConfigured()).toBe(true)
    expect(localEmbedderStats()).toMatchObject({ loaded: false, loads: 0, queued: 0 })
  })

  it(`an embedder killed mid-request (OOM) is null`, async () => {
    useRemote(await listen())
    embed.mockImplementation(() => new Promise(() => {}))
    const pending = embedText(`passage`, `login`)
    await vi.waitFor(() => expect(embed).toHaveBeenCalled())
    server!.closeAllConnections()
    expect(await pending).toBeNull()
  })

  it(`a broken worker, a wrong secret and a wrong vector are null`, async () => {
    const url = await listen()
    useRemote(url)
    embed.mockResolvedValue(null)
    expect(await embedText(`passage`, `a`)).toBeNull()
    useRemote(url, `w`.repeat(40))
    expect(await embedText(`passage`, `a`)).toBeNull()
    useRemote(url)
    embed.mockResolvedValue(new Float32Array(3))
    expect(await embedText(`passage`, `a`)).toBeNull()
    useRemote(url)
    embed.mockResolvedValue(vectorOf(1))
    expect(await embedText(`passage`, `a`)).toEqual(vectorOf(1))
  })

  it(`follows no redirect with the text`, async () => {
    const { createServer } = await import(`node:http`)
    let hits = 0
    const redirecting = createServer((req, res) => {
      hits += 1
      res.writeHead(307, { location: `/elsewhere` }).end()
    })
    server = redirecting
    await new Promise<void>((resolve) => redirecting.listen(0, `127.0.0.1`, resolve))
    useRemote(`http://127.0.0.1:${(redirecting.address() as AddressInfo).port}`)
    expect(await embedText(`passage`, `secret text`)).toBeNull()
    expect(hits).toBe(1)
  })

  it(`bounds the passages in flight; queries still pass`, async () => {
    useRemote(await listen())
    let release = () => {}
    const gate = new Promise<void>((resolve) => (release = resolve))
    embed.mockImplementation(async () => (await gate, vectorOf(1)))
    const passages = Array.from({ length: 40 }, (_, i) => embedText(`passage`, `p${i}`))
    const query = embedQuery(`q`)
    release()
    const done = await Promise.all(passages)
    expect(done.filter((v) => v === null)).toHaveLength(8)
    expect(await query).not.toBeNull()
  })

  it(`ISSUE_EMBEDDINGS=off wins over the URL`, async () => {
    useRemote(await listen())
    process.env.ISSUE_EMBEDDINGS = `off`
    expect(embedderConfigured()).toBe(false)
    expect(await embedText(`passage`, `a`)).toBeNull()
    expect(embed).not.toHaveBeenCalled()
  })
})

describe(`the configuration`, () => {
  it(`unset is the in-process embedder`, () => {
    expect(remoteEmbedderFrom({})).toBeNull()
    expect(remoteEmbedderFrom({ ISSUE_EMBEDDER_URL: ` ` })).toBeNull()
  })

  it(`accepts only hosts that cannot be another company's machine`, () => {
    for (const host of [`embedder`, `exp-embedder_1`, `localhost`, `host.docker.internal`, `127.0.0.1`, `10.0.1.4`, `172.20.0.3`, `192.168.1.9`, `[::1]`, `[fd00::1]`]) {
      expect(remoteEmbedderFrom({ ISSUE_EMBEDDER_URL: `http://${host}:3000`, ISSUE_EMBEDDER_SECRET: SECRET }), host).toEqual({
        ok: true,
        url: `http://${host}:3000/embed`,
        secret: SECRET,
      })
    }
    for (const host of [`embed.example.com`, `8.8.8.8`, `172.32.0.1`, `169.254.169.254`, `[2001:db8::1]`, `internal.evil.com`]) {
      expect(remoteEmbedderFrom({ ISSUE_EMBEDDER_URL: `http://${host}`, ISSUE_EMBEDDER_SECRET: SECRET })?.ok, host).toBe(false)
    }
    expect(isHostLocal(`evil.internal.example`)).toBe(false)
  })

  it(`a misconfigured URL or secret is off, never in-process`, async () => {
    for (const [url, secret] of [[`nonsense`, SECRET], [`ftp://embedder`, SECRET], [`http://embedder:3000`, `short`], [`https://embed.example.com`, SECRET]]) {
      useRemote(url!, secret)
      expect(embedderConfigured(), url).toBe(false)
      expect(await embedText(`passage`, `a`)).toBeNull()
    }
    expect(localEmbedderStats().loads).toBe(0)
  })
})
