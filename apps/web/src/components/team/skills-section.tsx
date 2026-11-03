import { useCallback, useEffect, useRef, useState } from "react"
import { useNavigate } from "@tanstack/react-router"
import { TRPCClientError } from "@trpc/client"
import {
  SKILLS,
  canEditSkill,
  skillPill,
  validateSkillForm,
  type SkillField,
} from "@exp/domain-contract/skills"
import {
  BARE_FIELD_CLASS,
  Button,
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
  FEEDBACK_ERRORS,
  FEEDBACK_WORDS,
  FieldError,
  GlassGroup,
  GlassSectionHeader,
  GlassToggleRow,
  Input,
  ListRow,
  Pill,
  Prompt,
  SETTINGS_LIST_CLASS,
  SkeletonRows,
  Textarea,
  conceptIcon,
} from "@exp/ui"
import type { SyncedSkill, Team } from "@/db/schema"
import { Empty } from "@/components/empty"
import { GROUPED_FIELD_ROW } from "@/components/prompt-form"
import { failedAction, SKIP_ERROR_TOAST } from "@/lib/action-error"
import { skillCollection } from "@/lib/collections"
import { trpc } from "@/lib/trpc-client"

// #EXP-1659: Settings › Skills (fixtures/skills.json is the spec). The index
// is the band ("New skill" = the owners' trailing pill) over one row per
// skill in the MCP server row's layout (glyph, name + Default pill, the
// description as its caption, the owners' "…" menu); a row opens the skill's
// page, built like the prompt page (name, description, the mono body, the
// "Pre-select in new sessions" switch, Delete behind THE Prompt). Members read every page.

const COPY = SKILLS.list
const PAGE = SKILLS.page
export const SKILLS_TITLE = SKILLS.title
export const NEW_SKILL_LABEL = COPY.newButton

export const SkillIcon = conceptIcon(`ui-docs`)
const AddIcon = conceptIcon(`ui-add`)
const MoreIcon = conceptIcon(`ui-more`)
const EditIcon = conceptIcon(`ui-edit`)
const DeleteIcon = conceptIcon(`ui-delete`)

function pageField(id: string) {
  const field = PAGE.fields.find((entry) => entry.id === id)
  if (!field) throw new Error(`skills.json: no page field ${id}`)
  return field as { id: string; label: string; placeholder?: string }
}

/** The delete confirm + its mutation, shared by the row menu and the page. */
function useDeleteSkill(onDeleted: () => void) {
  const [target, setTarget] = useState<SyncedSkill | null>(null)
  const [busy, setBusy] = useState(false)
  const confirm = async () => {
    if (!target) return
    setBusy(true)
    try {
      const { txId } = await trpc.skills.delete.mutate({ id: target.id }, SKIP_ERROR_TOAST)
      await skillCollection.utils.awaitTxId(txId).catch(() => {})
      setTarget(null)
      onDeleted()
    } catch (err) {
      failedAction(`delete`, `the skill`, err)
    } finally {
      setBusy(false)
    }
  }
  const dialog = (
    <Prompt
      open={target !== null}
      onOpenChange={(open) => !open && setTarget(null)}
      busy={busy}
      title={PAGE.deleteConfirm.title}
      body={PAGE.deleteConfirm.body}
      actions={[
        { label: PAGE.deleteConfirm.cancel, role: `cancel` },
        {
          label: PAGE.deleteConfirm.confirm,
          role: `destructive`,
          busy,
          onSelect: confirm,
          testId: `skill-delete-confirm`,
        },
      ]}
    />
  )
  return { ask: setTarget, dialog }
}

/** The Skills band: the team's skills as Settings rows, or the list-size
 *  empty state while there are none. */
export function SkillsListBand({
  team,
  isOwner,
  skills,
  isReady,
}: {
  team: Team
  isOwner: boolean
  skills: readonly SyncedSkill[]
  isReady: boolean
}) {
  const navigate = useNavigate()
  const open = (skillId: string) =>
    void navigate({
      to: `/t/$teamSlug/settings/skills/$skillId`,
      params: { teamSlug: team.slug, skillId },
    })
  const remove = useDeleteSkill(() => {})
  return (
    <div data-testid={COPY.testIds.page}>
      <GlassSectionHeader
        label={SKILLS_TITLE}
        trailing={
          isOwner ? (
            <Pill
              mode="action"
              data-testid="skills-new"
              onClick={() =>
                void navigate({
                  to: `/t/$teamSlug/settings/skills/new`,
                  params: { teamSlug: team.slug },
                })
              }
            >
              <AddIcon className="size-3" />
              {NEW_SKILL_LABEL}
            </Pill>
          ) : undefined
        }
      />
      {!isReady ? (
        <SkeletonRows rowHeight={52} />
      ) : skills.length === 0 ? (
        <Empty id="skills" />
      ) : (
        <div className={SETTINGS_LIST_CLASS} data-testid={COPY.testIds.list}>
          {skills.map((skill) => (
            <ListRow
              key={skill.id}
              interactive
              onClick={() => open(skill.id)}
              className="gap-3 px-3 py-2"
              data-testid={COPY.testIds.row}
            >
              <SkillIcon className="size-4 shrink-0 text-foreground/70" />
              <div className="min-w-0 flex-1">
                <div className="flex min-w-0 items-baseline gap-1.5">
                  <span className="min-w-0 truncate text-sm font-medium">{skill.name}</span>
                  {skillPill(skill) && <Pill size="sm">{skillPill(skill)}</Pill>}
                </div>
                <div className="flex min-w-0 text-xs text-muted-foreground">
                  <span className="min-w-0 truncate" title={skill.description}>
                    {skill.description}
                  </span>
                </div>
              </div>
              {canEditSkill(skill, isOwner) && (
                <DropdownMenu>
                  <DropdownMenuTrigger asChild>
                    <Button
                      variant="ghost"
                      size="icon-sm"
                      aria-label={`Skill menu for ${skill.name}`}
                      onClick={(event) => event.stopPropagation()}
                    >
                      <MoreIcon />
                    </Button>
                  </DropdownMenuTrigger>
                  <DropdownMenuContent align="end" onClick={(event) => event.stopPropagation()}>
                    <DropdownMenuItem onSelect={() => open(skill.id)}>
                      <EditIcon />
                      Edit
                    </DropdownMenuItem>
                    <DropdownMenuSeparator />
                    <DropdownMenuItem variant="destructive" onSelect={() => remove.ask(skill)}>
                      <DeleteIcon />
                      Delete
                    </DropdownMenuItem>
                  </DropdownMenuContent>
                </DropdownMenu>
              )}
            </ListRow>
          ))}
        </div>
      )}
      {remove.dialog}
    </div>
  )
}

/** One skill's page (`skill` = the live synced row), or "New skill"
 *  (`skill` null, owners only). */
export function SkillSettingsPage({
  team,
  skill,
  isOwner,
}: {
  team: Team
  skill: SyncedSkill | null
  isOwner: boolean
}) {
  const navigate = useNavigate()
  const remove = useDeleteSkill(
    () =>
      void navigate({
        to: `/t/$teamSlug/settings/skills`,
        params: { teamSlug: team.slug },
        replace: true,
      })
  )
  return (
    <div className="flex flex-col gap-4" data-testid="skill-page">
      <GlassSectionHeader label={skill ? skill.name : NEW_SKILL_LABEL} />
      <SkillForm
        key={skill?.id ?? `new`}
        skill={skill}
        teamId={team.id}
        readOnly={skill ? !canEditSkill(skill, isOwner) : !isOwner}
        onCreated={(skillId) =>
          void navigate({
            to: `/t/$teamSlug/settings/skills/$skillId`,
            params: { teamSlug: team.slug, skillId },
            replace: true,
          })
        }
      />
      {skill && canEditSkill(skill, isOwner) && (
        <div className="flex flex-wrap gap-2">
          <Button
            variant="destructive"
            size="sm"
            onClick={() => remove.ask(skill)}
            data-testid="skill-delete"
          >
            <DeleteIcon />
            {pageField(`delete`).label}
          </Button>
        </div>
      )}
      {remove.dialog}
    </div>
  )
}

/** The skill's ONE form: `skill` = the synced row to edit; null = create. */
function SkillForm({
  skill,
  teamId,
  readOnly,
  onCreated,
}: {
  skill: SyncedSkill | null
  teamId: string
  readOnly: boolean
  onCreated: (skillId: string) => void
}) {
  const creating = skill === null
  const [name, setName] = useState(``)
  const [description, setDescription] = useState(``)
  const [enabledByDefault, setEnabledByDefault] = useState(false)
  const [body, setBody] = useState(``)
  const [savedBody, setSavedBody] = useState(``)
  const [bodyLoading, setBodyLoading] = useState(false)
  const [bodyFailed, setBodyFailed] = useState(false)
  const [submitting, setSubmitting] = useState(false)
  const [touched, setTouched] = useState(false)
  // A server CONFLICT (the name is taken) reads inline under the name.
  const [nameTaken, setNameTaken] = useState(false)

  const loadSeq = useRef(0)
  const skillId = skill?.id ?? null
  const loadBody = useCallback(
    (retried = false) => {
      if (!skillId) return
      const seq = ++loadSeq.current
      setBodyLoading(true)
      trpc.skills.get
        .query({ id: skillId })
        .then((res) => {
          if (seq !== loadSeq.current) return
          setBody(res.skill.body)
          setSavedBody(res.skill.body)
          setBodyFailed(false)
          setBodyLoading(false)
        })
        .catch((err) => {
          if (seq !== loadSeq.current) return
          setBodyFailed(true)
          setBodyLoading(false)
          if (retried) failedAction(`load`, `the skill`, err, { retry: () => loadBody(true) })
        })
    },
    [skillId]
  )

  // Seed when the SKILL changes, never on a resync of the same one.
  useEffect(() => {
    setName(skill?.name ?? ``)
    setDescription(skill?.description ?? ``)
    setEnabledByDefault(skill?.enabledByDefault ?? false)
    setBody(``)
    setSavedBody(``)
    setBodyFailed(false)
    setSubmitting(false)
    setTouched(false)
    setNameTaken(false)
    loadBody()
    return () => {
      loadSeq.current++
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [skillId])

  const errors = validateSkillForm({ name, description, body })
  const shown = readOnly || (creating && !touched) ? [] : errors
  const errorOf = (field: SkillField) => {
    if (field === `name` && nameTaken) return PAGE.errors.nameTaken
    return shown.find((error) => error.field === field)?.message ?? null
  }
  const dirty =
    creating ||
    name.trim() !== (skill?.name ?? ``) ||
    description.trim() !== (skill?.description ?? ``) ||
    enabledByDefault !== (skill?.enabledByDefault ?? false) ||
    body !== savedBody
  const canSubmit = dirty && errors.length === 0 && !bodyLoading && !bodyFailed

  const isConflict = (err: unknown) =>
    err instanceof TRPCClientError &&
    (err.data as { code?: string } | undefined)?.code === `CONFLICT`

  const handleSubmit = async (event: React.FormEvent) => {
    event.preventDefault()
    setTouched(true)
    if (readOnly || !canSubmit || submitting) return
    const fields = { name: name.trim(), description: description.trim(), body, enabledByDefault }
    setSubmitting(true)
    setNameTaken(false)
    try {
      if (creating) {
        const { skill: created, txId } = await trpc.skills.create.mutate(
          { teamId, ...fields },
          SKIP_ERROR_TOAST
        )
        await skillCollection.utils.awaitTxId(txId).catch(() => {})
        onCreated(created.id)
      } else if (skill) {
        await trpc.skills.update.mutate({ id: skill.id, ...fields }, SKIP_ERROR_TOAST)
        setSavedBody(body)
      }
    } catch (err) {
      if (isConflict(err)) setNameTaken(true)
      else failedAction(creating ? `create` : `save`, `the skill`, err)
    } finally {
      setSubmitting(false)
    }
  }

  const nameField = pageField(`name`)
  const descriptionField = pageField(`description`)
  const bodyField = pageField(`body`)
  const nameError = errorOf(`name`)
  const descriptionError = errorOf(`description`)
  const bodyError = errorOf(`body`)

  return (
    <form onSubmit={handleSubmit} className="flex flex-col gap-3" data-testid="skill-form">
      <div className="flex flex-col gap-2">
        <GlassGroup>
          <div className="flex items-center gap-3 px-4 py-3">
            <SkillIcon className="size-4 shrink-0 text-foreground/70" />
            <Input
              id="skill-name"
              value={name}
              onChange={(event) => {
                setName(event.target.value)
                setNameTaken(false)
              }}
              placeholder={nameField.placeholder}
              aria-label={nameField.label}
              aria-invalid={Boolean(nameError) || undefined}
              className={`${BARE_FIELD_CLASS} font-mono`}
              readOnly={readOnly}
              autoFocus={creating}
              data-testid="skill-name"
            />
          </div>
          <Textarea
            id="skill-description"
            value={description}
            onChange={(event) => setDescription(event.target.value)}
            placeholder={descriptionField.placeholder}
            aria-label={descriptionField.label}
            aria-invalid={Boolean(descriptionError) || undefined}
            readOnly={readOnly}
            rows={2}
            className={`${GROUPED_FIELD_ROW} min-h-0 resize-none`}
            data-testid="skill-description"
          />
        </GlassGroup>
        <FieldError className="px-1">{nameError ?? descriptionError}</FieldError>
      </div>

      <div className="flex flex-col gap-2">
        <GlassGroup>
          {bodyLoading ? (
            <SkeletonRows dot={false} className="min-h-64 py-1" />
          ) : bodyFailed ? (
            <div className="flex min-h-64 flex-col items-center justify-center gap-2 text-sm text-muted-foreground">
              Skill unavailable
              <Button type="button" variant="outline" size="sm" onClick={() => loadBody(true)}>
                {FEEDBACK_ERRORS.retry}
              </Button>
            </div>
          ) : (
            <Textarea
              id="skill-body"
              value={body}
              onChange={(event) => setBody(event.target.value)}
              placeholder={bodyField.placeholder}
              aria-label={bodyField.label}
              aria-invalid={Boolean(bodyError) || undefined}
              readOnly={readOnly}
              rows={14}
              className={`${GROUPED_FIELD_ROW} min-h-64 resize-y font-mono text-xs`}
              data-testid="skill-body"
            />
          )}
        </GlassGroup>
        <FieldError className="px-1">{bodyError}</FieldError>
      </div>

      <GlassGroup>
        <GlassToggleRow
          id="skill-enabled-by-default"
          label={pageField(`enabledByDefault`).label}
          checked={enabledByDefault}
          onCheckedChange={setEnabledByDefault}
          disabled={readOnly}
        />
      </GlassGroup>

      {!readOnly && (
        <div className="flex justify-end gap-2">
          <Button
            type="submit"
            disabled={!canSubmit}
            pending={submitting}
            pendingLabel={FEEDBACK_WORDS.pending.save}
            data-testid="skill-submit"
          >
            {PAGE.save}
          </Button>
        </div>
      )}
    </form>
  )
}
