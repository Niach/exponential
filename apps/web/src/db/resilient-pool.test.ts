import net from "node:net"
import { afterEach, describe, expect, it, vi } from "vitest"
import { freshDnsSocket, isConnectionError, ResilientPool, withConnectRetry } from "./resilient-pool"

const servers: Array<net.Server> = []
afterEach(async () => {
  await Promise.all(servers.splice(0).map((s) => new Promise((r) => s.close(r))))
})

function listen(host: string, port = 0): Promise<{ port: number; accepted: () => number }> {
  let n = 0
  const server = net.createServer((sock) => {
    n++
    sock.destroy()
  })
  servers.push(server)
  return new Promise((resolve, reject) => {
    server.once(`error`, reject)
    server.listen(port, host, () => resolve({ port: (server.address() as net.AddressInfo).port, accepted: () => n }))
  })
}

function dial(socket: net.Socket, port: number, host: string): Promise<string> {
  return new Promise((resolve, reject) => {
    socket.once(`connect`, () => {
      resolve(socket.remoteAddress ?? ``)
      socket.destroy()
    })
    socket.once(`error`, reject)
    socket.connect(port, host)
  })
}

const errWithCode = (code: string, message = code) => Object.assign(new Error(message), { code })

describe(`freshDnsSocket`, () => {
  it(`resolves the host on every connect, so a DB that moved IPs is dialed at its new address`, async () => {
    // Same port on two loopback addresses = the DB container before and after a recreate.
    const before = await listen(`127.0.0.1`)
    const after = await listen(`127.0.0.2`, before.port)
    let current = `127.0.0.1`
    const resolver = vi.fn(async () => current)

    expect(await dial(freshDnsSocket(resolver), before.port, `postgres`)).toBe(`127.0.0.1`)
    current = `127.0.0.2`
    expect(await dial(freshDnsSocket(resolver), before.port, `postgres`)).toBe(`127.0.0.2`)

    expect(resolver).toHaveBeenCalledTimes(2)
    expect(resolver).toHaveBeenCalledWith(`postgres`)
    expect(after.accepted()).toBe(1)
  })

  it(`passes IPs through without resolving`, async () => {
    const srv = await listen(`127.0.0.1`)
    const resolver = vi.fn(async () => `10.255.255.1`)
    expect(await dial(freshDnsSocket(resolver), srv.port, `127.0.0.1`)).toBe(`127.0.0.1`)
    expect(resolver).not.toHaveBeenCalled()
  })

  it(`a failed lookup surfaces as a socket error`, async () => {
    const resolver = async () => {
      throw errWithCode(`EAI_AGAIN`)
    }
    await expect(dial(freshDnsSocket(resolver), 5432, `postgres`)).rejects.toMatchObject({ code: `EAI_AGAIN` })
  })
})

describe(`isConnectionError`, () => {
  it.each([
    errWithCode(`ECONNREFUSED`),
    errWithCode(`EHOSTUNREACH`),
    errWithCode(`ETIMEDOUT`),
    errWithCode(`57P01`, `terminating connection due to administrator command`),
    errWithCode(`57P03`, `the database system is starting up`),
    new Error(`Connection terminated unexpectedly`),
    new Error(`timeout expired`),
  ])(`connection-class: %s`, (err) => expect(isConnectionError(err)).toBe(true))

  it.each([
    errWithCode(`23505`, `duplicate key value violates unique constraint`),
    errWithCode(`42P01`, `relation "x" does not exist`),
    new Error(`boom`),
    `ECONNREFUSED`,
  ])(`not connection-class: %s`, (err) => expect(isConnectionError(err)).toBe(false))
})

describe(`withConnectRetry`, () => {
  const opts = (sleeps: Array<number>) => ({
    attempts: 4,
    baseDelayMs: 100,
    maxDelayMs: 250,
    sleep: async (ms: number) => void sleeps.push(ms),
  })

  it(`retries connection errors with capped exponential backoff, then succeeds`, async () => {
    const sleeps: Array<number> = []
    const fn = vi
      .fn<() => Promise<string>>()
      .mockRejectedValueOnce(errWithCode(`ECONNREFUSED`))
      .mockRejectedValueOnce(errWithCode(`EHOSTUNREACH`))
      .mockRejectedValueOnce(new Error(`Connection terminated unexpectedly`))
      .mockResolvedValue(`client`)
    expect(await withConnectRetry(fn, opts(sleeps))).toBe(`client`)
    expect(fn).toHaveBeenCalledTimes(4)
    expect(sleeps).toHaveLength(3)
    // 100, 200, 250 (capped), each + up to 20% jitter
    ;[100, 200, 250].forEach((base, i) => {
      expect(sleeps[i]).toBeGreaterThanOrEqual(base)
      expect(sleeps[i]).toBeLessThanOrEqual(base * 1.2)
    })
  })

  it(`is bounded: gives up after the last attempt with the last error`, async () => {
    const fn = vi.fn(async () => {
      throw errWithCode(`ECONNREFUSED`)
    })
    await expect(withConnectRetry(fn, opts([]))).rejects.toMatchObject({ code: `ECONNREFUSED` })
    expect(fn).toHaveBeenCalledTimes(4)
  })

  it(`never retries a statement error`, async () => {
    const fn = vi.fn(async () => {
      throw errWithCode(`23505`)
    })
    await expect(withConnectRetry(fn, opts([]))).rejects.toMatchObject({ code: `23505` })
    expect(fn).toHaveBeenCalledTimes(1)
  })
})

describe(`ResilientPool`, () => {
  it(`retries the connect phase until the DB is back (pool.query path)`, async () => {
    // Nothing listens here at first: ECONNREFUSED. pg itself is the client, so
    // we only assert the connect attempts happened through the fresh resolver.
    const resolver = vi.fn(async () => `127.0.0.1`)
    const closed = await listen(`127.0.0.1`)
    const port = closed.port
    await new Promise((r) => servers.pop()!.close(r))
    const retries: Array<number> = []
    const pool = new ResilientPool(
      { host: `postgres`, port, stream: () => freshDnsSocket(resolver), connectionTimeoutMillis: 1000 },
      { attempts: 3, baseDelayMs: 1, maxDelayMs: 2, onRetry: (_e, n) => retries.push(n) }
    )
    await expect(pool.query(`select 1`)).rejects.toMatchObject({ code: `ECONNREFUSED` })
    expect(retries).toEqual([1, 2])
    expect(resolver).toHaveBeenCalledTimes(3)
    await pool.end()
  })
})
