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
import { contract } from "@exp/domain-contract"
import type { LaunchComposerModel } from "@/hooks/use-launch-composer"
import {
  agentEffortValues,
  agentSupportsSubagentModel,
  agentSupportsUltracode,
} from "@/lib/coding-launch-prefs"
import { subjectOwnsMcpServers } from "@/lib/mcp-servers"
import { ActionPickerList } from "@/components/launch-dialog/action-picker"
import { ComposerIssuePicker } from "@/components/launch-dialog/issue-picker"
import {
  CLI_DEFAULT_EFFORT,
  CLI_DEFAULT_MODEL,
  effortLabel,
  modelLabel,
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

// EXP-1249: the composer's ONE "+" — the subject (Implement issue ›, Run
// action ›), the attach, the run's options (Effort ›, Subagents ›,
// Ultracode) and the run's tools (MCP servers ›, Computer use), as ONE
// `Menu`: a dropdown at the "+" from md up, the bottom sheet of the same rows
// on a phone (submenus as pushed pages). Which rows show, their order, glyphs
// and copy are `composer-menu.ts` (fixture-locked ×4); this file binds each
// row to the composer model.

const UiAddIcon = conceptIcon(`ui-add`)

const CLI_DEFAULT_LABEL = `CLI default`

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
  const [open, setOpen] = useState(false)
  const { launch, subject } = model
  const { agent } = launch
  const ultracodeOn = launch.ultracode && agentSupportsUltracode(agent)
  const checkedCount = subject?.kind === `issues` ? subject.ids.length : 0
  const selectedActionId = subject?.kind === `action` ? subject.id : null
  const mcpServers = model.mcpServers ?? []
  const showMcp = mcpServers.length > 0 && !subjectOwnsMcpServers(subject)

  const effortOptions: ValueChoice[] = [
    { value: CLI_DEFAULT_EFFORT, label: CLI_DEFAULT_LABEL },
    ...agentEffortValues(agent).map((value) => ({
      value,
      label: effortLabel(value),
    })),
  ]
  const effortValue =
    launch.effortValue === `` ? CLI_DEFAULT_EFFORT : launch.effortValue
  const subagentOptions: ValueChoice[] = [
    { value: CLI_DEFAULT_MODEL, label: `Default` },
    ...contract.codingModel.values.map((value) => ({
      value,
      label: modelLabel(value),
    })),
  ]
  const subagentValue =
    launch.subagentModel === `` ? CLI_DEFAULT_MODEL : launch.subagentModel
  const labelOf = (options: ValueChoice[], value: string) =>
    options.find((option) => option.value === value)?.label

  const layout = composerMenuLayout({
    subagentModel: agentSupportsSubagentModel(agent),
    ultracode: agentSupportsUltracode(agent),
    mcp: showMcp,
    computerUse: launch.computerUseAvailable,
  })

  const entryFor = (row: ComposerMenuRow): MenuEntry => {
    const base = {
      id: row.id,
      label: agent === `codex` && row.codexLabel ? row.codexLabel : row.label,
      icon: conceptIcon(row.icon as IconConcept),
      "data-testid": composerMenuRowTestId(row.id),
    }
    switch (row.id) {
      case `implement-issue`:
        return {
          ...base,
          kind: `submenu`,
          value: checkedCount > 0 ? String(checkedCount) : undefined,
          contentClassName: `w-[26rem] p-0`,
          body: (
            <div className="flex min-h-0 flex-col">
              <ComposerIssuePicker
                teamId={model.teamId}
                eligible={model.eligibleIssues}
                checked={model.checkedIssues}
                onToggle={model.toggleIssue}
              />
              {checkedCount > 0 && (
                <div className="flex justify-end border-t border-glass-stroke p-2">
                  {/* The picks already ARE the subject: this only closes the
                      menu, back to the field. */}
                  <Button
                    type="button"
                    size="sm"
                    data-testid={COMPOSER_MENU_TEST_IDS.implementSubmit}
                    onClick={() => setOpen(false)}
                  >
                    {implementButtonLabel(checkedCount)}
                  </Button>
                </div>
              )}
            </div>
          ),
        }
      case `run-action`:
        return {
          ...base,
          kind: `submenu`,
          contentClassName: `w-[24rem] p-0`,
          body: (
            <div
              data-testid="agent-composer-actions-picker"
              className="flex max-h-[min(24rem,var(--radix-dropdown-menu-content-available-height,24rem))] min-h-0 flex-col"
            >
              <ActionPickerList
                actions={model.actions}
                selectedActionId={selectedActionId}
                onSelect={(actionId) => {
                  model.pickAction(actionId)
                  setOpen(false)
                }}
              />
            </div>
          ),
        }
      case `add-file`:
        return { ...base, kind: `item`, onSelect: onAttach }
      case `effort`:
        return {
          ...base,
          kind: `submenu`,
          value: labelOf(effortOptions, effortValue),
          // Ultracode runs at the agent's top effort: nothing to pick.
          disabled: ultracodeOn,
          body: (
            <PickerMenuRows
              mode="single"
              items={effortOptions}
              value={effortValue}
              onChange={launch.setEffortValue}
            />
          ),
        }
      case `subagents`:
        return {
          ...base,
          kind: `submenu`,
          value: labelOf(subagentOptions, subagentValue),
          body: (
            <PickerMenuRows
              mode="single"
              items={subagentOptions}
              value={subagentValue}
              onChange={(value) =>
                launch.setSubagentModel(value === CLI_DEFAULT_MODEL ? `` : value)
              }
            />
          ),
        }
      case `ultracode`:
        return {
          ...base,
          kind: `toggle`,
          checked: launch.ultracode,
          onChange: launch.setUltracode,
        }
      case `mcp-servers`:
        return {
          ...base,
          kind: `submenu`,
          value:
            launch.mcpServerIds.length > 0
              ? String(launch.mcpServerIds.length)
              : undefined,
          contentClassName: `w-[18rem]`,
          body: (
            <McpServerMenuRows
              servers={mcpServers}
              selectedIds={launch.mcpServerIds}
              onToggle={launch.toggleMcpServer}
              connectHref={model.mcpConnectHref}
            />
          ),
        }
      case `computer-use`:
        return {
          ...base,
          kind: `toggle`,
          checked: launch.computerUse,
          onChange: launch.setComputerUse,
        }
    }
  }

  const entries: MenuEntry[] = layout.map((entry) =>
    entry.kind === `separator` ? { kind: `separator` } : entryFor(entry)
  )

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
          className={
            subject !== null || model.images.length > 0
              ? `text-foreground`
              : undefined
          }
        >
          <UiAddIcon />
        </ComposerTool>
      }
    />
  )
}
