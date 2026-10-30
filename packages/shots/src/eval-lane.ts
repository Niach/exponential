/**
 * VAPP-127: the `eval` lane's pure half. The Exponential UI ergonomics eval
 * (packages/exponential-ui/eval) records ~30 agent-built surfaces; score.ts
 * writes them as native specimens (`eval/eval-specimens.json`, ids
 * `exponential-ui-eval-<corpus id>`) that the Compose example (`--es shot`),
 * the Swift example (`-shot`) and the IDE (`EXP_DEV_SCREEN`) paint by id.
 * This lane photographs them like the `package` lane photographs the SDK
 * specimens, but OUTSIDE the catalog and the store: the pictures go to
 * `eval/out/<id>/<platform>.webp` (gitignored, beside the web renders), the
 * frames each host dumps to `eval/out/<id>/<platform>.frames.json`, and
 * `bun run --filter @exponential_app/eui eval:natives` measures them into
 * `eval/report.json`.
 *
 *   bun run shots -- --platform android --views eval              (every surface)
 *   bun run shots -- --platform ios,desktop --views eval:login,eval:chat --skip-seed
 *
 * The web is not a lane here: `eval:render` paints the web harnesses.
 */
import { readFileSync } from "node:fs"
import { join } from "node:path"
import { rawDir, repoRoot } from "./paths.ts"

/** The natives the lane drives (the web = `eval:render`). */
export const EVAL_PLATFORMS = [`android`, `ios`, `desktop`] as const
export type EvalPlatform = (typeof EVAL_PLATFORMS)[number]

/** `--views eval` = every corpus surface; `eval:<id>` = one. */
export const EVAL_VIEW = `eval`

export const EVAL_SPECIMEN_PREFIX = `exponential-ui-eval-`
/** The id the native hosts load (`eval/eval-specimens.json`). */
export const evalSpecimenId = (corpusId: string): string => `${EVAL_SPECIMEN_PREFIX}${corpusId}`

/** Is this `--views` entry an eval one? */
export const isEvalView = (id: string): boolean => id === EVAL_VIEW || id.startsWith(`${EVAL_VIEW}:`)

/**
 * The corpus ids a `--views` list names, or `null` when it names no eval
 * surface. Eval entries never mix with catalog views (they leave the store
 * alone), and an unknown corpus id is an error, not an empty run.
 */
export function evalScope(viewIds: readonly string[] | undefined, corpusIds: readonly string[]): string[] | null {
  if (!viewIds?.some(isEvalView)) return null
  const other = viewIds.filter((id) => !isEvalView(id))
  if (other.length > 0) {
    throw new Error(`--views mixes eval surfaces with catalog views (${other.join(`, `)}): run them separately`)
  }
  if (viewIds.includes(EVAL_VIEW)) return [...corpusIds]
  const wanted = viewIds.map((id) => id.slice(EVAL_VIEW.length + 1))
  const unknown = wanted.filter((id) => !corpusIds.includes(id))
  if (unknown.length > 0) {
    throw new Error(`unknown eval surface(s): ${unknown.join(`, `)} (packages/exponential-ui/eval/corpus.json ids)`)
  }
  return corpusIds.filter((id) => wanted.includes(id))
}

/** Which natives a run drives: the requested ones, minus the web. */
export function evalPlatforms(requested: readonly string[]): EvalPlatform[] {
  return EVAL_PLATFORMS.filter((p) => requested.includes(p))
}

export function evalDir(): string {
  return join(repoRoot(), `packages/exponential-ui/eval`)
}

/** The corpus ids, in corpus order. */
export function corpusIds(): string[] {
  const corpus = JSON.parse(readFileSync(join(evalDir(), `corpus.json`), `utf8`)) as { prompts: { id: string }[] }
  return corpus.prompts.map((p) => p.id)
}

/** The specimen ids eval-specimens.json holds (what the hosts can paint). */
export function specimenIds(): string[] {
  const file = JSON.parse(readFileSync(join(evalDir(), `eval-specimens.json`), `utf8`)) as { specimens: { id: string }[] }
  return file.specimens.map((s) => s.id)
}

/** The raw capture, under `.shots-raw/eval/` (never a store platform dir). */
export const evalRawPath = (platform: EvalPlatform, corpusId: string): string => join(rawDir(), `eval`, platform, `${corpusId}.png`)
/** The stored picture, beside the web renders. */
export const evalShotPath = (corpusId: string, platform: EvalPlatform): string => join(evalDir(), `out`, corpusId, `${platform}.webp`)
/** The frames the host dumped (`--es frames` / `-frames` / `EXP_DEV_EVAL_FRAMES`). */
export const evalFramesPath = (corpusId: string, platform: EvalPlatform): string => join(evalDir(), `out`, corpusId, `${platform}.frames.json`)
