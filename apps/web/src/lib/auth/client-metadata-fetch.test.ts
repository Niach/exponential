// @vitest-environment node
import { EventEmitter } from "node:events"
import { Readable } from "node:stream"
import { describe, expect, it } from "vitest"
import { clientMetadataFetch, pinnedAddress } from "./client-metadata-fetch"

const answers = (...addresses: string[]) => async () => addresses.map((address) => ({ address }))

function connection(status: number, body = ``, headers: Record<string, string> = {}) {
  const seen: Record<string, unknown>[] = []
  const connect = ((options: Record<string, unknown>, onResponse: (res: unknown) => void) => {
    seen.push(options)
    const req = new EventEmitter() as EventEmitter & { end: () => void }
    req.end = () => onResponse(Object.assign(Readable.from(body ? [Buffer.from(body)] : []), { statusCode: status, headers }))
    return req
  }) as never
  return { seen, connect }
}

describe(`client metadata fetch (#NEW-141, CIMD)`, () => {
  it(`refuses a host with any non-public answer`, async () => {
    for (const set of [[`10.0.0.5`], [`127.0.0.1`], [`169.254.169.254`], [`::1`], [`93.184.216.34`, `192.168.0.2`], [`::ffff:10.0.0.1`], []]) {
      await expect(pinnedAddress(`evil.example`, answers(...set))).rejects.toThrow(`not a public address`)
    }
    await expect(pinnedAddress(`[::1]`, answers(`1.1.1.1`))).rejects.toThrow(`not a public address`)
  })

  it(`connects to the checked address, in the host's name, resolving once`, async () => {
    let lookups = 0
    const { seen, connect } = connection(200, `{"client_id":"x"}`, { "content-type": `application/json` })
    const fetch = clientMetadataFetch(async () => (lookups++, lookups === 1 ? [{ address: `2606:4700::1` }, { address: `93.184.216.34` }] : [{ address: `10.0.0.5` }]), connect)
    const res = await fetch(`https://client.example/oauth/meta.json?v=1`, { headers: { accept: `application/json` } })
    expect(await res.json()).toEqual({ client_id: `x` })
    expect(res.headers.get(`content-type`)).toBe(`application/json`)
    expect(lookups).toBe(1)
    expect(seen).toHaveLength(1)
    expect(seen[0]).toMatchObject({ host: `93.184.216.34`, servername: `client.example`, path: `/oauth/meta.json?v=1`, headers: { host: `client.example`, accept: `application/json` } })
  })

  it(`refuses redirects, plain http and other methods`, async () => {
    const { connect, seen } = connection(302, ``, { location: `http://10.0.0.5/` })
    const fetch = clientMetadataFetch(answers(`93.184.216.34`), connect)
    await expect(fetch(`https://client.example/a`)).rejects.toThrow(`redirect`)
    await expect(fetch(`http://client.example/a`)).rejects.toThrow(`https`)
    await expect(fetch(`https://u:p@client.example/a`)).rejects.toThrow(`https`)
    await expect(fetch(`https://client.example/a`, { method: `POST` })).rejects.toThrow(`GET`)
    expect(seen).toHaveLength(1)
  })

  it(`passes a 304 on without a body`, async () => {
    const { connect } = connection(304, ``, { etag: `"1"` })
    const res = await clientMetadataFetch(answers(`93.184.216.34`), connect)(`https://client.example/a`)
    expect([res.status, res.body]).toEqual([304, null])
  })
})
