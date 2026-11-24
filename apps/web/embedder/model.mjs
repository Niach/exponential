// #NEW-134: the embedding model this build runs, read from model.json (the
// ONE description: the server bundle imports the same file for the model id).
//
// multilingual-e5-small (intfloat, MIT), the int8-quantized ONNX export from
// the Xenova conversion repo, pinned to one commit and checked by sha256.
// The files are fetched ONCE, at image build (Dockerfile, `fetch-model.mjs`),
// and loaded from disk afterwards: the running server never asks a third
// party for weights, and no text or vector ever leaves it.
//
// Plain .mjs on purpose: the worker and the fetch script run UNBUNDLED (bun in
// the image, node under vitest and `bun dev`), next to the server bundle.
import { readFileSync } from "node:fs"
import { dirname, join } from "node:path"
import { fileURLToPath } from "node:url"

const here = dirname(fileURLToPath(import.meta.url))
const model = JSON.parse(readFileSync(join(here, `model.json`), `utf8`))

/** Stored in `issue_embeddings.model` and mixed into every content hash: a
 *  model, quantization or input-recipe change re-embeds every issue. */
export const EMBEDDING_MODEL_ID = model.id
export const EMBEDDING_DIMENSIONS = model.dimensions
/** The model's position limit, special tokens included. */
export const EMBEDDING_MAX_TOKENS = model.maxTokens
export const EMBEDDING_MODEL_SOURCE = model.source

/** Where the weights live: `apps/web/embedder/models/<name>/` in a checkout
 *  and in the image alike (`ISSUE_EMBEDDINGS_MODEL_DIR` moves it). */
export function embeddingModelDir() {
  return process.env.ISSUE_EMBEDDINGS_MODEL_DIR || join(here, `models`, `multilingual-e5-small`)
}
