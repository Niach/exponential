#!/usr/bin/env bun
// EXP-1474: prove a BUILT web server actually serves. The trace repairs in
// fix-server-trace.ts are the third family of "green build, dead server"
// (react-dom bun entry, unbound SSR util, duplicate @noble/hashes): every
// route failed while the gate, the image build AND a status-only health
// check stayed green, because /api/health answered 200 with the error body.
//
// Probes `/`, `/auth/login` (a real HTML page: status 200 after redirects, an
// <html> document that loads our /assets/) and `/api/health` (JSON with
// `ok: true`, the database answering). Exits 1 when any probe misses.
//
//   bun scripts/smoke-server.ts [baseUrl]   probe a running server (the
//                                           image in build-web.yml)
//   bun scripts/smoke-server.ts --boot      boot .output/server/index.mjs
//                                           with bun, like the image, on a
//                                           free port, probe it, stop it
//
// The server needs DATABASE_URL + BETTER_AUTH_SECRET (the image's minimum).

import { join } from "node:path"

const WAIT_MS = Number.parseInt(process.env.SMOKE_WAIT_MS || ``) || 90_000

type Probe = { path: string; check: (res: Response, body: string) => string | null }

const page = (res: Response, body: string) => {
  if (res.status !== 200) return `status ${res.status}`
  if (!/<html[\s>]/i.test(body)) return `no <html> document`
  if (!body.includes(`/assets/`)) return `no /assets/ reference (not our app shell)`
  return null
}

const PROBES: Probe[] = [
  { path: `/`, check: page },
  { path: `/auth/login`, check: page },
  {
    path: `/api/health`,
    check: (res, body) => {
      if (res.status !== 200) return `status ${res.status}`
      try {
        return (JSON.parse(body) as { ok?: unknown }).ok === true ? null : `ok is not true`
      } catch {
        return `not JSON`
      }
    },
  },
]

async function waitForListen(base: string, server?: Bun.Subprocess) {
  const deadline = Date.now() + WAIT_MS
  while (Date.now() < deadline) {
    if (server?.exitCode !== undefined && server.exitCode !== null) {
      throw new Error(`the server exited with ${server.exitCode} before listening`)
    }
    try {
      await fetch(`${base}/api/health`, { signal: AbortSignal.timeout(2000) })
      return
    } catch {
      await Bun.sleep(1000)
    }
  }
  throw new Error(`nothing listened on ${base} within ${WAIT_MS / 1000}s`)
}

async function probe(base: string) {
  let failed = 0
  for (const { path, check } of PROBES) {
    let miss: string | null
    let body = ``
    try {
      const res = await fetch(`${base}${path}`, { signal: AbortSignal.timeout(15_000) })
      body = await res.text()
      miss = check(res, body)
    } catch (e) {
      miss = `request failed: ${e instanceof Error ? e.message : String(e)}`
    }
    if (miss) {
      failed += 1
      console.error(`[smoke-server] FAIL ${path}: ${miss}\n${body.slice(0, 2000)}`)
    } else {
      console.log(`[smoke-server] ok   ${path}`)
    }
  }
  return failed
}

async function main() {
  const args = process.argv.slice(2)
  let server: Bun.Subprocess | undefined
  let base = args.find((arg) => !arg.startsWith(`--`)) ?? `http://localhost:3000`
  if (args.includes(`--boot`)) {
    const port = 20_000 + Math.floor(Math.random() * 20_000)
    base = `http://127.0.0.1:${port}`
    const webRoot = join(import.meta.dir, `..`)
    server = Bun.spawn([process.execPath, `.output/server/index.mjs`], {
      cwd: webRoot,
      env: { ...process.env, NODE_ENV: `production`, PORT: String(port), NITRO_PORT: String(port) },
      stdout: `inherit`,
      stderr: `inherit`,
    })
  }
  base = base.replace(/\/$/, ``)
  try {
    await waitForListen(base, server)
    const failed = await probe(base)
    if (failed > 0) {
      console.error(`[smoke-server] ${failed} of ${PROBES.length} probes failed on ${base}`)
      process.exitCode = 1
    }
  } finally {
    server?.kill()
  }
}

main().catch((e) => {
  console.error(`[smoke-server] ${e instanceof Error ? e.message : String(e)}`)
  process.exit(1)
})
