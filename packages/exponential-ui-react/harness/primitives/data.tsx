// FEED-131: the `data` lane's primitive specimens (React = the reference).
// One entry per `primitive-specimens.json` row whose `primitive` is "data";
// the SAME ids in packages/exponential-ui-solid/harness/primitives/data.tsx.
import type { ReactElement } from "react"

export const SPECIMENS: Record<string, () => ReactElement> = {}
