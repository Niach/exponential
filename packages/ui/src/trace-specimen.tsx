import { useState, type ReactNode } from "react"
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
} from "./code-block"
import { conceptIcon } from "./icons.generated"
import {
  Trace,
  TraceCollapsible,
  TraceCollapsibleContent,
  TraceCollapsibleTrigger,
  TraceDescription,
  TraceIcon,
  TraceLine,
  TraceTitle,
} from "./trace"

// EXP-1324 (+ EXP-1321): THE Trace and CodeBlock specimens, identical ×4
// (iOS ThreadCardsScreen, Android TraceSpecimenScreen, the IDE styleguide
// entries trace.rs / code_block.rs draw the same lines and blocks). Drawn by
// the styleguide entries `trace` / `code-block` and, for their captures, by
// the web app's dev-only `/t/$teamSlug/agent?dev=trace|code-block`.
//
// Trace: the session transcript's tail at size `sm` (a running command (the
// shimmer), reasoning, a command opened onto its Output, a subagent with its
// duration badge, a failed test run, a pending line), then the Queue, then
// two lines at the default size. CodeBlock: the natives paint `plain` and
// `ansi` at sm only, so every block is sm: a titled plain block, a tool's
// Output (`ansi`, the Terminal pattern), a failed call's destructive one.

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

export function TraceSpecimen(): ReactNode {
  const [todoOpen, setTodoOpen] = useState(true)
  const [doneOpen, setDoneOpen] = useState(false)
  const failedTests = traceFailedTests(parseTestResults(BENCHMARK_OUTPUT))!
  return (
    <div className="flex max-w-2xl flex-col gap-6" data-testid="trace-specimen">
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

const TS = `export function coldStart(board: Board): Promise<void> {
  // Paint the cached snapshot first, subscribe after the first frame.
  const cached = snapshots.read(board.id)
  return render(cached ?? [], { deferSubscribe: true })
}`

const CODE_ANSI = [
  `$ ./gradlew :app:startupProfile`,
  `> Task :app:startupProfile`,
  `\u001b[33mwarning:\u001b[0m baseline profile is 3 days old`,
  `Pixel 6a     p50 740 ms   p90 812 ms`,
  `\u001b[32mBUILD SUCCESSFUL\u001b[0m in 41s`,
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

export function CodeBlockSpecimen(): ReactNode {
  return (
    <div className="flex max-w-2xl flex-col gap-4" data-testid="code-block-specimen">
      <Block title="startup.ts" code={TS} language="plain" size="sm" />
      <Block title="Output" code={CODE_ANSI} language="ansi" size="sm" />
      <Block title="Output" code={BENCHMARK_OUTPUT} language="ansi" size="sm" variant="destructive" />
    </div>
  )
}
