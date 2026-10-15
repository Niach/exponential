import {
  CodeBlock,
  CodeBlockActions,
  CodeBlockContent,
  CodeBlockCopyButton,
  CodeBlockHeader,
  CodeBlockTitle,
} from "@exp/ui"

import type { StyleguideEntry } from "./types.ts"

// EXP-1324: THE CodeBlock, shadcn's code-block (radix-nova) on our Button,
// coloured by the catalog's ONE tokenizer (`tokenizeCode`): the default
// block, the sm block a tool's Output is, the destructive one a failed call
// gets, and `ansi` command output (the Terminal pattern).

const TS = `export function coldStart(board: Board): Promise<void> {
  // Paint the cached snapshot first, subscribe after the first frame.
  const cached = snapshots.read(board.id)
  return render(cached ?? [], { deferSubscribe: true })
}`

const ANSI = [
  `$ ./gradlew :app:startupProfile`,
  `> Task :app:startupProfile`,
  `\u001b[33mwarning:\u001b[0m baseline profile is 3 days old`,
  `Pixel 6a     p50 740 ms   p90 812 ms`,
  `\u001b[32mBUILD SUCCESSFUL\u001b[0m in 41s`,
].join(`\n`)

const FAILED = [
  `StartupBenchmark > coldStart FAILED`,
  `    java.lang.AssertionError: p90 812 ms > 800 ms`,
  ``,
  `12 tests completed, 1 failed`,
].join(`\n`)

function Block({
  title,
  code,
  language,
  size = `default`,
  variant = `default`,
}: {
  title: string
  code: string
  language: string
  size?: `default` | `sm`
  variant?: `default` | `destructive`
}) {
  return (
    <CodeBlock size={size} variant={variant}>
      <CodeBlockHeader>
        <CodeBlockTitle>{title}</CodeBlockTitle>
        <CodeBlockActions>
          <CodeBlockCopyButton value={code} />
        </CodeBlockActions>
      </CodeBlockHeader>
      <CodeBlockContent language={language}>{code}</CodeBlockContent>
    </CodeBlock>
  )
}

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
  island: () => (
    <div className="flex max-w-2xl flex-col gap-4">
      <Block title="startup.ts" code={TS} language="ts" />
      <Block title="Output" code={ANSI} language="ansi" size="sm" />
      <Block title="Output" code={FAILED} language="ansi" size="sm" variant="destructive" />
    </div>
  ),
}
