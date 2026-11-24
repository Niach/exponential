// #NEW-134: measures the embedder where it actually runs (the web image:
// `docker run --rm --network none <image> bun apps/web/embedder/bench.mjs`).
// Prints load time, ms per embed for a short and a 512-token input, the
// process RSS (before, loaded, after the worker was terminated), the longest
// event-loop stall while the worker loaded and computed, and the DE/EN cosine
// similarities the kNN search rests on.
import { Worker } from "node:worker_threads"
import { EMBEDDING_MODEL_ID } from "./model.mjs"

const rssMb = () => Math.round(process.memoryUsage().rss / 1048576)
const rssBefore = rssMb()

// The event loop's worst stall per phase: a 5 ms tick that reports how late
// it fired (this thread only starts the worker and waits for it).
const stalls = { load: 0, embed: 0 }
let phase = `load`
let last = performance.now()
const ticker = setInterval(() => {
  const now = performance.now()
  stalls[phase] = Math.max(stalls[phase], now - last - 5)
  last = now
}, 5)

const worker = new Worker(new URL(`./worker.mjs`, import.meta.url))
const pending = new Map()
let nextId = 1
const ready = new Promise((resolve, reject) => {
  worker.on(`message`, (message) => {
    if (message.ready) return resolve(message.loadMs)
    if (message.fatal) return reject(new Error(message.fatal))
    pending.get(message.id)?.(message)
    pending.delete(message.id)
  })
  worker.on(`error`, reject)
})
const embed = (text) =>
  new Promise((resolve, reject) => {
    const id = nextId++
    pending.set(id, (message) => (message.error ? reject(new Error(message.error)) : resolve(message)))
    worker.postMessage({ id, text })
  })
const cosine = (a, b) => a.reduce((sum, value, i) => sum + value * b[i], 0)
const median = (values) => [...values].sort((a, b) => a - b)[Math.floor(values.length / 2)]

const loadMs = await ready
phase = `embed`
const short = `passage: Login button does nothing on Safari\n\nClicking "Sign in" on the login page has no effect in Safari 18.`
const long = `passage: ${`The session list reorders itself while a device reconnects and the selected row jumps. `.repeat(60)}`

await embed(short)
const shortRuns = []
const longRuns = []
let longTokens = 0
let shortTokens = 0
for (let i = 0; i < 10; i++) {
  const result = await embed(short)
  shortRuns.push(result.ms)
  shortTokens = result.tokens
}
for (let i = 0; i < 5; i++) {
  const result = await embed(long)
  longRuns.push(result.ms)
  longTokens = result.tokens
}

const sentences = {
  en_bug: `passage: Login button does nothing on Safari`,
  de_bug: `passage: Der Anmelde-Knopf reagiert in Safari nicht`,
  en_other: `passage: Export the board as a CSV file`,
  de_other: `passage: Dunkles Farbschema für die Einstellungen`,
  query_de: `query: Anmeldung funktioniert nicht im Browser`,
}
const vectors = {}
for (const [name, text] of Object.entries(sentences)) vectors[name] = (await embed(text)).vector

clearInterval(ticker)
const rssLoaded = rssMb()
await worker.terminate()
await new Promise((resolve) => setTimeout(resolve, 2000))
globalThis.Bun?.gc(true)
console.log(
  JSON.stringify(
    {
      model: EMBEDDING_MODEL_ID,
      runtime: process.versions.bun ? `bun ${process.versions.bun}` : `node ${process.versions.node}`,
      loadMs,
      shortEmbed: { tokens: shortTokens, medianMs: Math.round(median(shortRuns)) },
      longEmbed: { tokens: longTokens, medianMs: Math.round(median(longRuns)) },
      rssMb: { beforeLoad: rssBefore, loaded: rssLoaded, afterUnload: rssMb() },
      worstEventLoopStallMs: { whileLoading: Math.round(stalls.load), whileEmbedding: Math.round(stalls.embed) },
      cosine: {
        "en_bug~de_bug": +cosine(vectors.en_bug, vectors.de_bug).toFixed(3),
        "en_bug~en_other": +cosine(vectors.en_bug, vectors.en_other).toFixed(3),
        "query_de~en_bug": +cosine(vectors.query_de, vectors.en_bug).toFixed(3),
        "query_de~de_bug": +cosine(vectors.query_de, vectors.de_bug).toFixed(3),
        "query_de~en_other": +cosine(vectors.query_de, vectors.en_other).toFixed(3),
        "query_de~de_other": +cosine(vectors.query_de, vectors.de_other).toFixed(3),
      },
    },
    null,
    2
  )
)
