import { useState } from "react"
import { useNavigate } from "@tanstack/react-router"
import type { BoardIcon } from "@exp/db-schema/domain"
import { APPS_COPY, APP_DESCRIPTION_MAX } from "@exp/domain-contract/apps"
import {
  Button,
  Dialog,
  DialogBody,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  GlassGroup,
  Textarea,
} from "@exp/ui"
import { BoardIdentityRow } from "@/components/board-form-fields"
import type { AppListEntry } from "@/hooks/use-apps"
import { failedAction, SKIP_ERROR_TOAST } from "@/lib/action-error"
import { appCollection } from "@/lib/collections"
import { trpc } from "@/lib/trpc-client"

// VAPP-110 (fixtures/apps.json): New app = the board form's identity row
// (icon + colour + name) and an optional description; Create opens the
// editor. The same dialog renames an existing app (`app` set).

const DEFAULT_COLOR = `#6366f1`
const DEFAULT_ICON: BoardIcon = `puzzle`

export function AppFormDialog({
  open,
  onOpenChange,
  teamId,
  teamSlug,
  app,
}: {
  open: boolean
  onOpenChange: (open: boolean) => void
  teamId: string
  teamSlug: string
  /** Rename mode: the app to edit. */
  app?: AppListEntry | null
}) {
  const navigate = useNavigate()
  const [name, setName] = useState(app?.name ?? ``)
  const [icon, setIcon] = useState<BoardIcon>((app?.icon as BoardIcon | null) ?? DEFAULT_ICON)
  const [color, setColor] = useState(app?.color ?? DEFAULT_COLOR)
  const [description, setDescription] = useState(app?.description ?? ``)
  const [submitting, setSubmitting] = useState(false)
  const editing = Boolean(app)

  const reset = () => {
    setName(app?.name ?? ``)
    setIcon((app?.icon as BoardIcon | null) ?? DEFAULT_ICON)
    setColor(app?.color ?? DEFAULT_COLOR)
    setDescription(app?.description ?? ``)
  }

  const submit = async (event: React.FormEvent) => {
    event.preventDefault()
    if (submitting || !name.trim()) return
    setSubmitting(true)
    try {
      if (app) {
        const result = await trpc.apps.update.mutate(
          { id: app.id, name: name.trim(), icon, color, description },
          SKIP_ERROR_TOAST
        )
        await appCollection.utils.awaitTxId(result.txid, 3000).catch(() => undefined)
        onOpenChange(false)
      } else {
        const result = await trpc.apps.create.mutate(
          { teamId, name: name.trim(), icon, color, description: description.trim() || undefined },
          SKIP_ERROR_TOAST
        )
        await appCollection.utils.awaitTxId(result.txid, 3000).catch(() => undefined)
        reset()
        onOpenChange(false)
        void navigate({ to: `/t/$teamSlug/apps/$appSlug`, params: { teamSlug, appSlug: result.app.slug } })
      }
    } catch (error) {
      failedAction(editing ? `rename` : `create`, `the app`, error)
    } finally {
      setSubmitting(false)
    }
  }

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        if (!next) reset()
        onOpenChange(next)
      }}
    >
      <DialogContent className="sm:max-w-md" data-testid="app-form-dialog">
        <DialogHeader>
          <DialogTitle>{editing ? APPS_COPY.menu.rename : APPS_COPY.newApp}</DialogTitle>
        </DialogHeader>
        <form onSubmit={submit} className="flex min-h-0 flex-1 flex-col gap-4">
          <DialogBody className="space-y-4">
            <GlassGroup>
              <BoardIdentityRow
                name={name}
                onNameChange={setName}
                autoFocus
                icon={icon}
                onIconChange={setIcon}
                color={color}
                onColorChange={setColor}
                placeholder={APPS_COPY.namePlaceholder}
              />
            </GlassGroup>
            <Textarea
              value={description}
              onChange={(event) => setDescription(event.target.value.slice(0, APP_DESCRIPTION_MAX))}
              placeholder={APPS_COPY.descriptionPlaceholder}
              rows={3}
              data-testid="app-form-description"
            />
          </DialogBody>
          <DialogFooter>
            <Button type="submit" disabled={!name.trim()} pending={submitting}>
              {editing ? APPS_COPY.rename : APPS_COPY.create}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  )
}
