import { AgentRunMark, ThreadGlyph, ThreadListRow, type RunMarkState } from "@exp/ui"
import type { ThreadBundle } from "@/hooks/use-threads"

// EXP-1284: a THREAD in a list (the sidebar's Running, Agent home's Recent,
// the Agent › Recent panel, the phone's Agent screen): its title over the
// model's `listCaption` (`2 issues · 1 run · macbook`), led by the live
// run's mark while one runs (the spark while it works, amber while it waits
// on a person — the run rows' rule), else the thread glyph.

export function threadMarkState(bundle: ThreadBundle): RunMarkState | undefined {
  if (!bundle.view.live) return undefined
  if (bundle.view.needsInput) return `needs_input`
  if (bundle.view.busy) return `working`
  return undefined
}

export function ThreadLead({
  bundle,
  ringClassName = `ring-sidebar`,
}: {
  bundle: ThreadBundle
  ringClassName?: string
}) {
  if (!bundle.view.live || !bundle.liveRun) return <ThreadGlyph />
  return (
    <AgentRunMark
      agent={bundle.liveRun.agent}
      state={threadMarkState(bundle)}
      ringClassName={ringClassName}
    />
  )
}

export function ThreadRow({
  bundle,
  active = false,
  onOpen,
  ringClassName,
  testId,
}: {
  bundle: ThreadBundle
  active?: boolean
  onOpen: () => void
  ringClassName?: string
  testId?: string
}) {
  return (
    <ThreadListRow
      lead={<ThreadLead bundle={bundle} ringClassName={ringClassName} />}
      title={bundle.view.title}
      caption={bundle.view.listCaption}
      captionTone={bundle.view.needsInput ? `amber` : `muted`}
      active={active}
      onClick={onOpen}
      data-testid={testId ?? `thread-row-${bundle.thread.id}`}
    />
  )
}
