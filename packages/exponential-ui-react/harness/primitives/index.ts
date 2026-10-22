// FEED-131: every primitive specimen of this harness, merged from one module
// per lane (so lanes never edit one file): `?view=primitives&id=<id>`
// renders SPECIMENS[id]. Ids = packages/exponential-ui/fixtures/
// primitive-specimens.json; the Solid harness has the twin modules.
import type { ReactElement } from "react"
import { SPECIMENS as smoke } from "./smoke"
import { SPECIMENS as plain } from "./plain"
import { SPECIMENS as controls } from "./controls"
import { SPECIMENS as overlays } from "./overlays"
import { SPECIMENS as data } from "./data"
import { SPECIMENS as calendar } from "./calendar"

export const SPECIMENS: Record<string, () => ReactElement> = { ...smoke, ...plain, ...controls, ...overlays, ...data, ...calendar }
