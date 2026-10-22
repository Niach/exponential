// FEED-131: the `calendar` lane's primitive specimens (React = the reference).
// One entry per `primitive-specimens.json` row whose `primitive` is "calendar";
// the SAME ids in packages/exponential-ui-solid/harness/primitives/calendar.tsx.
import type { ReactElement } from "react"

export const SPECIMENS: Record<string, () => ReactElement> = {}
