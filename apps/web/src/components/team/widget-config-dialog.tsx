import { useEffect, useState } from "react"
import { Trash2 } from "lucide-react"
import { useLiveQuery, eq } from "@tanstack/react-db"
import { isPickableIcon, type PickableIcon } from "@exp/icons"
import { PICKABLE_ICON_SVG } from "@exp/icons/pickable-svg"
import { defaultLauncher } from "@exp/widget/launcher"
import type {
  WidgetLauncherMode,
  WidgetLauncherPlacement,
  WidgetLauncherPosition,
} from "@exp/widget/types"
import { trpc } from "@/lib/trpc-client"
import { labelCollection } from "@/lib/collections"
import { cn } from "@/lib/utils"
import {
  DEFAULT_ACCENT,
  WidgetLauncherPreviewViewport,
} from "@/components/widget-launcher-preview"
import { WidgetPanelPreview } from "@/components/widget-panel-preview"
import { IconPicker } from "@/components/ui/icon-picker"
import type { BoardIcon } from "@exp/db-schema/domain"
import { LabelPicker } from "@/components/issue-properties/label-picker"
import { Button } from "@/components/ui/button"
import {
  Dialog,
  DialogBody,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { Switch } from "@/components/ui/switch"
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { Textarea } from "@/components/ui/textarea"
import type { Label as TeamLabel } from "@/db/schema"

// The create/edit dialog extracted from widget-section.tsx (EXP-435), now
// organized into General / Form / Appearance tabs with a live whole-panel
// preview beside the appearance knobs.

export type WidgetListItem = Awaited<
  ReturnType<typeof trpc.widgets.list.query>
>[number]

// Client mirror of the server's maxWidgetLabels (lib/widget/service.ts is
// server-only — importing it here would drag db/crypto into the bundle).
const maxWidgetLabels = 10

export function parseDomains(value: string): string[] {
  return value
    .split(/[\n,]/)
    .map((domain) => domain.trim())
    .filter((domain) => domain.length > 0)
}

type WidgetThemeChoice = `dark` | `light` | `auto`
type WidgetDevice = `desktop` | `mobile`

// Row-major picker order for the 3×2 position grid.
const launcherPositionGrid: WidgetLauncherPosition[] = [
  `top-left`,
  `top-right`,
  `middle-left`,
  `middle-right`,
  `bottom-left`,
  `bottom-right`,
]
// The settings-facing shape of formConfig.modes: a single pick instead of a
// multi-select (there are only three valid combinations).
type WidgetModeChoice = `feedback` | `support` | `both`

// Editor rows for the owner-defined custom fields (EXP-244). `key` is null
// for a not-yet-saved row — it's derived from the label at save time and
// then stays STABLE across renames (submitted values live under it in the
// customData blob).
interface CustomFieldRow {
  key: string | null
  label: string
  required: boolean
}

// Derive a storage key from a field label; must satisfy the server's
// /^[a-z0-9][a-z0-9_-]{0,39}$/ pattern.
function slugifyFieldKey(label: string): string {
  const slug = label
    .toLowerCase()
    .replace(/[^a-z0-9_-]+/g, `-`)
    .replace(/^[-_]+/, ``)
    .slice(0, 40)
    .replace(/[-_]+$/, ``)
  return slug || `field`
}

// The stored launcher blob (EXP-569), mirroring the server's precedence: a
// stored per-device entry > the legacy two-value `position` (an explicit
// pre-EXP-569 corner choice, honored as a fab) > the defaults.
function readLauncher(raw: Record<string, unknown> | null): {
  desktop: WidgetLauncherPlacement
  mobile: WidgetLauncherPlacement
  icon: PickableIcon | ``
} {
  const launcher =
    raw?.launcher !== null && typeof raw?.launcher === `object`
      ? (raw.launcher as Record<string, unknown>)
      : {}
  const legacy =
    raw?.position === `bottom-left` || raw?.position === `bottom-right`
      ? ({ mode: `fab`, position: raw.position } as const)
      : null
  const device = (key: WidgetDevice): WidgetLauncherPlacement => {
    const entry = launcher[key]
    if (entry !== null && typeof entry === `object`) {
      const { mode, position } = entry as Record<string, unknown>
      if (
        (mode === `fab` || mode === `tab`) &&
        typeof position === `string` &&
        launcherPositionGrid.includes(position as WidgetLauncherPosition)
      ) {
        return { mode, position: position as WidgetLauncherPosition }
      }
    }
    return legacy ?? defaultLauncher[key]
  }
  return {
    desktop: device(`desktop`),
    mobile: device(`mobile`),
    icon:
      typeof launcher.icon === `string` && isPickableIcon(launcher.icon)
        ? launcher.icon
        : ``,
  }
}

// The styling knobs stored in widget_configs.form_config (jsonb) — read
// defensively, rows may predate any of the fields.
function readFormConfig(raw: Record<string, unknown> | null): {
  buttonLabel: string
  accentColor: string
  launcher: { desktop: WidgetLauncherPlacement; mobile: WidgetLauncherPlacement }
  icon: PickableIcon | ``
  emailRequired: boolean
  collectEmail: boolean
  collectName: boolean
  nameRequired: boolean
  customFields: CustomFieldRow[]
  mode: WidgetModeChoice
  labelIds: string[]
  theme: WidgetThemeChoice
} {
  const modes = Array.isArray(raw?.modes) ? raw.modes : []
  const hasSupport = modes.includes(`support`)
  const hasFeedback = modes.includes(`feedback`) || !hasSupport
  const customFields = (
    Array.isArray(raw?.customFields) ? raw.customFields : []
  ).flatMap((entry): CustomFieldRow[] => {
    if (entry === null || typeof entry !== `object`) return []
    const { key, label, required } = entry as Record<string, unknown>
    if (typeof key !== `string` || typeof label !== `string`) return []
    return [{ key, label, required: required === true }]
  })
  const readHex = (value: unknown): string =>
    typeof value === `string` && /^#[0-9a-fA-F]{6}$/.test(value) ? value : ``
  const launcher = readLauncher(raw)
  return {
    buttonLabel: typeof raw?.buttonLabel === `string` ? raw.buttonLabel : ``,
    accentColor: readHex(raw?.accentColor),
    launcher: { desktop: launcher.desktop, mobile: launcher.mobile },
    icon: launcher.icon,
    emailRequired: raw?.emailRequired === true,
    collectEmail: raw?.collectEmail !== false,
    collectName: raw?.collectName === true,
    nameRequired: raw?.nameRequired === true,
    customFields,
    mode: hasSupport ? (hasFeedback ? `both` : `support`) : `feedback`,
    labelIds: (Array.isArray(raw?.labelIds) ? raw.labelIds : [])
      .filter((id): id is string => typeof id === `string`)
      .slice(0, maxWidgetLabels),
    // Absent = dark, the pre-theme behavior.
    theme: raw?.theme === `light` || raw?.theme === `auto` ? raw.theme : `dark`,
  }
}

function modesForChoice(
  choice: WidgetModeChoice
): Array<`feedback` | `support`> {
  if (choice === `both`) return [`feedback`, `support`]
  return [choice]
}

export function WidgetConfigDialog({
  open,
  onOpenChange,
  teamId,
  boards,
  editTarget,
  onSaved,
}: {
  open: boolean
  onOpenChange(open: boolean): void
  teamId: string
  boards: { id: string; name: string }[]
  // null → create.
  editTarget: WidgetListItem | null
  // Called after a successful save; `created` is the fresh row on create
  // (the parent opens the snippet dialog for it), null on edit.
  onSaved(created: WidgetListItem | null): void | Promise<void>
}) {
  const [formName, setFormName] = useState(``)
  const [formBoardId, setFormBoardId] = useState<string>(``)
  const [formDomains, setFormDomains] = useState(``)
  const [formButtonLabel, setFormButtonLabel] = useState(``)
  // Empty string = "use the widget default" (the color key is omitted).
  const [formAccent, setFormAccent] = useState(``)
  const [formTheme, setFormTheme] = useState<WidgetThemeChoice>(`dark`)
  // Per-device launcher appearance (EXP-569) + the shared icon (empty = the
  // built-in megaphone, omitted from the blob).
  const [formLauncher, setFormLauncher] = useState<
    Record<WidgetDevice, WidgetLauncherPlacement>
  >({ desktop: defaultLauncher.desktop, mobile: defaultLauncher.mobile })
  const [formIcon, setFormIcon] = useState<PickableIcon | ``>(``)
  // Which device the launcher controls + preview show (UI-only).
  const [launcherDevice, setLauncherDevice] = useState<WidgetDevice>(`desktop`)
  const [formEmailRequired, setFormEmailRequired] = useState(false)
  const [formCollectEmail, setFormCollectEmail] = useState(true)
  const [formCollectName, setFormCollectName] = useState(false)
  const [formNameRequired, setFormNameRequired] = useState(false)
  const [formCustomFields, setFormCustomFields] = useState<CustomFieldRow[]>([])
  const [formLabelIds, setFormLabelIds] = useState<string[]>([])
  const [formMode, setFormMode] = useState<WidgetModeChoice>(`feedback`)
  const [saving, setSaving] = useState(false)
  const [formError, setFormError] = useState<string | null>(null)

  // Reset the form from the target every time the dialog opens.
  useEffect(() => {
    if (!open) return
    const config = readFormConfig(editTarget?.formConfig ?? null)
    setFormName(editTarget?.name ?? ``)
    setFormBoardId(editTarget ? (editTarget.boardId ?? ``) : (boards[0]?.id ?? ``))
    setFormDomains(editTarget?.allowedDomains.join(`\n`) ?? ``)
    setFormButtonLabel(config.buttonLabel)
    setFormAccent(config.accentColor)
    setFormTheme(config.theme)
    setFormLauncher(config.launcher)
    setFormIcon(config.icon)
    setLauncherDevice(`desktop`)
    setFormEmailRequired(config.emailRequired)
    setFormCollectEmail(config.collectEmail)
    setFormCollectName(config.collectName)
    setFormNameRequired(config.nameRequired)
    setFormCustomFields(config.customFields)
    setFormLabelIds(config.labelIds)
    setFormMode(config.mode)
    setFormError(null)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, editTarget])

  // The team's labels (Electric-synced) — the picker's source of truth AND
  // the preview's name/color lookup. Save derives labelIds from this list so
  // ids of since-deleted labels silently drop instead of failing validation.
  const { data: teamLabels } = useLiveQuery(
    (q) =>
      teamId
        ? q
            .from({ labels: labelCollection })
            .where(({ labels }) => eq(labels.teamId, teamId))
            .orderBy(({ labels }) => labels.sortOrder)
        : undefined,
    [teamId]
  )
  const selectedLabels = (teamLabels ?? []).filter((label: TeamLabel) =>
    formLabelIds.includes(label.id)
  )

  const toggleFormLabel = (labelId: string) => {
    setFormLabelIds((previous) => {
      if (previous.includes(labelId)) {
        return previous.filter((id) => id !== labelId)
      }
      if (previous.length >= maxWidgetLabels) return previous
      return [...previous, labelId]
    })
  }

  // Rows with a label get a stable key: existing keys survive renames (the
  // submitted values live under them), new rows derive one from the label
  // with a numeric suffix on collision.
  const buildCustomFields = () => {
    const out: Array<{ key: string; label: string; required: boolean }> = []
    const seen = new Set<string>()
    for (const row of formCustomFields) {
      const label = row.label.trim().slice(0, 40)
      if (!label) continue
      let key = row.key ?? slugifyFieldKey(label)
      if (seen.has(key)) {
        const base = key.slice(0, 37)
        let suffix = 2
        while (seen.has(`${base}-${suffix}`)) suffix += 1
        key = `${base}-${suffix}`
      }
      seen.add(key)
      out.push({ key, label, required: row.required })
    }
    return out
  }

  // Defaults are omitted to keep the jsonb blob lean (collectEmail true,
  // collectName/nameRequired false, theme dark, no icon/labels). The
  // launcher is always written — reading it back is what keeps a row stable
  // once the serve-time defaults change again. Required always implies
  // collect on the server, so a hidden email field never re-enforces.
  const buildFormConfig = () => {
    const customFields = buildCustomFields()
    const labelIds = selectedLabels.map((label: TeamLabel) => label.id)
    return {
      ...(formButtonLabel.trim()
        ? { buttonLabel: formButtonLabel.trim() }
        : {}),
      ...(formAccent ? { accentColor: formAccent } : {}),
      launcher: {
        desktop: formLauncher.desktop,
        mobile: formLauncher.mobile,
        ...(formIcon ? { icon: formIcon } : {}),
      },
      // Legacy two-value position for cached pre-EXP-569 widget bundles:
      // the desktop launcher's corner.
      position: formLauncher.desktop.position.endsWith(`left`)
        ? (`bottom-left` as const)
        : (`bottom-right` as const),
      emailRequired: formCollectEmail && formEmailRequired,
      ...(formCollectEmail ? {} : { collectEmail: false }),
      ...(formCollectName ? { collectName: true } : {}),
      ...(formCollectName && formNameRequired ? { nameRequired: true } : {}),
      ...(customFields.length > 0 ? { customFields } : {}),
      modes: modesForChoice(formMode),
      ...(labelIds.length > 0 ? { labelIds } : {}),
      ...(formTheme !== `dark` ? { theme: formTheme } : {}),
    }
  }

  const updateLauncher = (patch: Partial<WidgetLauncherPlacement>) =>
    setFormLauncher((previous) => ({
      ...previous,
      [launcherDevice]: { ...previous[launcherDevice], ...patch },
    }))

  // A feedback board is required whenever the widget offers feedback mode; a
  // support-only widget has none (tickets go to the team support inbox).
  const needsBoard = formMode !== `support`
  const canSave =
    Boolean(formName.trim()) &&
    (!needsBoard || Boolean(formBoardId)) &&
    parseDomains(formDomains).length > 0

  const save = async () => {
    if (!canSave) {
      setFormError(
        needsBoard
          ? `Name, feedback board, and at least one allowed domain are required.`
          : `Name and at least one allowed domain are required.`
      )
      return
    }
    setSaving(true)
    setFormError(null)
    const boardId = needsBoard ? formBoardId : null
    try {
      let created: WidgetListItem | null = null
      if (editTarget) {
        await trpc.widgets.update.mutate({
          widgetConfigId: editTarget.id,
          name: formName.trim(),
          boardId,
          allowedDomains: parseDomains(formDomains),
          formConfig: buildFormConfig(),
        })
      } else {
        const row = await trpc.widgets.create.mutate({
          teamId,
          boardId,
          name: formName.trim(),
          allowedDomains: parseDomains(formDomains),
          formConfig: buildFormConfig(),
        })
        created = {
          ...row,
          boardName:
            boards.find((board) => board.id === row.boardId)?.name ?? null,
          submissionCount: 0,
        }
      }
      onOpenChange(false)
      await onSaved(created)
    } catch (saveError) {
      setFormError(
        saveError instanceof Error ? saveError.message : `Couldn't save widget`
      )
    } finally {
      setSaving(false)
    }
  }

  // A hex color input + reset, matching the pre-EXP-435 accent pattern.
  const colorField = (
    id: string,
    label: string,
    value: string,
    fallback: string,
    onChange: (next: string) => void
  ) => (
    <div className="space-y-2">
      <Label htmlFor={id}>{label}</Label>
      <div className="flex items-center gap-2">
        <Input
          id={id}
          type="color"
          value={value || fallback}
          onChange={(event) => onChange(event.target.value)}
          className="h-9 w-14 cursor-pointer p-1"
        />
        {value ? (
          <Button variant="ghost" size="sm" onClick={() => onChange(``)}>
            Reset
          </Button>
        ) : (
          <span className="text-xs text-muted-foreground">Default</span>
        )}
      </div>
    </div>
  )

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      {/* Wide on desktop (EXP-267): this is the longest form dialog in the
          app — since EXP-435 it's organized into General / Form / Appearance
          tabs, with the whole-panel preview beside the appearance knobs. */}
      <DialogContent className="sm:max-w-3xl">
        <DialogHeader>
          <DialogTitle>{editTarget ? `Edit widget` : `New widget`}</DialogTitle>
          <DialogDescription>
            Feedback submissions create issues on the selected board; support
            tickets land in the team&apos;s Support inbox. The key in the
            snippet is public; restrict it to your domains.
          </DialogDescription>
        </DialogHeader>
        <DialogBody>
          <Tabs defaultValue="general">
            <TabsList className="w-full sm:w-auto">
              <TabsTrigger value="general">General</TabsTrigger>
              <TabsTrigger value="form">Form</TabsTrigger>
              <TabsTrigger value="appearance">Appearance</TabsTrigger>
            </TabsList>

            <TabsContent value="general" className="mt-2 space-y-4">
              <div className="space-y-2">
                <Label htmlFor="widget-name">Name</Label>
                <Input
                  id="widget-name"
                  placeholder="Acme App"
                  value={formName}
                  onChange={(event) => setFormName(event.target.value)}
                />
              </div>
              <div className="space-y-2">
                <Label>Modes</Label>
                <Select
                  value={formMode}
                  onValueChange={(value) =>
                    setFormMode(value as WidgetModeChoice)
                  }
                >
                  <SelectTrigger>
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="feedback">Feedback</SelectItem>
                    <SelectItem value="support">Support</SelectItem>
                    <SelectItem value="both">Feedback + support</SelectItem>
                  </SelectContent>
                </Select>
                {formMode !== `feedback` && (
                  <p className="text-xs text-muted-foreground">
                    Support files helpdesk tickets. Visitors get a
                    reply-by-email conversation. Requires the helpdesk to be
                    enabled for this team (below).
                  </p>
                )}
              </div>
              {needsBoard && (
                <div className="space-y-2">
                  <Label>Feedback board</Label>
                  <Select value={formBoardId} onValueChange={setFormBoardId}>
                    <SelectTrigger>
                      <SelectValue placeholder="Select a board" />
                    </SelectTrigger>
                    <SelectContent>
                      {boards.map((board) => (
                        <SelectItem key={board.id} value={board.id}>
                          {board.name}
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                  <p className="text-xs text-muted-foreground">
                    Feedback submissions land on this board as issues.
                  </p>
                </div>
              )}
              <div className="space-y-2">
                <Label htmlFor="widget-domains">Allowed domains</Label>
                <Textarea
                  id="widget-domains"
                  placeholder={`app.example.com\n*.example.com\nlocalhost:5173`}
                  value={formDomains}
                  onChange={(event) => setFormDomains(event.target.value)}
                  className="min-h-20 font-mono text-xs"
                />
                <p className="text-xs text-muted-foreground">
                  One per line. `*.example.com` matches subdomains only. At
                  least one domain is required.
                </p>
              </div>
            </TabsContent>

            <TabsContent value="form" className="mt-2 space-y-4">
              {/* EXP-244 replaced EXP-267's single "Require email" cell with
                  per-field Show/Required switches. */}
              <div className="space-y-2">
                <Label>Form fields</Label>
                <div className="space-y-3 rounded-md border px-3 py-3">
                  {needsBoard && (
                    <div className="flex items-center justify-between gap-2">
                      <span className="text-sm">Email</span>
                      <div className="flex items-center gap-4">
                        <Label
                          htmlFor="widget-collect-email"
                          className="gap-1.5 text-xs font-normal text-muted-foreground"
                        >
                          Show
                          <Switch
                            id="widget-collect-email"
                            checked={formCollectEmail}
                            onCheckedChange={setFormCollectEmail}
                          />
                        </Label>
                        <Label
                          htmlFor="widget-email-required"
                          className="gap-1.5 text-xs font-normal text-muted-foreground"
                        >
                          Required
                          <Switch
                            id="widget-email-required"
                            checked={formCollectEmail && formEmailRequired}
                            disabled={!formCollectEmail}
                            onCheckedChange={setFormEmailRequired}
                          />
                        </Label>
                      </div>
                    </div>
                  )}
                  <div className="flex items-center justify-between gap-2">
                    <span className="text-sm">Name</span>
                    <div className="flex items-center gap-4">
                      <Label
                        htmlFor="widget-collect-name"
                        className="gap-1.5 text-xs font-normal text-muted-foreground"
                      >
                        Show
                        <Switch
                          id="widget-collect-name"
                          checked={formCollectName}
                          onCheckedChange={setFormCollectName}
                        />
                      </Label>
                      <Label
                        htmlFor="widget-name-required"
                        className="gap-1.5 text-xs font-normal text-muted-foreground"
                      >
                        Required
                        <Switch
                          id="widget-name-required"
                          checked={formCollectName && formNameRequired}
                          disabled={!formCollectName}
                          onCheckedChange={setFormNameRequired}
                        />
                      </Label>
                    </div>
                  </div>
                </div>
                <p className="text-xs text-muted-foreground">
                  {needsBoard
                    ? `Hide the email field for teams that follow up in person. A name is often all you need. Support mode always asks for an email (it's the reply channel).`
                    : `Support mode always asks for an email (it's the reply channel); the name field is optional.`}
                </p>
              </div>
              {needsBoard && (
                <div className="space-y-2">
                  <Label>Labels</Label>
                  <LabelPicker
                    teamId={teamId}
                    selectedLabelIds={formLabelIds}
                    onToggle={toggleFormLabel}
                    renderTrigger={(selected) => (
                      <Button
                        variant="outline"
                        className="h-auto min-h-9 w-full flex-wrap justify-start gap-1.5 font-normal"
                      >
                        {selected.length === 0 ? (
                          <span className="text-muted-foreground">
                            Select labels…
                          </span>
                        ) : (
                          selected.map((label: TeamLabel) => (
                            <span
                              key={label.id}
                              className="inline-flex items-center gap-1.5 rounded-full border px-2 py-0.5 text-xs"
                            >
                              <span
                                className="h-2 w-2 rounded-full"
                                style={{ backgroundColor: label.color }}
                              />
                              {label.name}
                            </span>
                          ))
                        )}
                      </Button>
                    )}
                  />
                  <p className="text-xs text-muted-foreground">
                    Visitors can tag their report with these labels (up to
                    {` `}
                    {maxWidgetLabels}).
                  </p>
                </div>
              )}
              {needsBoard && (
                <div className="space-y-2">
                  <div className="flex items-center justify-between">
                    <Label>Custom fields</Label>
                    <Button
                      variant="outline"
                      size="sm"
                      disabled={formCustomFields.length >= 8}
                      onClick={() =>
                        setFormCustomFields((rows) => [
                          ...rows,
                          { key: null, label: ``, required: false },
                        ])
                      }
                    >
                      Add field
                    </Button>
                  </div>
                  {formCustomFields.length > 0 && (
                    <div className="space-y-2">
                      {formCustomFields.map((row, index) => (
                        <div
                          // eslint-disable-next-line react/no-array-index-key
                          key={index}
                          className="flex items-center gap-2"
                        >
                          <Input
                            placeholder="Field label"
                            maxLength={40}
                            value={row.label}
                            onChange={(event) =>
                              setFormCustomFields((rows) =>
                                rows.map((current, i) =>
                                  i === index
                                    ? { ...current, label: event.target.value }
                                    : current
                                )
                              )
                            }
                          />
                          <Label className="gap-1.5 text-xs font-normal text-muted-foreground">
                            Required
                            <Switch
                              checked={row.required}
                              onCheckedChange={(next) =>
                                setFormCustomFields((rows) =>
                                  rows.map((current, i) =>
                                    i === index
                                      ? { ...current, required: next }
                                      : current
                                  )
                                )
                              }
                            />
                          </Label>
                          <Button
                            variant="ghost"
                            size="icon"
                            aria-label="Remove field"
                            onClick={() =>
                              setFormCustomFields((rows) =>
                                rows.filter((_, i) => i !== index)
                              )
                            }
                          >
                            <Trash2 className="h-4 w-4" />
                          </Button>
                        </div>
                      ))}
                    </div>
                  )}
                  <p className="text-xs text-muted-foreground">
                    Extra text inputs on the feedback form (up to 8). Responses
                    show under “Custom data” on the issue and override matching
                    setCustomData keys.
                  </p>
                </div>
              )}
              {!needsBoard && (
                <p className="text-xs text-muted-foreground">
                  Labels and custom fields apply to the feedback form only.
                </p>
              )}
            </TabsContent>

            <TabsContent value="appearance" className="mt-2">
              <div className="grid gap-4 sm:grid-cols-2 sm:gap-x-6">
                <div className="space-y-4">
                  <div className="space-y-2">
                    <Label>Theme</Label>
                    <Select
                      value={formTheme}
                      onValueChange={(value) =>
                        setFormTheme(value as WidgetThemeChoice)
                      }
                    >
                      <SelectTrigger>
                        <SelectValue />
                      </SelectTrigger>
                      <SelectContent>
                        <SelectItem value="dark">Dark</SelectItem>
                        <SelectItem value="light">Light</SelectItem>
                        <SelectItem value="auto">
                          Match visitor&apos;s system
                        </SelectItem>
                      </SelectContent>
                    </Select>
                    <p className="text-xs text-muted-foreground">
                      Your site can also switch it live via
                      {` `}
                      <code className="font-mono text-[11px]">
                        ExponentialWidget.setTheme(…)
                      </code>
                      .
                    </p>
                  </div>
                  <div className="space-y-2">
                    <Label htmlFor="widget-button-label">Button label</Label>
                    <Input
                      id="widget-button-label"
                      placeholder="Feedback"
                      maxLength={40}
                      value={formButtonLabel}
                      onChange={(event) =>
                        setFormButtonLabel(event.target.value)
                      }
                    />
                  </div>
                  {colorField(
                    `widget-accent`,
                    `Button color`,
                    formAccent,
                    DEFAULT_ACCENT,
                    setFormAccent
                  )}
                  <div className="space-y-2">
                    <Label>Icon</Label>
                    <IconPicker
                      // PickableIcon and BoardIcon are byte-equal by
                      // construction (locked by lib/icons.test.ts).
                      value={(formIcon || `megaphone`) as BoardIcon}
                      onChange={(icon) =>
                        // The megaphone IS the default — store nothing.
                        setFormIcon(
                          icon === `megaphone` ? `` : (icon as PickableIcon)
                        )
                      }
                    />
                  </div>
                  <div className="space-y-2">
                    <Label>Launcher</Label>
                    <Tabs
                      value={launcherDevice}
                      onValueChange={(value) =>
                        setLauncherDevice(value as WidgetDevice)
                      }
                    >
                      <TabsList className="w-full">
                        <TabsTrigger value="desktop" className="flex-1">
                          Desktop
                        </TabsTrigger>
                        <TabsTrigger value="mobile" className="flex-1">
                          Mobile
                        </TabsTrigger>
                      </TabsList>
                    </Tabs>
                    <div className="flex items-center justify-between gap-3 pt-1">
                      <div className="flex flex-col gap-1">
                        {(
                          [
                            [`fab`, `Floating button`],
                            [`tab`, `Edge tab`],
                          ] as Array<[WidgetLauncherMode, string]>
                        ).map(([mode, label]) => (
                          <Button
                            key={mode}
                            type="button"
                            size="sm"
                            variant={
                              formLauncher[launcherDevice].mode === mode
                                ? `secondary`
                                : `ghost`
                            }
                            className="justify-start"
                            onClick={() => updateLauncher({ mode })}
                          >
                            {label}
                          </Button>
                        ))}
                      </div>
                      <div
                        className="grid grid-cols-2 gap-1 rounded-md border bg-muted/30 p-1"
                        role="radiogroup"
                        aria-label="Launcher position"
                      >
                        {launcherPositionGrid.map((position) => {
                          const selected =
                            formLauncher[launcherDevice].position === position
                          return (
                            <button
                              key={position}
                              type="button"
                              role="radio"
                              aria-checked={selected}
                              aria-label={position.replace(`-`, ` `)}
                              title={position.replace(`-`, ` `)}
                              className={cn(
                                `flex h-7 w-9 items-center justify-center rounded-sm transition-colors`,
                                selected ? `bg-accent` : `hover:bg-muted`
                              )}
                              onClick={() => updateLauncher({ position })}
                            >
                              <span
                                className={cn(
                                  `h-2 w-2 rounded-full`,
                                  selected
                                    ? `bg-foreground`
                                    : `bg-muted-foreground/40`
                                )}
                              />
                            </button>
                          )
                        })}
                      </div>
                    </div>
                    <p className="text-xs text-muted-foreground">
                      Defaults: floating button bottom right on desktop, edge
                      tab middle right on mobile.
                    </p>
                  </div>
                </div>
                <div className="space-y-2">
                  <span className="text-xs text-muted-foreground">
                    Launcher preview (
                    {launcherDevice === `desktop` ? `desktop` : `mobile`}) —
                    hover it
                  </span>
                  <WidgetLauncherPreviewViewport
                    mode={formLauncher[launcherDevice].mode}
                    position={formLauncher[launcherDevice].position}
                    accentColor={formAccent || undefined}
                    label={formButtonLabel.trim() || undefined}
                    theme={formTheme === `light` ? `light` : `dark`}
                    iconSvg={formIcon ? PICKABLE_ICON_SVG[formIcon] : undefined}
                  />
                  <span className="text-xs text-muted-foreground">
                    Widget preview
                  </span>
                  <WidgetPanelPreview
                    theme={formTheme}
                    accentColor={formAccent}
                    labels={selectedLabels.map((label: TeamLabel) => ({
                      id: label.id,
                      name: label.name,
                      color: label.color,
                    }))}
                    collectEmail={needsBoard && formCollectEmail}
                    emailRequired={formCollectEmail && formEmailRequired}
                    collectName={formCollectName}
                    customFieldLabels={formCustomFields
                      .map((row) => row.label.trim())
                      .filter((label) => label.length > 0)}
                  />
                </div>
              </div>
            </TabsContent>
          </Tabs>
          {formError && (
            <p className="mt-3 text-sm text-destructive">{formError}</p>
          )}
        </DialogBody>
        <DialogFooter>
          <Button
            variant="ghost"
            onClick={() => onOpenChange(false)}
            disabled={saving}
          >
            Cancel
          </Button>
          <Button onClick={save} disabled={saving || !canSave}>
            {saving ? `Saving…` : editTarget ? `Save` : `Create widget`}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
