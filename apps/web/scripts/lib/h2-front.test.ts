// @vitest-environment node
import http, { type IncomingMessage, type ServerResponse } from "node:http"
import http2 from "node:http2"
import type { AddressInfo } from "node:net"
import { afterAll, beforeAll, describe, expect, it } from "vitest"
import { captureBase, forwardRequestHeaders, forwardResponseHeaders, needsH2Front } from "./h2-front"

const TARGET = new URL(`http://localhost:5181`)
const FRONT = `https://localhost:4443`

describe(`needsH2Front`, () => {
  it(`fronts a plain-http base and leaves the Caddy proxy alone`, () => {
    expect(needsH2Front(`http://localhost:5181`)).toBe(true)
    expect(needsH2Front(`https://localhost:3000`)).toBe(false)
  })
})

describe(`forwarded headers`, () => {
  it(`speak for the target's origin upstream`, () => {
    const out = forwardRequestHeaders(
      {
        ":authority": `localhost:4443`,
        host: `localhost:4443`,
        origin: FRONT,
        referer: `${FRONT}/auth/login`,
        connection: `keep-alive`,
        cookie: `a=1`,
      },
      FRONT,
      TARGET
    )
    expect(out).toEqual({
      host: `localhost:5181`,
      origin: `http://localhost:5181`,
      referer: `http://localhost:5181/auth/login`,
      cookie: `a=1`,
    })
  })

  it(`point redirects back at the front and drop hop-by-hop headers`, () => {
    const out = forwardResponseHeaders(
      { location: `http://localhost:5181/t/acme`, "transfer-encoding": `chunked`, "set-cookie": [`s=1`] },
      FRONT,
      TARGET
    )
    expect(out).toEqual({ location: `${FRONT}/t/acme`, "set-cookie": [`s=1`] })
  })
})

// The regression itself: Chromium gives an HTTP/1.1 origin 6 connections and
// Electric's long-polls keep them all, so a client navigation's 7th request
// never starts. Through the front, more than 6 held requests share ONE h2
// connection and a later one still answers at once.
describe(`captureBase`, () => {
  let target: http.Server
  let base = ``
  const held: ServerResponse[] = []

  beforeAll(async () => {
    target = http.createServer((req: IncomingMessage, res: ServerResponse) => {
      if (req.url === `/live`) {
        held.push(res)
        res.writeHead(200)
        return
      }
      res.writeHead(200, { "content-type": `application/json` })
      res.end(JSON.stringify({ host: req.headers.host, origin: req.headers.origin }))
    })
    await new Promise<void>((done) => target.listen(0, `127.0.0.1`, done))
    base = `http://localhost:${(target.address() as AddressInfo).port}`
  })

  afterAll(async () => {
    for (const res of held) res.end()
    await new Promise<void>((done) => target.close(() => done()))
  })

  it(`returns an https base as is`, async () => {
    const same = await captureBase(`https://localhost:3000`)
    expect(same.url).toBe(`https://localhost:3000`)
    await same.close()
  })

  it(`answers a request while 8 long-polls are held, over one h2 connection`, async () => {
    const front = await captureBase(base)
    expect(front.url).toMatch(/^https:\/\/localhost:\d+$/)
    const client = http2.connect(front.url, { rejectUnauthorized: false })
    try {
      for (let i = 0; i < 8; i += 1) client.request({ ":path": `/live` }).on(`error`, () => {})
      await expect.poll(() => held.length).toBe(8)

      const body = await new Promise<string>((done, fail) => {
        const req = client.request({ ":path": `/route-chunk`, origin: front.url })
        let data = ``
        req.setEncoding(`utf8`)
        req.on(`data`, (chunk: string) => (data += chunk))
        req.on(`end`, () => done(data))
        req.on(`error`, fail)
      })
      expect(JSON.parse(body)).toEqual({ host: new URL(base).host, origin: base })
      expect(client.alpnProtocol).toBe(`h2`)
    } finally {
      client.destroy()
      await front.close()
    }
  })
})
