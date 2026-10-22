// FEED-131: the `overlays` lane's primitive specimens (React = the reference).
// One entry per `primitive-specimens.json` row whose `primitive` is "overlays";
// the SAME ids in packages/exponential-ui-solid/harness/primitives/overlays.tsx.
import type { ReactElement } from "react"

export const SPECIMENS: Record<string, () => ReactElement> = {}
