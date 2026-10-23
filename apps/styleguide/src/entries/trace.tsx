import { TraceSpecimen } from "@exp/ui"

import type { StyleguideEntry } from "./types.ts"

// EXP-1324 (+ EXP-1321): THE Trace, shadcn's Trace on our Collapsible, over
// the contract's pure models (`@exp/domain-contract/trace`, `/task-queue`),
// drawn by `@exp/ui` `TraceSpecimen` (identical ×4; the same board the web
// app's `trace` capture photographs).

export const entry: StyleguideEntry = {
  id: `trace`,
  section: `general`,
  owner: `EXP-1324`,
  title: `Trace`,
  blurb: `shadcn's Trace (radix-nova) ported onto our Collapsible: ONE line = concept icon, title, muted description (tabular figures), size \`default\` (text-sm) or \`sm\` (the session transcript and the issue activity, fixtures/trace.json geometry). \`running\` shimmers the title (the motion.shimmer token; reduce motion = plain text), \`error\` paints the line rose, \`pending\` dims it to 60%. A collapsible line appends the chevron (rotates 90° open) and opens onto a railed column: a command's Output is the CodeBlock at sm (\`ansi\`, destructive when it failed). A subagent's duration is an outline badge. The Queue at a transcript's bottom is two collapsibles: "n to do" open (running work shimmering, pending muted), "n completed" folded. Models: \`traceTool\`, \`traceReasoning\`, \`traceSubagent\`, \`traceFailedTests\`, \`taskQueue\` ×4.`,
  status: {
    web: {
      state: `ok`,
      symbol: `Trace`,
      file: `packages/ui/src/trace.tsx`,
      note: `models: @exp/domain-contract/trace + /task-queue; rows: apps/web/src/components/agent-session.tsx`,
    },
    desktop: {
      state: `ok`,
      symbol: `TraceLine`,
      file: `apps/desktop/crates/steer/src/trace.rs`,
      note: `model; the gpui painter rebuilds the geometry (EXP-1324 desktop lane)`,
    },
    ios: {
      state: `ok`,
      symbol: `TraceLineModel`,
      file: `apps/ios/ExpCore/Sources/Domain/Trace.swift`,
      note: `model; ExpUI paints it (shot sg_trace)`,
    },
    android: {
      state: `ok`,
      symbol: `TraceLineModel`,
      file: `apps/android/app/src/main/java/com/exponential/app/domain/Trace.kt`,
      note: `model; Compose paints it (shot sg_trace)`,
    },
  },
  island: () => <TraceSpecimen />,
}
