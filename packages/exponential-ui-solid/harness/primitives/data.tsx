// FEED-131: the `data` lane's primitive specimens (Solid = the candidate).
// One entry per `primitive-specimens.json` row whose `primitive` is "data";
// the SAME ids in packages/exponential-ui-react/harness/primitives/data.tsx.
import type { JSX } from "solid-js"

export const SPECIMENS: Record<string, () => JSX.Element> = {}
