// #NEW-134: the server's one door to the embedding model. Two places it runs:
//
//   * default: in this process (`embedder/local.ts`, a worker thread);
//   * `ISSUE_EMBEDDER_URL` set (#EXP-2133): in the host's one embedder
//     (`embedder/server.ts`), shared by every web container. This process then
//     NEVER loads the model itself, also while that embedder is down.
//
// Both answer `null` instead of throwing, and the caller degrades (lexical
// search, a later sweep).
import { issueEmbeddingsDisabled, localEmbedText, localEmbedderConfigured, localEmbedderStats, stopLocalEmbedder, type EmbedKind } from "../../../embedder/local"
import { remoteEmbedText, remoteEmbedder, remoteEmbedderStats, resetRemoteEmbedder } from "./remote"

export { issueEmbeddingsDisabled, type EmbedKind }

/** A query is a search box's text: anything longer is not one. */
const MAX_QUERY_CHARS = 1000

/** True when this process can embed at all: not disabled, and the model on
 *  disk or a usable embedder service configured. */
export function embedderConfigured(): boolean {
  if (issueEmbeddingsDisabled()) return false
  const remote = remoteEmbedder()
  if (remote !== null) return remote.ok
  return localEmbedderConfigured()
}

/** For the admin/metrics surface and tests. `loaded` is this process's worker. */
export function embedderStats(): ReturnType<typeof localEmbedderStats> & { remote: boolean } {
  return remoteEmbedder() === null
    ? { ...localEmbedderStats(), remote: false }
    : { ...localEmbedderStats(), ...remoteEmbedderStats(), remote: true }
}

/**
 * Embeds `text` as an e5 `query` or `passage`: a 384-dimension, L2-normalised
 * vector, or null when the embedder is off, unconfigured, unreachable, broken
 * or (passages only) too far behind.
 */
export function embedText(kind: EmbedKind, text: string): Promise<Float32Array | null> {
  if (issueEmbeddingsDisabled()) return Promise.resolve(null)
  const remote = remoteEmbedder()
  if (remote === null) return localEmbedText(kind, text)
  return remote.ok ? remoteEmbedText(remote, kind, text) : Promise.resolve(null)
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
  stopLocalEmbedder()
  resetRemoteEmbedder()
}
