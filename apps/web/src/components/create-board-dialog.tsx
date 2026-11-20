import { useEffect, useState } from "react"
import type { BoardIcon } from "@exp/db-schema/domain"
import type { Team } from "@/db/schema"
import {
  Dialog,
  DialogBody,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
  Button,
  FieldError,
  GlassGroup,
} from "@exp/ui"
import {
  BoardIdentityRow,
  BoardPrefixField,
} from "@/components/board-form-fields"
import { type PickerRepo } from "@/components/github-repo-picker"
import { BoardRepoField } from "@/components/board-repo-field"
import { UpgradeDialog } from "@/components/upgrade-dialog"
import { getRuntimeConfig } from "@/lib/runtime-config"
import { derivePrefix } from "@/lib/board"
import { useCreateBoard } from "@/hooks/use-create-board"
import { failedAction } from "@/lib/action-error"

/** The prefix rule the server enforces (boards.create's zod regex); its
 *  validation error reaches the client as a JSON issue list, so the field
 *  says it in these words. */
const PREFIX_RULE = `Prefix must be 1-4 letters or digits, starting with a letter`

/** FEED-79: which field a refused create is about (a taken name or prefix,
 *  an invalid prefix = invalid INPUT, the field's own line), else null (a
 *  failed action, a toast). */
export function boardCreateFieldError(
  message: string
): { field: `name` | `prefix`; message: string } | null {
  if (message.includes(`this name`)) return { field: `name`, message }
  if (message.includes(`this prefix`)) return { field: `prefix`, message }
  if (message.includes(PREFIX_RULE)) return { field: `prefix`, message: PREFIX_RULE }
  return null
}

// The chosen backing repo: either an existing registry repo (by id) or a
// brand-new one picked through the GithubRepoPicker (connected inline by
// boards.create in the same transaction).
type RepoSelection =
  | { kind: `registry`; repositoryId: string; fullName: string }
  | { kind: `inline`; repo: PickerRepo }

/** #EXP-2027: the form both hosts render: the dialog (phones, settings,
 *  getting started) and the New board PAGE (`/t/$teamSlug/new/board`, the
 *  team menu's New › Board on md+). `layout` only picks the frame: the
 *  dialog's scrolling body over a pinned footer, or the page's plain stack
 *  ending on the Create button. */
export function CreateBoardForm({
  team,
  layout = `dialog`,
  onCreated,
  onPlanLimit,
}: {
  team: Team
  layout?: `dialog` | `page`
  /** The board exists (its slug + the write's txid). */
  onCreated: (board: { slug: string; txId: number }) => void
  /** The plan's board cap refused it: the host opens the upgrade dialog. */
  onPlanLimit: () => void
}) {
  const teamId = team.id
  const { createBoard } = useCreateBoard()
  const [name, setName] = useState(``)
  const [prefix, setPrefix] = useState(``)
  const [color, setColor] = useState(`#6366f1`)
  const [icon, setIcon] = useState<BoardIcon>(`code`)
  const [submitting, setSubmitting] = useState(false)

  const [selection, setSelection] = useState<RepoSelection | null>(null)
  // EXP-712: the board's own branch; null = the repo's default. Reset with
  // the repo — a branch belongs to the repo it was picked in.
  const [branch, setBranch] = useState<string | null>(null)
  const [fieldError, setFieldError] = useState<{
    field: `name` | `prefix`
    message: string
  } | null>(null)

  const handleNameChange = (value: string) => {
    setName(value)
    setPrefix(derivePrefix(value))
    setFieldError(null)
  }

  const handlePrefixChange = (value: string) => {
    setPrefix(value)
    if (fieldError?.field === `prefix`) setFieldError(null)
  }

  const changeSelection = (next: RepoSelection | null) => {
    setSelection(next)
    setBranch(null)
  }

  const canSubmit = Boolean(name.trim()) && Boolean(prefix.trim())

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    if (submitting || !name.trim() || !prefix.trim()) return

    setSubmitting(true)
    setFieldError(null)
    const repository = !selection
      ? undefined
      : selection.kind === `registry`
        ? { repositoryId: selection.repositoryId }
        : {
            fullName: selection.repo.fullName,
            defaultBranch: selection.repo.defaultBranch,
            private: selection.repo.private,
          }
    const result = await createBoard({
      teamId,
      name,
      prefix,
      color,
      icon,
      repository,
      defaultBranch: repository ? (branch ?? undefined) : undefined,
    })
    setSubmitting(false)
    if (result.ok) {
      onCreated({ slug: result.slug, txId: result.txId })
      return
    }
    if (result.error.kind === `planLimit`) {
      onPlanLimit()
    } else {
      const invalid = boardCreateFieldError(result.error.message)
      if (invalid) setFieldError(invalid)
      // A create can duplicate, so no Retry; the form stays.
      else failedAction(`create`, `the board`, result.error.message)
    }
  }

  // EXP-862: one grouped form — identity (icon, colour, name) in ONE row,
  // then the prefix; the repository block brings its own group. No captions
  // above any field.
  const fields = (
    <>
      <GlassGroup>
        <BoardIdentityRow
          name={name}
          onNameChange={handleNameChange}
          autoFocus
          icon={icon}
          onIconChange={setIcon}
          color={color}
          onColorChange={setColor}
          invalid={fieldError?.field === `name`}
        />
        <BoardPrefixField
          value={prefix}
          onChange={handlePrefixChange}
          invalid={fieldError?.field === `prefix`}
        />
      </GlassGroup>
      {fieldError && (
        <FieldError className="-mt-2 px-1">{fieldError.message}</FieldError>
      )}

      <BoardRepoField
        teamId={teamId}
        repositoryId={
          selection?.kind === `registry` ? selection.repositoryId : null
        }
        inlineRepo={selection?.kind === `inline` ? selection.repo : null}
        onSelectRegistry={(repo) =>
          changeSelection(
            repo
              ? {
                  kind: `registry`,
                  repositoryId: repo.id,
                  fullName: repo.fullName,
                }
              : null
          )
        }
        onConnectNew={(repo: PickerRepo) =>
          changeSelection({ kind: `inline`, repo })
        }
        branch={branch}
        onBranchChange={setBranch}
      />
    </>
  )
  const submit = (
    <Button type="submit" disabled={!canSubmit} pending={submitting}>
      Create board
    </Button>
  )

  if (layout === `page`) {
    return (
      <form onSubmit={handleSubmit} className="flex flex-col gap-4">
        {fields}
        <div className="flex justify-end">{submit}</div>
      </form>
    )
  }
  // The form is the flex middle of the panel: its fields scroll in the
  // DialogBody while the submit button rides the pinned footer — still
  // inside the form, so Enter and the click both submit.
  return (
    <form onSubmit={handleSubmit} className="flex min-h-0 flex-1 flex-col gap-4">
      <DialogBody className="space-y-4">{fields}</DialogBody>
      <DialogFooter>{submit}</DialogFooter>
    </form>
  )
}

/** The plan-cap nudge a refused create opens (both hosts). */
export function BoardLimitUpgradeDialog({
  open,
  onOpenChange,
  teamId,
}: {
  open: boolean
  onOpenChange: (open: boolean) => void
  teamId: string
}) {
  const [productIds, setProductIds] = useState<{
    team: string | null
    teamYearly: string | null
  }>({ team: null, teamYearly: null })

  useEffect(() => {
    void getRuntimeConfig().then((config) => {
      setProductIds({
        team: config.creemTeamProductId,
        teamYearly: config.creemTeamYearlyProductId,
      })
    })
  }, [])

  return (
    <UpgradeDialog
      open={open}
      onOpenChange={onOpenChange}
      title="Board limit reached"
      description="You've reached the maximum number of boards for your plan. Upgrade to create more."
      teamProductId={productIds.team}
      teamYearlyProductId={productIds.teamYearly}
      teamId={teamId}
    />
  )
}

export function CreateBoardDialog({
  open,
  onOpenChange,
  team,
}: {
  open: boolean
  onOpenChange: (open: boolean) => void
  team: Team
}) {
  const [upgradeOpen, setUpgradeOpen] = useState(false)
  // The form lives inside DialogContent, which unmounts on close: a reopened
  // dialog starts blank.
  return (
    <>
      <Dialog open={open} onOpenChange={onOpenChange}>
        <DialogContent className="sm:max-w-md">
          <DialogHeader>
            <DialogTitle>Create board</DialogTitle>
          </DialogHeader>
          <CreateBoardForm
            team={team}
            onCreated={() => onOpenChange(false)}
            onPlanLimit={() => {
              onOpenChange(false)
              setUpgradeOpen(true)
            }}
          />
        </DialogContent>
      </Dialog>

      <BoardLimitUpgradeDialog
        open={upgradeOpen}
        onOpenChange={setUpgradeOpen}
        teamId={team.id}
      />
    </>
  )
}
