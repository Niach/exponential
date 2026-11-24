// #NEW-134: the server's one door to the embedding model.
//
// The model runs in ONE worker thread (apps/web/embedder/worker.mjs,
// onnxruntime-web's WASM backend), so an inference never runs on the event
// loop. This module owns that worker:
//
//   * lazy: the worker starts (and the 118 MB model loads, ~1 s) on the first
//     embed, never at boot;
//   * one inference at a time: jobs wait in a queue here and reach the worker
//     one by one; a search query jumps ahead of the background passages;
//   * idle: `IDLE_MS` after the last job the worker is terminated, which
//     hands its memory (~0.5 GB while loaded) back to the OS;
//   * never throws: a missing model, a disabled feature, a crashed worker or
//     a full queue all answer `null` and the caller degrades (lexical search,
//     a later sweep).
//
// PRIVACY: no text and no vector leaves this process. The worker reads the
// wasm from node_modules and the weights from local disk (baked into the
// image, sha256-pinned) and opens no socket; the library has no telemetry and
// nothing here looks a model up remotely.
import { existsSync } from "node:fs"
import { join, resolve } from "node:path"
import { Worker } from "node:worker_threads"
import embeddingModel from "../../../embedder/model.json"

const IDLE_MS = 5 * 60 * 1000
const JOB_TIMEOUT_MS = 60 * 1000
const LOAD_TIMEOUT_MS = 120 * 1000
/** After a worker that failed to load or crashed, nothing is tried for this long. */
const BROKEN_COOLDOWN_MS = 5 * 60 * 1000
/** Background passages waiting beyond this are dropped (the sweep finds them again). */
const MAX_QUEUED_PASSAGES = 500
/** A query is a search box's text: anything longer is not one. */
const MAX_QUERY_CHARS = 1000

export type EmbedKind = `query` | `passage`

/** `ISSUE_EMBEDDINGS=off` (or false/0) turns the whole feature off. */
export function issueEmbeddingsDisabled(): boolean {
  const flag = (process.env.ISSUE_EMBEDDINGS ?? ``).trim().toLowerCase()
  return flag === `off` || flag === `false` || flag === `0`
}

interface EmbedderPaths {
  worker: string
  modelDir: string
}

let cachedPaths: { at: number; paths: EmbedderPaths | null } | null = null

/** The worker script and the model directory, or null when either is absent
 *  (a checkout that never ran `bun run embeddings:model`). The embedder
 *  directory is `apps/web/embedder` in a checkout and in the image; the
 *  server's cwd is `apps/web` (dev, tests, scripts) or the image's `/app`. */
function embedderPaths(): EmbedderPaths | null {
  if (cachedPaths && (cachedPaths.paths || Date.now() - cachedPaths.at < 60_000)) return cachedPaths.paths
  let found: EmbedderPaths | null = null
  for (const base of [`embedder`, join(`apps`, `web`, `embedder`)]) {
    const dir = resolve(process.cwd(), base)
    const worker = join(dir, `worker.mjs`)
    if (!existsSync(worker)) continue
    const modelDir = process.env.ISSUE_EMBEDDINGS_MODEL_DIR || join(dir, `models`, `multilingual-e5-small`)
    if (embeddingModel.source.files.every((file) => existsSync(join(modelDir, file.name)))) {
      found = { worker, modelDir }
      break
    }
  }
  cachedPaths = { at: Date.now(), paths: found }
  return found
}

/** True when this process can embed at all: not disabled, model on disk. */
export function embedderConfigured(): boolean {
  return !issueEmbeddingsDisabled() && embedderPaths() !== null
}

interface Job {
  id: number
  text: string
  kind: EmbedKind
  resolve: (vector: Float32Array | null) => void
}

type WorkerMessage =
  | { ready: true; loadMs: number }
  | { fatal: string }
  | { id: number; vector: Float32Array; tokens: number; ms: number }
  | { id: number; error: string }

let worker: Worker | null = null
let workerReady = false
let current: Job | null = null
let currentTimer: ReturnType<typeof setTimeout> | null = null
let idleTimer: ReturnType<typeof setTimeout> | null = null
let brokenUntil = 0
let nextJobId = 1
const queue: Job[] = []

const stats = { embeds: 0, totalMs: 0, loads: 0, lastLoadMs: 0, failures: 0 }
/** For the admin/metrics surface and tests. */
export function embedderStats(): Readonly<typeof stats> & { loaded: boolean; queued: number } {
  return { ...stats, loaded: worker !== null && workerReady, queued: queue.length + (current ? 1 : 0) }
}

function clearTimer(timer: ReturnType<typeof setTimeout> | null): null {
  if (timer) clearTimeout(timer)
  return null
}

function stopWorker(): void {
  currentTimer = clearTimer(currentTimer)
  idleTimer = clearTimer(idleTimer)
  const old = worker
  worker = null
  workerReady = false
  if (old) {
    old.removeAllListeners()
    void old.terminate().catch(() => {})
  }
}

/** The worker is gone or unusable: every waiting job answers null. */
function fail(reason: string): void {
  stats.failures += 1
  brokenUntil = Date.now() + BROKEN_COOLDOWN_MS
  console.error(`[issue-embeddings] embedder stopped: ${reason}`)
  stopWorker()
  const waiting = current ? [current, ...queue] : [...queue]
  current = null
  queue.length = 0
  for (const job of waiting) job.resolve(null)
}

function startWorker(paths: EmbedderPaths): void {
  // The worker resolves the same model directory itself (embedder/model.mjs).
  const started = new Worker(paths.worker)
  // Never keeps a script or a test run alive.
  started.unref()
  worker = started
  workerReady = false
  currentTimer = setTimeout(() => fail(`the model did not load within ${LOAD_TIMEOUT_MS / 1000}s`), LOAD_TIMEOUT_MS)
  currentTimer.unref?.()
  started.on(`message`, (message: WorkerMessage) => {
    if (worker !== started) return
    if (`fatal` in message) return fail(message.fatal)
    if (`ready` in message) {
      workerReady = true
      stats.loads += 1
      stats.lastLoadMs = message.loadMs
      currentTimer = clearTimer(currentTimer)
      pump()
      return
    }
    const job = current
    if (!job || job.id !== message.id) return
    current = null
    currentTimer = clearTimer(currentTimer)
    if (`error` in message) {
      stats.failures += 1
      console.error(`[issue-embeddings] embed failed: ${message.error}`)
      job.resolve(null)
    } else {
      stats.embeds += 1
      stats.totalMs += message.ms
      job.resolve(message.vector)
    }
    pump()
  })
  started.on(`error`, (err) => {
    if (worker === started) fail(String(err))
  })
  started.on(`exit`, (code) => {
    if (worker === started) fail(`worker exited with code ${code}`)
  })
}

function pump(): void {
  if (current) return
  const job = queue.shift()
  if (!job) {
    if (worker && !idleTimer) {
      idleTimer = setTimeout(() => {
        idleTimer = null
        if (!current && queue.length === 0) stopWorker()
      }, IDLE_MS)
      idleTimer.unref?.()
    }
    return
  }
  idleTimer = clearTimer(idleTimer)
  if (!worker) {
    const paths = embedderPaths()
    if (!paths) {
      job.resolve(null)
      return pump()
    }
    queue.unshift(job)
    startWorker(paths)
    return
  }
  if (!workerReady) {
    queue.unshift(job)
    return
  }
  current = job
  currentTimer = setTimeout(() => fail(`an embed took longer than ${JOB_TIMEOUT_MS / 1000}s`), JOB_TIMEOUT_MS)
  currentTimer.unref?.()
  worker.postMessage({ id: job.id, text: job.text })
}

/**
 * Embeds `text` as an e5 `query` or `passage` (the prefix is added here):
 * a 384-dimension, L2-normalised vector, or null when the embedder is off,
 * unconfigured, broken or (passages only) too far behind.
 */
export function embedText(kind: EmbedKind, text: string): Promise<Float32Array | null> {
  if (!embedderConfigured() || Date.now() < brokenUntil) return Promise.resolve(null)
  return new Promise((resolvePromise) => {
    const job: Job = { id: nextJobId++, kind, text: `${kind}: ${text}`, resolve: resolvePromise }
    if (kind === `query`) {
      // Ahead of every passage, behind the queries already waiting.
      const firstPassage = queue.findIndex((queued) => queued.kind === `passage`)
      if (firstPassage === -1) queue.push(job)
      else queue.splice(firstPassage, 0, job)
    } else {
      if (queue.length >= MAX_QUEUED_PASSAGES) return resolvePromise(null)
      queue.push(job)
    }
    pump()
  })
}

/**
 * #NEW-135's entry: the vector of a search query, or null when semantic
 * search cannot answer (the caller keeps its lexical results).
 */
export function embedQuery(text: string): Promise<Float32Array | null> {
  const query = text.replace(/\s+/g, ` `).trim().slice(0, MAX_QUERY_CHARS)
  if (!query) return Promise.resolve(null)
  return embedText(`query`, query)
}

/** Terminates the worker now (tests, scripts, shutdown). */
export function stopEmbedder(): void {
  stopWorker()
  const waiting = current ? [current, ...queue] : [...queue]
  current = null
  queue.length = 0
  for (const job of waiting) job.resolve(null)
}
