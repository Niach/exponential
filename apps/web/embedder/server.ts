// #EXP-2133: the host's one embedder. The same web image with
// `EXPONENTIAL_ROLE=embedder` runs THIS instead of the web server: no database,
// no migrations, no other route. Every web container with ISSUE_EMBEDDER_URL
// asks it, so the model (~0.5 GB) is resident at most once per host, inside
// this container's own memory limit. Lazy load, one inference at a time,
// queries first and the idle unload are `local.ts`, unchanged.
//
//   POST /embed       {kind: "query"|"passage", text, model} + Bearer secret
//                     → 384 little-endian float32; 503 = not now
//   GET  /api/health  {ok: true} (the image's HEALTHCHECK; says nothing else)
//
// Internal only: no published port and no domain. As a second wall a request
// that came through a reverse proxy (X-Forwarded-*, Forwarded, Via) is refused
// before anything is read, so a mistaken public route answers 404.
//
// PRIVACY: texts are never logged, stored or sent anywhere.
import { createHash, timingSafeEqual } from "node:crypto"
import { createServer, type IncomingMessage, type Server, type ServerResponse } from "node:http"
import embeddingModel from "./model.json"
import { localEmbedText, localEmbedderConfigured, stopLocalEmbedder, type EmbedKind } from "./local"

const MIN_SECRET_CHARS = 32
/** `EMBEDDER_MAX_TEXT_CHARS` (64 000) as worst-case JSON-escaped UTF-8. */
const MAX_BODY_BYTES = 64_000 * 6 + 1024

const PROXY_HEADERS = [`x-forwarded-for`, `x-forwarded-host`, `x-forwarded-proto`, `forwarded`, `via`, `cf-ray`]

const digest = (value: string) => createHash(`sha256`).update(value).digest()

function readBody(req: IncomingMessage): Promise<string | null> {
  return new Promise((resolve, reject) => {
    const chunks: Buffer[] = []
    let size = 0
    req.on(`data`, (chunk: Buffer) => {
      size += chunk.length
      if (size <= MAX_BODY_BYTES) chunks.push(chunk)
    })
    req.on(`end`, () => resolve(size <= MAX_BODY_BYTES ? Buffer.concat(chunks).toString(`utf8`) : null))
    req.on(`error`, reject)
  })
}

export interface EmbedderServerOptions {
  secret: string
  /** Injected in tests; defaults to the worker thread. */
  embed?: (kind: EmbedKind, text: string) => Promise<Float32Array | null>
}

export function createEmbedderServer({ secret, embed = localEmbedText }: EmbedderServerOptions): Server {
  if (secret.length < MIN_SECRET_CHARS) throw new Error(`ISSUE_EMBEDDER_SECRET must be at least ${MIN_SECRET_CHARS} characters`)
  const expected = digest(`Bearer ${secret}`)

  const handle = async (req: IncomingMessage, res: ServerResponse): Promise<void> => {
    const end = (status: number, body?: string | Buffer, type = `application/json`) => {
      res.writeHead(status, { "content-type": type, "cache-control": `no-store` })
      res.end(body)
    }
    if (PROXY_HEADERS.some((name) => req.headers[name] !== undefined)) return end(404)
    if (req.method === `GET` && req.url === `/api/health`) return end(200, `{"ok":true}`)
    if (req.method !== `POST` || req.url !== `/embed`) return end(404)
    if (!timingSafeEqual(digest(req.headers.authorization ?? ``), expected)) return end(401)
    const body = await readBody(req)
    if (body === null) return end(413)
    let input: { kind?: unknown; text?: unknown; model?: unknown }
    try {
      input = JSON.parse(body) as typeof input
    } catch {
      return end(400)
    }
    const { kind, text } = input ?? {}
    if ((kind !== `query` && kind !== `passage`) || typeof text !== `string`) return end(400)
    // Two image versions during a rollout: a vector of another model must
    // never be stored under this one's id.
    if (input.model !== embeddingModel.id) return end(409)
    const vector = await embed(kind, text)
    if (!vector) return end(503)
    end(200, Buffer.from(vector.buffer, vector.byteOffset, vector.byteLength), `application/octet-stream`)
  }

  return createServer((req, res) => {
    handle(req, res).catch(() => {
      if (!res.headersSent) res.writeHead(500)
      res.end()
    })
  })
}

/** `bun apps/web/embedder/server.ts` (the image's `embedder` role). */
export function startEmbedderServer(env: Record<string, string | undefined> = process.env): Server {
  const server = createEmbedderServer({ secret: env.ISSUE_EMBEDDER_SECRET?.trim() ?? `` })
  if (!localEmbedderConfigured()) throw new Error(`no embedding model on disk, or ISSUE_EMBEDDINGS is off`)
  const port = Number.parseInt(env.NITRO_PORT || env.PORT || ``) || 3000
  server.listen(port, () => console.log(`[embedder] ${embeddingModel.id} on :${port}, loaded on the first request`))
  for (const signal of [`SIGTERM`, `SIGINT`] as const) {
    process.on(signal, () => {
      stopLocalEmbedder()
      process.exit(0)
    })
  }
  return server
}

// `import.meta.main` (bun): never when a test imports this module.
if ((import.meta as { main?: boolean }).main) startEmbedderServer()
