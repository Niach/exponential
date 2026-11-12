import dns from "node:dns"
import net from "node:net"
import { Pool, type PoolClient, type PoolConfig } from "pg"

// A DB restart must not need a web restart (2026-11-12 incident: Postgres was
// recreated with a new container IP and the web app kept dialing the old one
// until `docker restart`). Three pieces:
//
// 1. Fresh DNS on every connect. Bun's `node:dns` (and so `net.connect`) goes
//    through c-ares, whose query cache keeps an answer for its record TTL:
//    Docker's embedded DNS answers with TTL 600, so every new pool connection
//    kept dialing the dead IP for ten minutes, and `BUN_CONFIG_DNS_TIME_TO_LIVE_SECONDS`
//    does not reach that cache. `freshDnsSocket` resolves the host through libc
//    getaddrinfo (Node's `dns.lookup` is uncached already) right before each
//    connect and dials its addresses in order until one accepts; TLS keeps the hostname (pg sets `servername`
//    from the configured host, not the socket's address).
// 2. Bounded retry with backoff on the CONNECT phase only (nothing has been
//    sent yet, so it is always safe; a statement is never re-run).
// 3. Any connection-class error drops every idle pooled connection, so the
//    next query dials (and resolves) anew instead of reusing a socket to the
//    old server.

type Resolver = (host: string) => Promise<Array<string>>

type BunDns = {
  lookup: (host: string, opts: { backend: string }) => Promise<Array<{ address: string }>>
}

const notFound = (host: string) =>
  Object.assign(new Error(`getaddrinfo ENOTFOUND ${host}`), { code: `ENOTFOUND` })

/**
 * Resolve `host` to EVERY address, in resolver order, without any in-process
 * cache. All of them, not the first: `localhost` answers `::1` first on most
 * hosts while Postgres often listens on IPv4 only (the #2715 regression).
 */
export const resolveFresh: Resolver = async (host) => {
  if (net.isIP(host)) return [host]
  const bunDns = (globalThis as { Bun?: { dns?: BunDns } }).Bun?.dns
  const found = bunDns
    ? (await bunDns.lookup(host, { backend: `libc` })).map((r) => r.address)
    : (await dns.promises.lookup(host, { all: true })).map((r) => r.address)
  const addresses = [...new Set(found)]
  if (addresses.length === 0) throw notFound(host)
  return addresses
}

/** Dial errors that mean "this address is no good, try the next one". */
const FALL_THROUGH_CODES = new Set([`ECONNREFUSED`, `EHOSTUNREACH`, `ENETUNREACH`, `EADDRNOTAVAIL`, `EAFNOSUPPORT`])

/**
 * A socket for pg's `stream` option: `connect(port, host)` resolves `host`
 * fresh first, then dials each address in order until one accepts (a refused
 * or unreachable address falls through to the next; the last error surfaces).
 * Unix-socket paths and IPs pass straight through.
 */
export function freshDnsSocket(resolve: Resolver = resolveFresh): net.Socket {
  const socket = new net.Socket()
  const connect = socket.connect.bind(socket) as (...args: Array<unknown>) => net.Socket
  ;(socket as unknown as { connect: (...args: Array<unknown>) => net.Socket }).connect = (...args) => {
    const [port, host] = args
    if (typeof host !== `string` || net.isIP(host) || host.startsWith(`/`)) return connect(...args)
    resolve(host).then(
      (addresses) => {
        if (addresses.length === 0) return socket.destroy(notFound(host))
        if (!socket.destroyed) dialInOrder(socket, connect, port, addresses)
      },
      (err: unknown) => socket.destroy(err instanceof Error ? err : new Error(String(err)))
    )
    return socket
  }
  return socket
}

/**
 * Dial `addresses` one by one on the SAME socket (pg holds this object). While
 * a fallback remains, a fall-through dial error and the `close` that follows
 * it are kept from the socket's listeners, and the socket reconnects to the
 * next address once closed (Node and Bun both allow reconnecting after close).
 */
function dialInOrder(
  socket: net.Socket,
  connect: (...args: Array<unknown>) => net.Socket,
  port: unknown,
  addresses: Array<string>
): void {
  let next = 0
  let retrying = false
  const emit = socket.emit.bind(socket) as (event: string | symbol, ...args: Array<unknown>) => boolean
  const restore = () => {
    ;(socket as unknown as { emit: typeof emit }).emit = emit
  }
  ;(socket as unknown as { emit: typeof emit }).emit = (event, ...args) => {
    if (event === `connect`) restore()
    else if (event === `error` && next < addresses.length) {
      const code = (args[0] as { code?: unknown } | undefined)?.code
      if (typeof code === `string` && FALL_THROUGH_CODES.has(code)) {
        retrying = true
        return true
      }
      restore()
    } else if (event === `close` && retrying) {
      retrying = false
      connect(port, addresses[next++])
      return true
    }
    return emit(event, ...args)
  }
  connect(port, addresses[next++])
}

const CONNECTION_ERROR_CODES = new Set([
  `ECONNREFUSED`,
  `ECONNRESET`,
  `EHOSTUNREACH`,
  `ENETUNREACH`,
  `ETIMEDOUT`,
  `EPIPE`,
  `ENOTFOUND`,
  `EAI_AGAIN`,
  // Postgres: admin_shutdown, crash_shutdown, cannot_connect_now (starting up)
  `57P01`,
  `57P02`,
  `57P03`,
  // connection_exception class
  `08000`,
  `08001`,
  `08003`,
  `08006`,
])

/** True for errors that mean "the connection/server is gone", not "the statement failed". */
export function isConnectionError(err: unknown): boolean {
  if (!(err instanceof Error)) return false
  const code = (err as { code?: unknown }).code
  if (typeof code === `string` && CONNECTION_ERROR_CODES.has(code)) return true
  return /Connection terminated|timeout expired|Connection ended|connect ECONNREFUSED/i.test(err.message)
}

export type RetryOptions = {
  /** Total connect attempts (first try included). */
  attempts: number
  baseDelayMs: number
  maxDelayMs: number
  sleep?: (ms: number) => Promise<void>
  onRetry?: (err: unknown, attempt: number, delayMs: number) => void
}

export const DEFAULT_RETRY: RetryOptions = { attempts: 5, baseDelayMs: 100, maxDelayMs: 2000 }

/** Retry `fn` on connection errors with exponential backoff (+ up to 20% jitter). */
export async function withConnectRetry<T>(fn: () => Promise<T>, opts: RetryOptions = DEFAULT_RETRY): Promise<T> {
  const sleep = opts.sleep ?? ((ms: number) => new Promise<void>((r) => setTimeout(r, ms)))
  for (let attempt = 1; ; attempt++) {
    try {
      return await fn()
    } catch (err) {
      if (attempt >= opts.attempts || !isConnectionError(err)) throw err
      const base = Math.min(opts.maxDelayMs, opts.baseDelayMs * 2 ** (attempt - 1))
      const delay = Math.round(base * (1 + Math.random() * 0.2))
      opts.onRetry?.(err, attempt, delay)
      await sleep(delay)
    }
  }
}

type ConnectCallback = (err: Error | undefined, client?: PoolClient, done?: (release?: unknown) => void) => void

/**
 * pg Pool with fresh DNS per connection, connect-phase retry and idle eviction
 * on connection errors. Drop-in for `new Pool(...)`.
 */
export class ResilientPool extends Pool {
  readonly #retry: RetryOptions
  readonly #all = new Set<PoolClient>()
  readonly #busy = new Set<PoolClient>()

  constructor(config: PoolConfig, retry: RetryOptions = DEFAULT_RETRY) {
    super({
      // A connect to a black-holed address fails in seconds instead of hanging
      // until the kernel gives up; keepalive surfaces half-open sockets.
      connectionTimeoutMillis: 5000,
      keepAlive: true,
      keepAliveInitialDelayMillis: 10_000,
      stream: () => freshDnsSocket(),
      ...config,
    })
    this.#retry = retry
    this.on(`connect`, (c) => this.#all.add(c))
    this.on(`remove`, (c) => {
      this.#all.delete(c)
      this.#busy.delete(c)
    })
    this.on(`acquire`, (c) => this.#busy.add(c))
    this.on(`release`, (err: Error | undefined, c: PoolClient) => {
      this.#busy.delete(c)
      if (err && isConnectionError(err)) this.evictIdle()
    })
    // An idle client's socket dying (DB restart) arrives here; pg-pool has
    // already dropped that client. Without a listener it would crash the process.
    this.on(`error`, (err) => {
      if (isConnectionError(err)) this.evictIdle()
    })
  }

  /** Close every idle pooled connection; the pool removes them and dials anew on demand. */
  evictIdle(): number {
    let n = 0
    for (const c of this.#all) {
      if (this.#busy.has(c)) continue
      const stream = (c as unknown as { connection?: { stream?: net.Socket } }).connection?.stream
      if (stream && !stream.destroyed) {
        stream.destroy()
        n++
      }
    }
    return n
  }

  override connect(): Promise<PoolClient>
  override connect(callback: ConnectCallback): void
  override connect(callback?: ConnectCallback): Promise<PoolClient> | void {
    const attempt = () => super.connect()
    const p = withConnectRetry(attempt, {
      ...this.#retry,
      onRetry: (err, n, delay) => {
        this.evictIdle()
        this.#retry.onRetry?.(err, n, delay)
      },
    })
    if (!callback) return p
    p.then(
      (client) => callback(undefined, client, (release) => client.release(release as Error | boolean | undefined)),
      (err: Error) => callback(err)
    )
  }
}
