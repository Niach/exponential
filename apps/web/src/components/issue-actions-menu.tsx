import { useState } from "react"
import { Link2 } from "lucide-react"
import { contract } from "@exp/domain-contract"
import { useNavigate } from "@tanstack/react-router"
import type { Board, Issue } from "@/db/schema"
import {
  conceptIcon,
  Button,
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSub,
  DropdownMenuSubContent,
  DropdownMenuSubTrigger,
  DropdownMenuTrigger,
  IconTooltip,
  Prompt,
  toast,
} from "@exp/ui"
import { trpc } from "@/lib/trpc-client"
import { deleteIssuePrompt, promptActions } from "@/lib/prompts"
import {
  RELATION_SIDES,
  pickLabel,
  useAddRelation,
} from "@/components/issue-relations-card"
import { useClosePr } from "@/components/close-pr-dialog"

const UiMoreIcon = conceptIcon(`ui-more`)
const RelationSectionIcon = conceptIcon(`relation-section`)
const UiDeleteIcon = conceptIcon(`ui-delete`)
const UiUndoIcon = conceptIcon(`ui-undo`)
const PrClosedIcon = conceptIcon(`pr-closed`)

/** The issue's canonical URL, for Copy link and the phone's share sheet. */
export function issueUrlFor(
  teamSlug: string,
  boardSlug: string,
  issueIdentifier: string
): string {
  const origin = typeof window === `undefined` ? `` : window.location.origin
  return `${origin}/t/${teamSlug}/boards/${boardSlug}/issues/${issueIdentifier}`
}

// EXP-760: ONE round `…` beside the title — Copy link · Add relation ▸ ·
// Unmark duplicate (conditional) · Close PR (EXP-1154, an open PR) · Delete
// issue (EXP-1215: confirmed in the shared `delete-issue` prompt). It replaces
// the copy-link / unmark / trash trio: three permanent circles for actions
// taken once a week, where the IDE (`work_header.rs`) and both natives already
// collapse everything but the switcher into one menu. EXP-877: lifted out of
// the detail view so the session route's issue face carries the same menu.
// The "Add relation" picker dialog renders alongside (it portals).
export function IssueActionsMenu({
  issue,
  board,
  teamSlug,
  readOnly = false,
}: {
  issue: Issue
  board: Pick<Board, `slug`>
  teamSlug: string
  readOnly?: boolean
}) {
  const navigate = useNavigate()
  // The "Add relation" flow behind the `…` menu and the list row's context
  // menu — one hook, one picker (issue-relations-card.tsx).
  const addRelation = useAddRelation(issue.id)
  const issueUrl = issueUrlFor(teamSlug, board.slug, issue.identifier)
  // EXP-1154: Close PR without merging (EXP-248), once the review page's.
  const closePr = useClosePr(issue, { readOnly })

  // EXP-1215: Delete confirms in the shared prompt (`delete-issue`), the same
  // card the phone menu and the list row's context menu raise.
  const [deleteOpen, setDeleteOpen] = useState(false)
  const deleteCopy = deleteIssuePrompt(issue.identifier)

  // Delete is a hard delete (issues.delete cleans up attachments server-side);
  // once it commits, land back on the board.
  const handleDeleteIssue = async () => {
    await trpc.issues.delete.mutate({ id: issue.id })
    void navigate({
      to: `/t/$teamSlug/boards/$boardSlug`,
      params: { teamSlug, boardSlug: board.slug },
      search: {},
    })
  }

  return (
    <>
      <DropdownMenu>
        <IconTooltip label="More actions">
          <DropdownMenuTrigger asChild>
            <Button variant="ghost" size="icon-sm" aria-label="Issue actions">
              <UiMoreIcon />
            </Button>
          </DropdownMenuTrigger>
        </IconTooltip>
        <DropdownMenuContent align="end" className="w-[14rem]">
          <DropdownMenuItem
            onSelect={() => {
              if (typeof navigator === `undefined` || !navigator.clipboard)
                return
              navigator.clipboard.writeText(issueUrl).then(
                () => toast.success(`Link copied`),
                () => {
                  // Clipboard denied (permissions/insecure context) — the
                  // toast would be a lie, so say nothing.
                }
              )
            }}
          >
            <Link2 className="size-4" />
            Copy link
          </DropdownMenuItem>

          {!readOnly && (
            <DropdownMenuSub>
              <DropdownMenuSubTrigger>
                <RelationSectionIcon className="size-4" />
                Add relation
              </DropdownMenuSubTrigger>
              <DropdownMenuSubContent className="w-[12rem]">
                {RELATION_SIDES.filter((entry) => entry.pickable).map(
                  (entry) => {
                    const Icon = entry.icon
                    return (
                      <DropdownMenuItem
                        key={entry.side}
                        onSelect={() => addRelation.pick(entry)}
                      >
                        <Icon className="size-4" />
                        {pickLabel(entry.type, entry.direction)}
                      </DropdownMenuItem>
                    )
                  }
                )}
              </DropdownMenuSubContent>
            </DropdownMenuSub>
          )}

          {!readOnly && issue.duplicateOfId && (
            <DropdownMenuItem
              onSelect={() => {
                void trpc.issues.update.mutate({
                  id: issue.id,
                  duplicateOfId: null,
                })
              }}
            >
              <UiUndoIcon className="size-4" />
              Unmark duplicate
            </DropdownMenuItem>
          )}

          {/* No separator above a destructive item (EXP-687): the red is the
              divider, on every client. EXP-1154: Close PR confirms in a
              dialog (`close-pr.json`). */}
          {closePr.canClose && (
            <DropdownMenuItem
              variant="destructive"
              disabled={closePr.closing}
              data-testid="issue-close-pr"
              onSelect={closePr.request}
            >
              <PrClosedIcon className="size-4" />
              {contract.diffUi.closePr}
            </DropdownMenuItem>
          )}

          {/* Delete confirms in the prompt, deferred past the menu's close +
              focus restore so the card's focus trap does not fight Radix. */}
          {!readOnly && (
            <DropdownMenuItem
              variant="destructive"
              data-testid="issue-delete"
              onSelect={() => setTimeout(() => setDeleteOpen(true), 0)}
            >
              <UiDeleteIcon className="size-4" />
              Delete issue
            </DropdownMenuItem>
          )}
        </DropdownMenuContent>
      </DropdownMenu>
      {addRelation.dialog}
      {closePr.dialog}
      <Prompt
        open={deleteOpen}
        onOpenChange={setDeleteOpen}
        data-testid="issue-delete-confirm"
        title={deleteCopy.title}
        body={deleteCopy.body}
        actions={promptActions(deleteCopy, {
          // Async: the pill spins until the delete lands and the page leaves.
          delete: { onSelect: handleDeleteIssue },
        })}
      />
    </>
  )
}
