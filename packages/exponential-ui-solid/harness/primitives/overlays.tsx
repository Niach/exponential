// FEED-131: the `overlays` lane's primitive specimens (Solid = the candidate).
// One entry per `primitive-specimens.json` row whose `primitive` is "overlays";
// the SAME ids in packages/exponential-ui-react/harness/primitives/overlays.tsx.
import type { JSX } from "solid-js"

export const SPECIMENS: Record<string, () => JSX.Element> = {}
