import { CodeBlockSpecimen } from "@exp/ui"

import type { StyleguideEntry } from "./types.ts"

// EXP-1324: THE CodeBlock, shadcn's code-block (radix-nova) on our Button,
// coloured by the catalog's ONE tokenizer (`tokenizeCode`), drawn by
// `@exp/ui` `CodeBlockSpecimen` (identical ×4; the same blocks the web
// app's `code-block` capture photographs).

export const entry: StyleguideEntry = {
  id: `code-block`,
  section: `general`,
  owner: `EXP-1324`,
  title: `Code block`,
  blurb: `shadcn's code-block (radix-nova): a radius-lg box with an input-coloured hairline over the input fill at 30%, a header (the title, sans medium, and the ghost icon-xs copy button: copy → check for 2 s), then the mono code scrolling sideways, never wrapping, at most 16 lines tall. Tokens come from the catalog's ONE tokenizer (catalog/code.json) in the code palette. Size \`sm\` (12 px, 10 px padding) is a tool's Output inside a Trace line; \`destructive\` (rose hairline at 40%, rose text, no fill) is a failed call's; \`ansi\` colours command output by its SGR colours and line kinds, the escapes never reaching the text.`,
  status: {
    web: {
      state: `ok`,
      symbol: `CodeBlock`,
      file: `packages/exponential-ui-react/src/primitives/code-block.tsx`,
      note: `tokenizer: packages/exponential-ui/src/code.ts; Output: apps/web/src/components/agent-session.tsx`,
    },
    desktop: {
      state: `ok`,
      symbol: `TraceGeometry`,
      file: `apps/desktop/crates/steer/src/trace.rs`,
      note: `geometry.codeBlock; the gpui painter (EXP-1324 desktop lane)`,
    },
    ios: {
      state: `ok`,
      symbol: `TraceLineModel`,
      file: `apps/ios/ExpCore/Sources/Domain/Trace.swift`,
      note: `geometry.codeBlock + AnsiCode.swift; ExpUI paints it (shot sg_code-block)`,
    },
    android: {
      state: `ok`,
      symbol: `TraceSpec`,
      file: `apps/android/app/src/main/java/com/exponential/app/domain/Trace.kt`,
      note: `TraceSpec.CodeBlock; Compose paints it (shot sg_code-block)`,
    },
  },
  island: () => <CodeBlockSpecimen />,
}
