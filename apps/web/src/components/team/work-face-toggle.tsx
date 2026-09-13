import {
  SEGMENTED_TAB,
  Tabs,
  TabsList,
  TabsTrigger,
} from "@/components/ui/tabs"
import type { WorkTabFace } from "@/lib/work-tabs"

// EXP-870: an issue and its run are ONE work tab with two faces. This is the
// `Issue | Run` segmented control the md+ issue detail header and the md+
// session header carry (desktop `issue_header.rs` top row) — the same
// segmented pill as the list nav's Inbox / My Issues strip. A face with no
// handler is disabled: Run on an issue that never had a run of mine
// (`issueSessionTarget`), Issue on a run that links none never renders it.

export function WorkFaceToggle({
  face,
  onIssue,
  onRun,
}: {
  face: WorkTabFace
  onIssue?: () => void
  onRun?: () => void
}) {
  return (
    <Tabs
      value={face}
      onValueChange={(next) => {
        if (next === face) return
        if (next === `issue`) onIssue?.()
        else onRun?.()
      }}
      className="w-fit shrink-0"
      data-testid="work-face-toggle"
    >
      <TabsList>
        <TabsTrigger
          value="issue"
          className={SEGMENTED_TAB}
          disabled={face !== `issue` && !onIssue}
        >
          Issue
        </TabsTrigger>
        <TabsTrigger
          value="run"
          className={SEGMENTED_TAB}
          disabled={face !== `run` && !onRun}
        >
          Run
        </TabsTrigger>
      </TabsList>
    </Tabs>
  )
}
