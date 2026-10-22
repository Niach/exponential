// FEED-131: the `plain` lane's primitive specimens (React = the reference).
// One entry per `primitive-specimens.json` row whose `primitive` is "plain";
// the SAME ids in packages/exponential-ui-solid/harness/primitives/plain.tsx.
import type { ReactElement } from "react"

export const SPECIMENS: Record<string, () => ReactElement> = {}
