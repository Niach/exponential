import { TRPCError } from "@trpc/server"
import { CORE_CATALOG_ID, cardProblems, cardV10, reduceNested, type ReduceResult } from "@exponential_app/eui"
import { motionPropsProblems } from "@exponential_app/eui/motion/server"
import { PLAN_CARD_SURFACE_MAX_BYTES, PLAN_CARD_SURFACE_MAX_COMPONENTS, type CardSurface } from "@exp/db-schema/domain"

// A card's surface, checked (VAPP-289 §13). Pure and free of the server's
// modules: the A2UI 1.0 migration script loads it without auth or a pool.

const bad = (message: string) => new TRPCError({ code: `BAD_REQUEST`, message })

export interface CardSurfaceInput {
  version?: string
  root: Record<string, unknown>
  data?: Record<string, unknown>
}

function countNodes(result: ReduceResult): number {
  type Node = { children?: Node[]; slots?: Record<string, Node> }
  const walk = (node: Node): number =>
    1 +
    (node.children ?? []).reduce((sum, child) => sum + walk(child), 0) +
    Object.values(node.slots ?? {}).reduce((sum, slot) => sum + walk(slot), 0)
  return (
    walk(result.root as Node) +
    Object.values(result.templates ?? {}).reduce((sum, node) => sum + walk(node as Node), 0)
  )
}

/** The inline surface: the core catalog only (no extensions), within the
 *  byte and component caps, with no reducer issue. */
export function validateCardSurface(surface: CardSurfaceInput | undefined): CardSurface | null {
  if (!surface) return null
  const bytes = new TextEncoder().encode(JSON.stringify(surface)).byteLength
  if (bytes > PLAN_CARD_SURFACE_MAX_BYTES) {
    throw bad(`surface is ${bytes} bytes; the cap is ${PLAN_CARD_SURFACE_MAX_BYTES}: simplify it`)
  }
  const root = surface.root
  if (typeof root.id !== `string` || typeof root.component !== `string`) {
    throw bad(`surface.root must be a genUI node: {id, component, props?, children?}`)
  }
  // Round 9 §13: a card is 1.0 (a missing `version` names the fix).
  const entry = cardV10(surface)
  if (`error` in entry) throw bad(`surface: ${entry.error}`)
  const problems = cardProblems(entry.card.root)
  if (problems.length > 0) throw bad(`surface is not valid: ${problems.slice(0, 10).join(`; `)}${problems.length > 10 ? `; and ${problems.length - 10} more` : ``}`)
  let result: ReduceResult
  try {
    result = reduceNested(entry.card.root, { catalogId: CORE_CATALOG_ID })
  } catch (error) {
    throw bad(`surface.root is not a valid genUI tree: ${error instanceof Error ? error.message : String(error)}`)
  }
  const count = countNodes(result)
  if (count > PLAN_CARD_SURFACE_MAX_COMPONENTS) {
    throw bad(`surface has ${count} components; the cap is ${PLAN_CARD_SURFACE_MAX_COMPONENTS}`)
  }
  if (result.issues.length > 0) {
    const listed = result.issues.slice(0, 10).map((issue) => `${issue.id}: ${issue.message}`)
    const more = result.issues.length > 10 ? `; and ${result.issues.length - 10} more` : ``
    throw bad(`surface is not valid in the genUI core catalog: ${listed.join(`; `)}${more}`)
  }
  // EXP-1604: a Motion's animation is checked by the engine itself (the
  // WASM): the dialect compiles, Lottie has no expressions and fits the limits.
  const motion = motionProblems(root as unknown as MotionWalkNode)
  if (motion.length > 0) {
    throw bad(`surface has an invalid Motion: ${motion.slice(0, 5).join(`; `)}${motion.length > 5 ? `; and ${motion.length - 5} more` : ``}`)
  }
  return { version: entry.card.version, root, data: surface.data ?? null }
}

type MotionWalkNode = { id?: string; component?: string; props?: Record<string, unknown>; children?: unknown; slots?: Record<string, unknown> }

/** Every Motion node's problems, `<id>: <problem>`. */
export function motionProblems(node: MotionWalkNode): string[] {
  const out: string[] = []
  const walk = (n: MotionWalkNode) => {
    if (n.component === `Motion`) {
      for (const problem of motionPropsProblems(n.props ?? {})) out.push(`${n.id ?? `Motion`}: ${problem}`)
    }
    if (Array.isArray(n.children)) for (const child of n.children) if (child && typeof child === `object`) walk(child as MotionWalkNode)
    for (const slot of Object.values(n.slots ?? {})) if (slot && typeof slot === `object`) walk(slot as MotionWalkNode)
  }
  walk(node)
  return out
}
