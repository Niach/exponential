// #NEW-134: the embedding worker. One `worker_threads` thread owns the ONNX
// session (onnxruntime-web's WASM backend: the web image is bun on musl, where
// onnxruntime-node has no binary) and the tokenizer, and answers one request
// at a time, so an inference never runs on the server's event loop.
//
// Unbundled .mjs, started by src/lib/issue-embeddings/embedder.ts. Everything
// it reads is on local disk: the wasm from node_modules/onnxruntime-web, the
// weights from `embeddingModelDir()`. It opens no socket.
//
// in:  { id, text }                (the caller already added `query: ` / `passage: `)
// out: { id, vector: Float32Array, tokens, ms } | { id, error }
//      { ready: true, loadMs } once, after the model loaded (or { fatal })
import { readFile } from "node:fs/promises"
import { join } from "node:path"
import { parentPort } from "node:worker_threads"
import * as ort from "onnxruntime-web"
import { buildTokenizer, encodeForModel } from "./tokenizer.mjs"
import { EMBEDDING_DIMENSIONS, EMBEDDING_MAX_TOKENS, EMBEDDING_MODEL_SOURCE, embeddingModelDir } from "./model.mjs"

// One intra-op thread: the embedder is background work beside a web server,
// and a second level of wasm threads buys little at this model size.
ort.env.wasm.numThreads = 1
ort.env.wasm.proxy = false
ort.env.logLevel = `error`

async function load() {
  const dir = embeddingModelDir()
  const [modelName, tokenizerName] = EMBEDDING_MODEL_SOURCE.files.map((file) => file.name)
  // XLM-R: <s> = 0, </s> = 2; the post-processor adds <s> … </s>.
  const tokenizer = buildTokenizer(JSON.parse(await readFile(join(dir, tokenizerName), `utf8`)), {
    maxTokens: EMBEDDING_MAX_TOKENS,
  })
  const session = await ort.InferenceSession.create(await readFile(join(dir, modelName)), {
    executionProviders: [`wasm`],
    graphOptimizationLevel: `all`,
  })
  return { tokenizer, session }
}

function embed({ tokenizer, session }, text) {
  const ids = encodeForModel(tokenizer, text, EMBEDDING_MAX_TOKENS)
  const length = ids.length
  const dims = [1, length]
  const feeds = {
    input_ids: new ort.Tensor(`int64`, BigInt64Array.from(ids, (id) => BigInt(id)), dims),
    attention_mask: new ort.Tensor(`int64`, new BigInt64Array(length).fill(1n), dims),
  }
  if (session.inputNames.includes(`token_type_ids`)) {
    feeds.token_type_ids = new ort.Tensor(`int64`, new BigInt64Array(length), dims)
  }
  return session.run(feeds).then((outputs) => {
    // last_hidden_state [1, length, 384] → mean over the tokens (one
    // sequence, no padding, so the attention mask is all ones) → L2 norm.
    const hidden = outputs.last_hidden_state ?? outputs[session.outputNames[0]]
    const data = hidden.data
    const vector = new Float32Array(EMBEDDING_DIMENSIONS)
    for (let token = 0; token < length; token++) {
      const offset = token * EMBEDDING_DIMENSIONS
      for (let d = 0; d < EMBEDDING_DIMENSIONS; d++) vector[d] += data[offset + d]
    }
    let norm = 0
    for (let d = 0; d < EMBEDDING_DIMENSIONS; d++) {
      vector[d] /= length
      norm += vector[d] * vector[d]
    }
    norm = Math.sqrt(norm) || 1
    for (let d = 0; d < EMBEDDING_DIMENSIONS; d++) vector[d] /= norm
    return { vector, tokens: length }
  })
}

const startedAt = performance.now()
let loaded
try {
  loaded = await load()
  // The parsed tokenizer.json (17 MB of JSON) and the model file's buffer
  // (118 MB) are garbage now: collect once, so they are not resident for as
  // long as the worker lives.
  globalThis.Bun?.gc(true)
  parentPort.postMessage({ ready: true, loadMs: Math.round(performance.now() - startedAt) })
} catch (err) {
  parentPort.postMessage({ fatal: String(err?.stack ?? err) })
}

// Strictly one inference at a time, in arrival order: the main thread's queue
// decides the order, this chain only guarantees no two runs overlap.
let chain = Promise.resolve()
parentPort.on(`message`, ({ id, text }) => {
  chain = chain.then(async () => {
    if (!loaded) {
      parentPort.postMessage({ id, error: `model not loaded` })
      return
    }
    const t0 = performance.now()
    try {
      const { vector, tokens } = await embed(loaded, text)
      parentPort.postMessage({ id, vector, tokens, ms: performance.now() - t0 }, [vector.buffer])
    } catch (err) {
      parentPort.postMessage({ id, error: String(err?.message ?? err) })
    }
  })
})
