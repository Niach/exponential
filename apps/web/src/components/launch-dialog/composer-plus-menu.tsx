import { useMemo, useState } from "react"
import type { IconConcept } from "@exp/icons"
import {
  AccountOptionRow,
  Button,
  ComposerTool,
  Menu,
  PickerMenuRows,
  SearchList,
  choicePickerItems,
  conceptIcon,
  devicePickerItems,
  getActionIcon,
  repositoryPickerItems,
  useIsMobile,
  type AccountPickerOption,
  type DevicePickerDevice,
  type MenuEntry,
  type PickerItem,
  type RepositoryPickerRow,
  type ValueChoice,
} from "@exp/ui"
import type { LaunchComposerModel } from "@/hooks/use-launch-composer"
import type { McpServerList } from "@/lib/mcp-servers"
import {
  agentEffortValues,
  agentSupportsUltracode,
} from "@/lib/coding-launch-prefs"
import { ComposerIssuePicker } from "@/components/launch-dialog/issue-picker"
import { IssuePickerBody } from "@/components/issue-picker-host"
import { useIssueRefs, type ResolvedIssueRef } from "@/components/issue-ref-provider"
import {
  CLI_DEFAULT_EFFORT,
  effortLabel,
  modelChoices,
} from "@/components/launch-dialog/launch-options-pane"
import { McpServerMenuRows } from "@/components/launch-dialog/mcp-server-picker"
import { healthBadgeLabel } from "@/lib/agent-usage"
import { accountOptionKey } from "@/lib/accounts/account-option"
import { ISSUE_SEARCH_NO_RESULTS, ISSUE_SEARCH_PLACEHOLDER } from "@/lib/issue-search"
import { filterSlashPrompts, promptSlashName, type SlashPrompt } from "@/lib/steer-commands"
import {
  COMPOSER_MENU_TEST_IDS,
  COMPOSER_PLUS_LABEL,
  composerMenuLayout,
  composerMenuRowTestId,
  implementButtonLabel,
  type ComposerMenuRow,
  type ComposerMenuSurface,
} from "@/components/launch-dialog/composer-menu"

// EXP-1249 / S-3: EVERY composer's ONE "+" (composer-menu.json, the launch
// composer, a session's and an agent chat's: `surface`), its only tool: Photo
// or file, Issue › (launch: the implement subject; session/agent: THE issue
// search list, a pick inserts `#IDENT` at the caret), Prompts › (#EXP-1434:
// THE search list over the team's prompts, a pick = the `/` menu's pick), the
// RUNNER submenus
// over THE existing pickers' rows (Device ›, Account ›, Model ›, Repository ›;
// each a search list, value = the picked label) and, on a Temporary-session
// launch, the work picks (Effort ›, Ultracode, MCP servers ›, Computer use),
// as ONE `Menu`: a dropdown at the "+" from md up, the bottom sheet of the
// same rows on a phone (submenus as pushed pages). Which rows show, their
// order, glyphs and copy are `composer-menu.ts` (fixture-locked ×4); this
// file binds each row to the composer model. No options line, no inline
// tools (S-3).

const UiAddIcon = conceptIcon(`ui-add`)

const CLI_DEFAULT_LABEL = `CLI default`

/** SLOP-28: the submenus are the search list (`composer-menu.json`
 *  `submenuRowHeight` × `submenuMaxRows`), at one width. On a pointer they
 *  open to the side on hover or →, ← and Escape close back (`Menu`'s
 *  submenu body); on a phone they push as pages. */
const SUBMENU_LIST_CLASS = `w-[26rem] p-0`
const SUBMENU_PICKER_CLASS = `w-[20rem] p-0`

/** The work picks a Temporary-session launch reads and writes
 *  (`LaunchOptions`). */
export interface ComposerOptionsBinding {
  agent: string
  effortValue: string
  setEffortValue: (value: string) => void
  ultracode: boolean
  setUltracode: (value: boolean) => void
  computerUse: boolean
  setComputerUse: (value: boolean) => void
  computerUseAvailable: boolean
  mcpServers: McpServerList
  mcpServerIds: string[]
  toggleMcpServer: (id: string) => void
  mcpConnectHref: (serverId: string) => string | null
}

/** S-3: the runner submenus, each present only where it can change
 *  (composer-menu.json `conditions`: device / account / model). */
export interface ComposerRunnerBinding {
  device?: {
    value: string | null
    devices: readonly DevicePickerDevice[]
    onChange: (deviceId: string) => void
  }
  account?: {
    value: string | null
    options: readonly AccountPickerOption[]
    onChange: (key: string) => void
  }
  model?: {
    value: string | null
    models: readonly ValueChoice[]
    onChange: (value: string) => void
  }
  /** Shown only while the team has more than one repository. */
  repository?: {
    value: string | null
    repositories: readonly RepositoryPickerRow[]
    onChange: (repositoryId: string) => void
  }
}

/** Issue ›: the launch's implement subject, or (a session, an agent chat)
 *  THE issue search list whose pick inserts `#IDENT` at the caret. */
export type ComposerIssueBinding =
  | {
      kind: `implement`
      teamId: string
      eligibleIssues: LaunchComposerModel[`eligibleIssues`]
      checkedIssues: LaunchComposerModel[`checkedIssues`]
      /** The picked issue ids' count (the subject's, ahead of the rows). */
      checkedCount: number
      toggleIssue: (issueId: string) => void
    }
  | {
      kind: `insert`
      onPick: (identifier: string) => void
    }

/** Prompts › (#EXP-1434): the team's prompts in the `/` menu's order; a
 *  pick does what picking it in that composer's `/` menu does. */
export interface ComposerPromptsBinding {
  prompts: readonly SlashPrompt[]
  onPick: (prompt: SlashPrompt) => void
}

/** THE "+" of every composer ×4. */
export function ComposerOptionsMenu({
  surface,
  agent,
  temporary = false,
  options,
  runner = {},
  issues,
  prompts,
  onAttach,
  disabled,
  rowsDisabled,
  highlighted,
}: {
  surface: ComposerMenuSurface
  /** The agent CLI the picks are for (`codexLabel`, ultracode). */
  agent: string
  /** Launch: the agent row picks Temporary session (the work rows show). */
  temporary?: boolean
  /** Launch while temporary: the work picks. */
  options?: ComposerOptionsBinding
  runner?: ComposerRunnerBinding
  issues?: ComposerIssueBinding
  /** Prompts ›, shown while the team has a prompt. */
  prompts?: ComposerPromptsBinding
  /** Opens the file chooser (the composer owns the hidden input). */
  onAttach: () => void
  disabled?: boolean
  /** The rows read but cannot change (an ended session). */
  rowsDisabled?: boolean
  /** The "+" reads as active (a subject or a pending file is up). */
  highlighted?: boolean
}) {
  const [open, setOpen] = useState(false)
  const ultracodeOn = options !== undefined && options.ultracode && agentSupportsUltracode(agent)
  const checkedCount = issues?.kind === `implement` ? issues.checkedCount : 0
  const work = surface === `launch` && temporary && options !== undefined

  const effortOptions: ValueChoice[] = [
    { value: CLI_DEFAULT_EFFORT, label: CLI_DEFAULT_LABEL },
    ...agentEffortValues(agent).map((value) => ({
      value,
      label: effortLabel(value),
    })),
  ]
  const effortValue =
    !options || options.effortValue === `` ? CLI_DEFAULT_EFFORT : options.effortValue
  const labelOf = (choices: readonly { value: string; label: string }[], value: string | null) =>
    choices.find((choice) => choice.value === value)?.label

  const layout = composerMenuLayout(surface, {
    temporary: surface === `launch` && temporary,
    device: runner.device !== undefined,
    account: runner.account !== undefined,
    model: runner.model !== undefined,
    repositories: (runner.repository?.repositories.length ?? 0) > 1,
    // EXP-1293: `sessions.send` carries no subagent model; the row hides.
    subagentModel: false,
    ultracode: work && agentSupportsUltracode(agent),
    mcp: work && options.mcpServers.length > 0,
    computerUse: work && options.computerUseAvailable,
    prompts: (prompts?.prompts.length ?? 0) > 0,
  })

  const pick =
    <T,>(onChange: (value: T) => void) =>
    (value: T) => {
      onChange(value)
      setOpen(false)
    }

  const entryFor = (row: ComposerMenuRow): MenuEntry | null => {
    const base = {
      id: row.id,
      label: agent === `codex` && row.codexLabel ? row.codexLabel : row.label,
      icon: conceptIcon(row.icon as IconConcept),
      "data-testid": composerMenuRowTestId(row.id),
    }
    switch (row.id) {
      case `add-file`:
        return { ...base, kind: `item`, onSelect: onAttach }
      case `implement-issue`:
        if (!issues) return null
        if (issues.kind === `insert`) {
          return {
            ...base,
            kind: `submenu`,
            contentClassName: SUBMENU_LIST_CLASS,
            body: <InsertIssueList onPick={pick(issues.onPick)} />,
          }
        }
        return {
          ...base,
          kind: `submenu`,
          value: checkedCount > 0 ? String(checkedCount) : undefined,
          contentClassName: SUBMENU_LIST_CLASS,
          body: (
            <ComposerIssuePicker
              teamId={issues.teamId}
              eligible={issues.eligibleIssues}
              checked={issues.checkedIssues}
              onToggle={issues.toggleIssue}
              footer={
                checkedCount > 0 ? (
                  <div className="flex justify-end p-2">
                    {/* The picks already ARE the subject: this only closes
                        the menu, back to the field. */}
                    <Button
                      type="button"
                      size="sm"
                      data-testid={COMPOSER_MENU_TEST_IDS.implementSubmit}
                      onClick={() => setOpen(false)}
                    >
                      {implementButtonLabel(checkedCount)}
                    </Button>
                  </div>
                ) : undefined
              }
            />
          ),
        }
      case `prompts`:
        if (!prompts) return null
        return {
          ...base,
          kind: `submenu`,
          disabled: rowsDisabled,
          contentClassName: SUBMENU_PICKER_CLASS,
          body: <PromptList prompts={prompts.prompts} onPick={pick(prompts.onPick)} />,
        }
      case `device`: {
        const binding = runner.device
        if (!binding) return null
        return {
          ...base,
          kind: `submenu`,
          value: binding.devices.find((device) => device.id === binding.value)?.name,
          disabled: rowsDisabled,
          contentClassName: SUBMENU_PICKER_CLASS,
          body: (
            <SubmenuSearchList
              items={devicePickerItems(binding.devices)}
              value={binding.value}
              onChange={pick(binding.onChange)}
            />
          ),
        }
      }
      case `account`: {
        const binding = runner.account
        if (!binding) return null
        const picked = binding.options.find((option) => option.key === binding.value)
        return {
          ...base,
          kind: `submenu`,
          value: picked?.email,
          disabled: rowsDisabled,
          contentClassName: SUBMENU_PICKER_CLASS,
          body: (
            <AccountList
              options={binding.options}
              value={binding.value}
              onChange={pick(binding.onChange)}
            />
          ),
        }
      }
      case `model`: {
        const binding = runner.model
        if (!binding) return null
        return {
          ...base,
          kind: `submenu`,
          value: labelOf(binding.models, binding.value),
          disabled: rowsDisabled,
          contentClassName: SUBMENU_PICKER_CLASS,
          body: (
            <SubmenuSearchList
              items={choicePickerItems(binding.models)}
              value={binding.value}
              onChange={pick(binding.onChange)}
            />
          ),
        }
      }
      case `repository`: {
        const binding = runner.repository
        if (!binding) return null
        return {
          ...base,
          kind: `submenu`,
          value: binding.repositories.find((repo) => repo.id === binding.value)?.fullName,
          disabled: rowsDisabled,
          contentClassName: SUBMENU_LIST_CLASS,
          body: (
            <SubmenuSearchList
              items={repositoryPickerItems(binding.repositories)}
              value={binding.value}
              onChange={pick(binding.onChange)}
            />
          ),
        }
      }
      case `effort`:
        if (!options) return null
        return {
          ...base,
          kind: `submenu`,
          value: labelOf(effortOptions, effortValue),
          // Ultracode runs at the agent's top effort: nothing to pick.
          disabled: ultracodeOn || rowsDisabled,
          body: (
            <PickerMenuRows
              mode="single"
              items={effortOptions}
              value={effortValue}
              onChange={options.setEffortValue}
            />
          ),
        }
      case `subagents`:
        return null
      case `ultracode`:
        if (!options) return null
        return {
          ...base,
          kind: `toggle`,
          checked: options.ultracode,
          disabled: rowsDisabled,
          onChange: options.setUltracode,
        }
      case `mcp-servers`:
        if (!options) return null
        return {
          ...base,
          kind: `submenu`,
          value:
            options.mcpServerIds.length > 0
              ? String(options.mcpServerIds.length)
              : undefined,
          disabled: rowsDisabled,
          contentClassName: `w-[18rem]`,
          body: (
            <McpServerMenuRows
              servers={options.mcpServers}
              selectedIds={options.mcpServerIds}
              onToggle={options.toggleMcpServer}
              connectHref={options.mcpConnectHref}
            />
          ),
        }
      case `computer-use`:
        if (!options) return null
        return {
          ...base,
          kind: `toggle`,
          checked: options.computerUse,
          disabled: rowsDisabled,
          onChange: options.setComputerUse,
        }
    }
  }

  const entries: MenuEntry[] = []
  for (const entry of layout) {
    if (entry.kind === `separator`) {
      // A row that bound to nothing must not strand a separator.
      if (entries.length > 0 && entries[entries.length - 1]?.kind !== `separator`) {
        entries.push({ kind: `separator` })
      }
      continue
    }
    const bound = entryFor(entry)
    if (bound) entries.push(bound)
  }
  while (entries[entries.length - 1]?.kind === `separator`) entries.pop()

  return (
    <Menu
      open={disabled ? false : open}
      onOpenChange={(next) => setOpen(disabled ? false : next)}
      entries={entries}
      aria-label={COMPOSER_PLUS_LABEL}
      data-testid={COMPOSER_MENU_TEST_IDS.menu}
      align="start"
      contentClassName="w-[17rem]"
      trigger={
        <ComposerTool
          aria-label={COMPOSER_PLUS_LABEL}
          title={COMPOSER_PLUS_LABEL}
          data-testid={COMPOSER_MENU_TEST_IDS.plus}
          disabled={disabled}
          className={highlighted ? `text-foreground` : undefined}
        >
          <UiAddIcon />
        </ComposerTool>
      }
    />
  )
}

/** THE search list as a single-pick submenu body (a runner picker's rows). */
function SubmenuSearchList({
  items,
  value,
  onChange,
}: {
  items: PickerItem[]
  value: string | null
  onChange: (value: string) => void
}) {
  return (
    <div className="flex max-h-[min(24rem,var(--radix-dropdown-menu-content-available-height,24rem))] min-h-0 flex-col">
      <SearchList
        mode="single"
        items={items}
        value={value}
        onChange={onChange}
        inputTestId={COMPOSER_MENU_TEST_IDS.submenuSearch}
      />
    </div>
  )
}

/** Account ›: THE account picker's rows (brand mark + email + the health
 *  hint) with its EXP-992 usage preview (#EXP-1434): a hover card beside the
 *  hovered row on a pointer, the bars inline under the email on touch. */
function AccountList({
  options,
  value,
  onChange,
}: {
  options: readonly AccountPickerOption[]
  value: string | null
  onChange: (key: string) => void
}) {
  const touch = useIsMobile()
  const byKey = useMemo(
    () => new Map(options.map((option) => [option.key, option])),
    [options]
  )
  const items = useMemo<PickerItem[]>(
    () =>
      options.map((option) => ({
        value: option.key,
        label: option.email,
        keywords: [option.email, option.agent],
        disabled: option.disabled,
      })),
    [options]
  )
  return (
    <div className="flex max-h-[min(24rem,var(--radix-dropdown-menu-content-available-height,24rem))] min-h-0 flex-col">
      <SearchList
        mode="single"
        items={items}
        value={value}
        onChange={onChange}
        inputTestId={COMPOSER_MENU_TEST_IDS.submenuSearch}
        // Touch: the bars sit inline under the email, a taller row.
        rowClassName={touch ? `h-auto min-h-11 py-1.5` : undefined}
        renderItem={(item) => {
          const option = byKey.get(item.value)
          return option ? (
            <AccountOptionRow option={option} touch={touch} />
          ) : null
        }}
      />
    </div>
  )
}

/** Prompts ›: THE search list over the team's prompts (the `/` menu's
 *  order and name filter); a row = the prompt's icon + its name. */
function PromptList({
  prompts,
  onPick,
}: {
  prompts: readonly SlashPrompt[]
  onPick: (prompt: SlashPrompt) => void
}) {
  const [query, setQuery] = useState(``)
  const visible = filterSlashPrompts(prompts, promptSlashName(query))
  const items: PickerItem[] = visible.map((prompt) => ({
    value: prompt.id,
    label: prompt.name,
    icon: getActionIcon(prompt),
  }))
  return (
    <div
      data-testid="agent-composer-prompts-picker"
      className="flex max-h-[min(24rem,var(--radix-dropdown-menu-content-available-height,24rem))] min-h-0 flex-col"
    >
      <SearchList
        mode="single"
        items={items}
        value={null}
        query={query}
        onQueryChange={setQuery}
        shouldFilter={false}
        onChange={(id) => {
          const prompt = prompts.find((candidate) => candidate.id === id)
          if (prompt) onPick(prompt)
        }}
        inputTestId={COMPOSER_MENU_TEST_IDS.submenuSearch}
      />
    </div>
  )
}

/** Issue › in a session or an agent chat: THE `#` search list over the
 *  team's issues; a pick inserts the reference. */
function InsertIssueList({ onPick }: { onPick: (identifier: string) => void }) {
  const issueRefs = useIssueRefs()
  const [query, setQuery] = useState(``)
  if (!issueRefs) return null
  return (
    <div
      data-testid="agent-composer-issues-picker"
      className="flex max-h-[min(24rem,var(--radix-dropdown-menu-content-available-height,24rem))] min-h-0 flex-col"
    >
      <IssuePickerBody<ResolvedIssueRef>
        mode="single"
        teamId={issueRefs.teamId}
        rows={issueRefs.rows}
        resolveHit={(hit) => issueRefs.fromHit(hit)}
        query={query}
        onQueryChange={setQuery}
        searchPlaceholder={ISSUE_SEARCH_PLACEHOLDER}
        emptyText={() => ISSUE_SEARCH_NO_RESULTS}
        inputTestId={COMPOSER_MENU_TEST_IDS.submenuSearch}
        onPick={(issue) => onPick(issue.identifier)}
      />
    </div>
  )
}

/** The launch composer's "+": the runner and work picks bound to the
 *  launch, the implement subject. With an agent picked on the agent row
 *  (`recipient`) the message runs on that agent: no runner, no work rows. */
export function ComposerPlusMenu({
  model,
  onAttach,
  disabled,
}: {
  model: LaunchComposerModel
  /** Opens the file chooser (the composer owns the hidden input). */
  onAttach: () => void
  disabled?: boolean
}) {
  const { launch, subject, candidateDevices } = model
  const temporary = model.recipient === null
  const runner = useMemo<ComposerRunnerBinding>(() => {
    if (!temporary || candidateDevices.length === 0) return {}
    return {
      // EXP-1030: the machines as THE device picker's rows (kind glyph +
      // name, EXP-432's owner suffix on a teammate's shared server).
      device: {
        value: launch.device?.deviceId ?? null,
        devices: candidateDevices.map((candidate) => ({
          id: candidate.deviceId,
          name: `${candidate.deviceLabel || candidate.deviceId}${
            candidate.owner ? ` — ${candidate.owner.name}` : ``
          }`,
          icon: candidate.icon,
          kind: candidate.kind,
        })),
        onChange: launch.setDeviceId,
      },
      // EXP-872: THE account picker's rows: brand mark + email, the agent
      // implied by the pick.
      account: {
        value: launch.accountKey ?? null,
        options: launch.accountOptions.map((option) => ({
          key: accountOptionKey(option),
          agent: option.agent,
          email: option.email,
          // EXP-849: health beats everything else in the row.
          hint: healthBadgeLabel(option.health) ?? undefined,
          limits: option.limits,
        })),
        onChange: launch.setAccountKey,
      },
      // Every model is explicit (owner 2026-10-15): THE contract list.
      model: {
        value: launch.model || null,
        models: modelChoices(launch.agent),
        onChange: launch.setModel,
      },
      // EXP-993: a choice only when there IS one; a picked issue anchors
      // the repository itself.
      ...(subject?.kind !== `issues`
        ? {
            repository: {
              value: model.repoId || null,
              repositories: model.repoOptions.map((option) => ({
                id: option.value,
                fullName: option.label,
              })),
              onChange: model.setRepoId,
            },
          }
        : {}),
    }
  }, [temporary, candidateDevices, launch, subject?.kind, model.repoId, model.repoOptions, model.setRepoId])

  const issues: ComposerIssueBinding = {
    kind: `implement`,
    teamId: model.teamId,
    eligibleIssues: model.eligibleIssues,
    checkedIssues: model.checkedIssues,
    checkedCount: subject?.kind === `issues` ? subject.ids.length : 0,
    toggleIssue: model.toggleIssue,
  }

  return (
    <ComposerOptionsMenu
      surface="launch"
      agent={launch.agent}
      temporary={temporary}
      options={{
        agent: launch.agent,
        effortValue: launch.effortValue,
        setEffortValue: launch.setEffortValue,
        ultracode: launch.ultracode,
        setUltracode: launch.setUltracode,
        computerUse: launch.computerUse,
        setComputerUse: launch.setComputerUse,
        computerUseAvailable: launch.computerUseAvailable,
        mcpServers: model.mcpServers ?? [],
        mcpServerIds: launch.mcpServerIds,
        toggleMcpServer: launch.toggleMcpServer,
        mcpConnectHref: model.mcpConnectHref,
      }}
      runner={runner}
      issues={issues}
      // The `/` menu's pick: the prompt becomes the new session's subject.
      prompts={{ prompts: model.prompts, onPick: (prompt) => model.pickPrompt(prompt.id) }}
      onAttach={onAttach}
      disabled={disabled}
      highlighted={subject !== null || model.images.length > 0}
    />
  )
}
