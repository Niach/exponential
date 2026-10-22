// FEED-131: the `plain` lane's primitive specimens (Solid = the candidate).
// One entry per `primitive-specimens.json` row whose `primitive` is "plain";
// the SAME ids in packages/exponential-ui-react/harness/primitives/plain.tsx.
import type { JSX } from "solid-js"

export const SPECIMENS: Record<string, () => JSX.Element> = {}
