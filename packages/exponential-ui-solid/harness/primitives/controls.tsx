// FEED-131: the `controls` lane's primitive specimens (Solid = the candidate).
// One entry per `primitive-specimens.json` row whose `primitive` is "controls";
// the SAME ids in packages/exponential-ui-react/harness/primitives/controls.tsx.
import type { JSX } from "solid-js"

export const SPECIMENS: Record<string, () => JSX.Element> = {}
