// EXP-375 — third-party notice generation.
//
// Nothing in the running application imports this package; it is a build-time
// pipeline whose product is five committed NOTICES.txt files. The exports exist
// so `apps/web/src/lib/licenses.test.ts` and the collectors share one definition
// of the wire format, the election policy and the text normalisation rules.

export * from "./schema"
export * from "./spdx"
export * from "./text"
export * from "./render"
