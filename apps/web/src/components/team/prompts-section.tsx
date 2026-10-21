import { useState } from "react"
import { useNavigate } from "@tanstack/react-router"
import {
  Button,
  EmptyState,
  GlassSectionHeader,
  ListRow,
  Pill,
  Prompt,
  SETTINGS_LIST_CLASS,
  conceptIcon,
  getActionIcon,
} from "@exp/ui"
import type { SyncedPrompt, Team } from "@/db/schema"
import { PromptForm } from "@/components/prompt-form"
import { failedAction, SKIP_ERROR_TOAST } from "@/lib/action-error"
import { promptCollection } from "@/lib/collections"
import { trpc } from "@/lib/trpc-client"

// NEW-2: Settings → Prompts, built like Settings → Agents: the index is the
// band ("New prompt" as its owner-only trailing pill over one row per
// prompt, icon + name), a row opens the prompt's page (THE form, plus Delete
// for owners), "New prompt" opens the same page in create mode. Members read
// every page; the owner-only controls are HIDDEN from them.

export const PROMPTS_TITLE = `Prompts`
export const NEW_PROMPT_LABEL = `New prompt`

const UiAddIcon = conceptIcon(`ui-add`)
const UiDeleteIcon = conceptIcon(`ui-delete`)
const PromptsIcon = conceptIcon(`nav-actions`)

/** The Prompts band: the team's prompts as Settings rows, or the list-size
 *  empty state while there are none. */
export function PromptsListBand({
  team,
  isOwner,
  prompts,
}: {
  team: Team
  isOwner: boolean
  prompts: readonly SyncedPrompt[]
}) {
  const navigate = useNavigate()
  return (
    <div>
      <GlassSectionHeader
        label={PROMPTS_TITLE}
        trailing={
          isOwner ? (
            <Pill
              mode="action"
              data-testid="prompts-new"
              onClick={() =>
                void navigate({
                  to: `/t/$teamSlug/settings/prompts/new`,
                  params: { teamSlug: team.slug },
                })
              }
            >
              <UiAddIcon className="size-3" />
              {NEW_PROMPT_LABEL}
            </Pill>
          ) : undefined
        }
      />
      {prompts.length === 0 ? (
        <EmptyState icon={PromptsIcon} title="No prompts yet" size="list" />
      ) : (
        <div className={SETTINGS_LIST_CLASS}>
          {prompts.map((prompt) => {
            const Icon = getActionIcon(prompt)
            return (
              <ListRow
                key={prompt.id}
                interactive
                onClick={() =>
                  void navigate({
                    to: `/t/$teamSlug/settings/prompts/$promptId`,
                    params: { teamSlug: team.slug, promptId: prompt.id },
                  })
                }
                className="gap-3 px-3 py-2 text-sm"
                data-testid={`prompts-list-row-${prompt.id}`}
              >
                <Icon className="size-4 shrink-0 text-muted-foreground" />
                <span className="min-w-0 flex-1 truncate font-medium">{prompt.name}</span>
              </ListRow>
            )
          })}
        </div>
      )}
    </div>
  )
}

/** One prompt's page (`prompt` = the live synced row), or "New prompt"
 *  (`prompt` null, owners only). */
export function PromptSettingsPage({
  team,
  prompt,
  isOwner,
}: {
  team: Team
  prompt: SyncedPrompt | null
  isOwner: boolean
}) {
  const navigate = useNavigate()
  const [deleteOpen, setDeleteOpen] = useState(false)
  const [deleting, setDeleting] = useState(false)

  const confirmDelete = async () => {
    if (!prompt) return
    setDeleting(true)
    try {
      const { txId } = await trpc.prompts.delete.mutate({ id: prompt.id }, SKIP_ERROR_TOAST)
      await promptCollection.utils.awaitTxId(txId).catch(() => {})
      setDeleteOpen(false)
      void navigate({
        to: `/t/$teamSlug/settings/prompts`,
        params: { teamSlug: team.slug },
        replace: true,
      })
    } catch (err) {
      failedAction(`delete`, `the prompt`, err)
    } finally {
      setDeleting(false)
    }
  }

  return (
    <div className="flex flex-col gap-4" data-testid="prompt-page">
      <GlassSectionHeader label={prompt ? prompt.name : NEW_PROMPT_LABEL} />
      <PromptForm
        key={prompt?.id ?? `new`}
        prompt={prompt}
        teamId={team.id}
        readOnly={!isOwner}
        onCreated={(promptId) =>
          void navigate({
            to: `/t/$teamSlug/settings/prompts/$promptId`,
            params: { teamSlug: team.slug, promptId },
            replace: true,
          })
        }
      />
      {prompt && isOwner && (
        <div className="flex flex-wrap gap-2">
          <Button
            variant="destructive"
            size="sm"
            onClick={() => setDeleteOpen(true)}
            data-testid="prompt-delete"
          >
            <UiDeleteIcon />
            Delete prompt
          </Button>
        </div>
      )}
      <Prompt
        open={deleteOpen}
        onOpenChange={setDeleteOpen}
        busy={deleting}
        title={`Delete "${prompt?.name ?? ``}"?`}
        actions={[
          { label: `Cancel`, role: `cancel` },
          {
            label: `Delete`,
            role: `destructive`,
            busy: deleting,
            onSelect: confirmDelete,
            testId: `prompt-delete-confirm`,
          },
        ]}
      />
    </div>
  )
}
