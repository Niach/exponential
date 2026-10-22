import { useState } from "react"
import type { IconConcept } from "@exp/icons"
import {
  Button,
  ComposerTool,
  Menu,
  PickerMenuRows,
  conceptIcon,
  type MenuEntry,
  type ValueChoice,
} from "@exp/ui"
import type { LaunchComposerModel } from "@/hooks/use-launch-composer"
import type { McpServerList } from "@/lib/mcp-servers"
import {
  agentEffortValues,
  agentSupportsUltracode,
} from "@/lib/coding-launch-prefs"
import { ComposerIssuePicker } from "@/components/launch-dialog/issue-picker"
import { AgentIcon } from "@/components/team/sidebar-agents"
import {
  CLI_DEFAULT_EFFORT,
  effortLabel,
} from "@/components/launch-dialog/launch-options-pane"
import { McpServerMenuRows } from "@/components/launch-dialog/mcp-server-picker"
import {
  COMPOSER_MENU_TEST_IDS,
  COMPOSER_PLUS_LABEL,
  composerMenuLayout,
  composerMenuRowTestId,
  implementButtonLabel,
  type ComposerMenuRow,
} from "@/components/launch-dialog/composer-menu"

// EXP-1249: the composer's ONE "+" — the subject (Implement issue ›: THE
// search list since SLOP-28; NEW-2: prompts live in the `/` menu), the attach, the session's
// options (Effort ›, Ultracode; Subagents › hides, `sessions.send` has no
// field for it) and its tools (MCP servers ›, Computer use), as ONE
// `Menu`: a dropdown at the "+" from md up, the bottom sheet of the same rows
// on a phone (submenus as pushed pages). Which rows show, their order, glyphs
// and copy are `composer-menu.ts` (fixture-locked ×4); this file binds each
// row to the composer model.

const UiAddIcon = conceptIcon(`ui-add`)

const CLI_DEFAULT_LABEL = `CLI default`

/** SLOP-28: Implement issue › is the search
 *  list (`composer-menu.json` `submenuRowHeight` × `submenuMaxRows`), at one
 *  width. On a pointer they open to the side on hover or →, ← and Escape
 *  close back (`Menu`'s submenu body); on a phone they push as pages. */
const SUBMENU_LIST_CLASS = `w-[26rem] p-0`

/** What the "+" options rows read and write: a launch's picks before a
 *  start (`LaunchOptions`), a live session's synced options inside one
 *  (`session-composer-options.tsx`). */
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

/** The launch-only half: the subject rows (Implement issue ›, NEW-3
 *  Message an agent ›) exist only BEFORE a start, never inside a session. */
export interface ComposerSubjectRows {
  teamId: string
  eligibleIssues: LaunchComposerModel[`eligibleIssues`]
  checkedIssues: LaunchComposerModel[`checkedIssues`]
  /** The picked issue ids' count (the subject's, ahead of the rows). */
  checkedCount: number
  toggleIssue: (issueId: string) => void
  /** NEW-3: the team's agents (none = no Message an agent › row). */
  agents: LaunchComposerModel[`agents`]
  /** The subject agent's id, when the subject is an agent. */
  pickedAgentId: string | null
  pickAgent: (agentId: string) => void
}

/** THE "+" = the composer's options menu (EXP-1334: the same rows before a
 *  start and during a session, minus the subject rows in a session). */
export function ComposerOptionsMenu({
  options,
  subject,
  onAttach,
  disabled,
  rowsDisabled,
  highlighted,
}: {
  options: ComposerOptionsBinding
  /** Before a start only. */
  subject?: ComposerSubjectRows
  /** Opens the file chooser (the composer owns the hidden input). */
  onAttach: () => void
  disabled?: boolean
  /** The options rows read but cannot change (an ended session). */
  rowsDisabled?: boolean
  /** The "+" reads as active (a subject or a pending file is up). */
  highlighted?: boolean
}) {
  const [open, setOpen] = useState(false)
  const { agent } = options
  const ultracodeOn = options.ultracode && agentSupportsUltracode(agent)
  const checkedCount = subject?.checkedCount ?? 0
  const showMcp = options.mcpServers.length > 0

  const effortOptions: ValueChoice[] = [
    { value: CLI_DEFAULT_EFFORT, label: CLI_DEFAULT_LABEL },
    ...agentEffortValues(agent).map((value) => ({
      value,
      label: effortLabel(value),
    })),
  ]
  const effortValue =
    options.effortValue === `` ? CLI_DEFAULT_EFFORT : options.effortValue
  const labelOf = (choices: ValueChoice[], value: string) =>
    choices.find((choice) => choice.value === value)?.label

  const layout = composerMenuLayout({
    // EXP-1293: `sessions.send` carries no subagent model; the row hides.
    subagentModel: false,
    ultracode: agentSupportsUltracode(agent),
    mcp: showMcp,
    computerUse: options.computerUseAvailable,
    // NEW-3: Message an agent › while the team has agents (before a start).
    agents: (subject?.agents?.length ?? 0) > 0,
  }).filter(
    (row) =>
      subject !== undefined ||
      row.kind === `separator` ||
      (row.id !== `implement-issue` && row.id !== `message-agent`)
  )

  const entryFor = (row: ComposerMenuRow): MenuEntry | null => {
    const base = {
      id: row.id,
      label: agent === `codex` && row.codexLabel ? row.codexLabel : row.label,
      icon: conceptIcon(row.icon as IconConcept),
      "data-testid": composerMenuRowTestId(row.id),
    }
    switch (row.id) {
      case `implement-issue`:
        if (!subject) return null
        return {
          ...base,
          kind: `submenu`,
          value: checkedCount > 0 ? String(checkedCount) : undefined,
          contentClassName: SUBMENU_LIST_CLASS,
          body: (
            <ComposerIssuePicker
              teamId={subject.teamId}
              eligible={subject.eligibleIssues}
              checked={subject.checkedIssues}
              onToggle={subject.toggleIssue}
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
      case `message-agent`: {
        if (!subject) return null
        const agents = subject.agents ?? []
        const picked = agents.find((row) => row.agentId === subject.pickedAgentId) ?? null
        return {
          ...base,
          kind: `submenu`,
          value: picked?.name,
          contentClassName: `w-[18rem]`,
          body: (
            <PickerMenuRows
              mode="single"
              items={agents.map((row) => ({ value: row.agentId, label: row.name }))}
              value={subject.pickedAgentId}
              onChange={(agentId) => {
                subject.pickAgent(agentId)
                setOpen(false)
              }}
              renderItem={(item) => {
                const row = agents.find((candidate) => candidate.agentId === item.value)
                return (
                  <span className="flex min-w-0 items-center gap-2">
                    {row && <AgentIcon agent={row} />}
                    <span className="min-w-0 truncate">{item.label}</span>
                  </span>
                )
              }}
            />
          ),
        }
      }
      case `add-file`:
        return { ...base, kind: `item`, onSelect: onAttach }
      case `effort`:
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
        return {
          ...base,
          kind: `toggle`,
          checked: options.ultracode,
          disabled: rowsDisabled,
          onChange: options.setUltracode,
        }
      case `mcp-servers`:
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
      entries.push({ kind: `separator` })
      continue
    }
    const bound = entryFor(entry)
    if (bound) entries.push(bound)
  }

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

/** The launch composer's "+": the options bound to the launch picks, plus
 *  the subject rows. */
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
  const { launch, subject } = model
  return (
    <ComposerOptionsMenu
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
      subject={{
        teamId: model.teamId,
        eligibleIssues: model.eligibleIssues,
        checkedIssues: model.checkedIssues,
        checkedCount: subject?.kind === `issues` ? subject.ids.length : 0,
        toggleIssue: model.toggleIssue,
        agents: model.agents,
        pickedAgentId: subject?.kind === `agent` ? subject.id : null,
        pickAgent: model.pickAgent,
      }}
      onAttach={onAttach}
      disabled={disabled}
      highlighted={subject !== null || model.images.length > 0}
    />
  )
}
