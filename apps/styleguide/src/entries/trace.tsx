import { useState } from "react"
import {
  parseTestResults,
  traceFailedTests,
  traceReasoning,
  traceSubagent,
  traceTool,
  TRACE_COPY,
  type TraceLineModel,
} from "@exp/domain-contract/trace"
import { taskQueue } from "@exp/domain-contract/task-queue"
import {
  CodeBlock,
  CodeBlockActions,
  CodeBlockContent,
  CodeBlockCopyButton,
  CodeBlockHeader,
  CodeBlockTitle,
  conceptIcon,
  Trace,
  TraceCollapsible,
  TraceCollapsibleContent,
  TraceCollapsibleTrigger,
  TraceDescription,
  TraceIcon,
  TraceLine,
  TraceTitle,
} from "@exp/ui"

import type { StyleguideEntry } from "./types.ts"

// EXP-1324 (+ EXP-1321): THE Trace, shadcn's Trace on our Collapsible, over
// the contract's pure models (`@exp/domain-contract/trace`, `/task-queue`).
// The specimen is the session transcript's tail at size `sm`: a running
// command (the shimmer), reasoning, a command opened onto its Output, a
// subagent with its duration badge, a failed test run, a pending line, then
// the Queue.

type Concept = Parameters<typeof conceptIcon>[0]

const PROFILE_OUTPUT = [
  `> Task :app:startupProfile`,
  `Pixel 6a     p50 740 ms   p90 812 ms`,
  `Galaxy A54   p50 790 ms   p90 840 ms`,
  `BUILD SUCCESSFUL in 41s`,
].join(`\n`)

const BENCHMARK_OUTPUT = [
  `StartupBenchmark > coldStart FAILED`,
  `    java.lang.AssertionError: p90 812 ms > 800 ms`,
  ``,
  `12 tests completed, 1 failed`,
].join(`\n`)

function ModelLine({
  model,
  badge = false,
  chevron = false,
}: {
  model: TraceLineModel
  badge?: boolean
  chevron?: boolean
}) {
  const Icon = conceptIcon(model.icon as Concept)
  const Chevron = conceptIcon(`trace-chevron`)
  return (
    <TraceLine status={model.status}>
      <TraceIcon>
        <Icon />
      </TraceIcon>
      <TraceTitle>{model.title}</TraceTitle>
      {model.description ? (
        <TraceDescription
          className={
            badge
              ? `rounded-full border border-border px-1.5 text-[0.625rem] leading-4`
              : undefined
          }
        >
          {model.description}
        </TraceDescription>
      ) : null}
      {chevron ? (
        <TraceIcon>
          <Chevron />
        </TraceIcon>
      ) : null}
    </TraceLine>
  )
}

function Output({ text, failed = false }: { text: string; failed?: boolean }) {
  return (
    <CodeBlock size="sm" variant={failed ? `destructive` : `default`}>
      <CodeBlockHeader>
        <CodeBlockTitle>{TRACE_COPY.output}</CodeBlockTitle>
        <CodeBlockActions>
          <CodeBlockCopyButton value={text} />
        </CodeBlockActions>
      </CodeBlockHeader>
      <CodeBlockContent language="ansi">{text}</CodeBlockContent>
    </CodeBlock>
  )
}

const QUEUE = taskQueue({
  tasks: [
    { content: `Profile the cold start`, status: `completed` },
    { content: `Defer the snapshot cache warm-up`, status: `completed` },
    { content: `Measuring on the Galaxy A54`, status: `in_progress` },
    { content: `Open the PR with the numbers`, status: `pending` },
  ],
  backgroundTasks: [
    { id: `b1`, kind: `shell`, description: `Rerun the web and web-phone captures`, startedAt: 0 },
  ],
  waits: [],
  now: 130_000,
})!

function Specimen() {
  const [todoOpen, setTodoOpen] = useState(true)
  const [doneOpen, setDoneOpen] = useState(false)
  const failedTests = traceFailedTests(parseTestResults(BENCHMARK_OUTPUT))!
  return (
    <div className="flex max-w-2xl flex-col gap-6">
      <Trace size="sm">
        <ModelLine
          model={traceTool({
            toolKind: `execute`,
            name: `Bash`,
            detail: `./gradlew :app:startupProfile`,
            settled: false,
            failed: false,
          })}
        />
        <ModelLine model={traceReasoning({ running: false, durationMs: 5_200 })} />
        <TraceCollapsible defaultOpen>
          <TraceCollapsibleTrigger>
            <ModelLine
              model={traceTool({
                toolKind: `execute`,
                name: `Bash`,
                detail: `./gradlew :app:startupProfile`,
                settled: true,
                failed: false,
              })}
            />
          </TraceCollapsibleTrigger>
          <TraceCollapsibleContent>
            <Output text={PROFILE_OUTPUT} />
          </TraceCollapsibleContent>
        </TraceCollapsible>
        <button type="button">
          <ModelLine
            model={traceSubagent({
              label: `Explore`,
              running: false,
              failed: false,
              durationMs: 72_000,
            })}
            badge
            chevron
          />
        </button>
        <TraceCollapsible>
          <TraceCollapsibleTrigger>
            <ModelLine model={failedTests} />
          </TraceCollapsibleTrigger>
          <TraceCollapsibleContent>
            <Output text={BENCHMARK_OUTPUT} failed />
          </TraceCollapsibleContent>
        </TraceCollapsible>
        <ModelLine
          model={{ ...traceTool({ toolKind: `read`, name: `Read`, detail: `README.md`, settled: true, failed: false }), status: `pending` }}
        />
      </Trace>
      <Trace size="sm">
        {[
          { section: QUEUE.todo, open: todoOpen, setOpen: setTodoOpen },
          { section: QUEUE.completed!, open: doneOpen, setOpen: setDoneOpen },
        ].map(({ section, open, setOpen }) => {
          const Icon = conceptIcon(section.icon as Concept)
          return (
            <TraceCollapsible key={section.title} open={open} onOpenChange={setOpen}>
              <TraceCollapsibleTrigger>
                <TraceLine>
                  <TraceIcon>
                    <Icon />
                  </TraceIcon>
                  <TraceTitle>{section.title}</TraceTitle>
                </TraceLine>
              </TraceCollapsibleTrigger>
              <TraceCollapsibleContent>
                <div className="flex flex-col gap-1.5">
                  {section.items.map((item) => {
                    const ItemIcon = conceptIcon(item.icon as Concept)
                    return (
                      <TraceLine
                        key={item.key}
                        status={item.state === `running` ? `running` : `pending`}
                      >
                        <TraceIcon>
                          <ItemIcon />
                        </TraceIcon>
                        <TraceTitle>{item.label}</TraceTitle>
                        {item.description ? (
                          <TraceDescription>{item.description}</TraceDescription>
                        ) : null}
                      </TraceLine>
                    )
                  })}
                </div>
              </TraceCollapsibleContent>
            </TraceCollapsible>
          )
        })}
      </Trace>
      <Trace>
        <ModelLine
          model={traceTool({ toolKind: `search`, name: `Grep`, detail: `persistOnStop`, settled: true, failed: false })}
        />
        <ModelLine
          model={traceTool({ toolKind: `execute`, name: `Bash`, detail: `bun test`, settled: true, failed: true })}
        />
      </Trace>
    </div>
  )
}

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
      file: `packages/exponential-ui-react/src/primitives/trace.tsx`,
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
  island: () => <Specimen />,
}
