// FEED-130: the side-effect entry: importing it registers
// `<exponential-surface>` (idempotent) and re-exports the package.
import { define } from "./index"

define()

export * from "./index"
