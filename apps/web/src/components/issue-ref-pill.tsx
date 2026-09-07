import {
  useIssueRefs,
  type ResolvedIssueRef,
} from "@/components/issue-ref-provider"
import { IssuePreviewHoverCard } from "@/components/issue-preview-card"
import { IssueStatusIcon } from "@/components/issue-properties/status-dropdown"
import { Pill } from "@/components/ui/pill"

// EXP-760 — the inline issue chip for surfaces that are NOT a markdown editor:
// the steering feed's plain-prose path (`IssueRefText`) and the timeline's
// relation events. Inside an editor the same chip is a ProseMirror decoration
// instead (lib/issue-ref-extension.ts), because the document text has to stay
// the plain token.
//
// A chip renders ONLY for an already-resolved, same-team issue — the caller
// resolves; an unresolved token stays prose. Clicking navigates; hovering
// previews (desktop only, the hover host self-disables on phones).

export function IssueRefPill({ issue }: { issue: ResolvedIssueRef }) {
  const issueRefs = useIssueRefs()

  return (
    <IssuePreviewHoverCard issueId={issue.id}>
      <Pill
        size="sm"
        mode="action"
        // `align-middle`: the chip flows inline in prose, and an
        // inline-flex box would otherwise sit on the text baseline.
        className="max-w-[18rem] align-middle"
        title={`${issue.identifier} · ${issue.title}`}
        onClick={() => issueRefs?.open(issue.identifier)}
        leading={<IssueStatusIcon issue={issue} className="!h-3 !w-3" />}
      >
        <span className="font-mono">{issue.identifier}</span>
        <span className="min-w-0 truncate text-muted-foreground">
          {issue.title}
        </span>
      </Pill>
    </IssuePreviewHoverCard>
  )
}
