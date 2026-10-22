// FEED-131: the `controls` lane's primitive specimens (React = the reference).
// One entry per `primitive-specimens.json` row whose `primitive` is "controls";
// the SAME ids in packages/exponential-ui-solid/harness/primitives/controls.tsx.
import type { ReactElement } from "react"

export const SPECIMENS: Record<string, () => ReactElement> = {}
