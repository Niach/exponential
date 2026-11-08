import { useMemo, useState } from "react"
import { useLaunchOptions } from "@/components/launch-dialog/use-launch-options"
import { accountPickerOptions } from "@/components/launch-dialog/composer-plus-menu"
import { modelChoices } from "@/components/launch-dialog/launch-options-pane"
import type { EditorHeldRunner } from "@/components/session-composer"
import { accountOptionKey, type AccountOption } from "@/lib/accounts/account-option"
import { defaultModelFor } from "@/lib/coding-launch-prefs"
import type { SteerDevice } from "@/lib/steer-devices"

// #EXP-1729 (apps.json `editorHeader`, composer-menu.json `account`/`model`:
// an app editor's composer, always): the editor composer's Account › and
// Model › while its root is not live. NO ROOT: THE launch composer's own
// state (`useLaunchOptions` over the editor device: its last used login
// names the agent, the device's default model), and the next send carries
// `{cli, account, model}` (`messageApp` → `runnerFields`). An ENDED root
// revives on its own agent (`reviveRoot` keeps `cli`): its agent's logins
// and models, reading its own account/model until a pick, and only a pick
// rides the send (`reviveRoot`: `account`, `runnerPicks`). A LIVE root is
// the session composer's own switch (not here). Picks are held while the
// editor is open, never remembered.

export interface EditorRunnerRoot {
  id: string
  status: string
  cli: string | null
  account: string | null
  model: string | null
}

/** An ENDED root's held rows: its agent's logins (keys), the value = the
 *  pick, else the root's own; the send = the picks alone. */
export function endedRootRunner(
  root: Pick<EditorRunnerRoot, `cli` | `account` | `model`>,
  options: readonly AccountOption[],
  picked: { account?: string; model?: string }
): { agent: string; options: AccountOption[]; accountKey: string | null; model: string; send: EditorHeldRunner[`send`] } {
  const agent = root.cli ?? `claude`
  const own = options.filter((option) => option.agent === agent)
  const pickedOption = picked.account ? own.find((option) => accountOptionKey(option) === picked.account) : undefined
  const rootOption = root.account ? own.find((option) => option.id === root.account) : undefined
  const accountKey = pickedOption ? accountOptionKey(pickedOption) : rootOption ? accountOptionKey(rootOption) : null
  const model = picked.model ?? root.model ?? defaultModelFor(agent)
  return {
    agent,
    options: own,
    accountKey,
    model,
    send: {
      // An unpinned option (id ``) names no login: the root keeps its own.
      ...(pickedOption?.id ? { account: pickedOption.id } : {}),
      ...(picked.model ? { model: picked.model } : {}),
    },
  }
}

/** The held Account › / Model › for the editor composer (undefined while
 *  the root is live: the composer's own session switch applies). */
export function useEditorRunner(
  root: EditorRunnerRoot | null,
  device: SteerDevice | undefined
): EditorHeldRunner | undefined {
  const devices = useMemo(() => (device ? [device] : []), [device])
  const launch = useLaunchOptions({ open: true, devices, initialDeviceId: device?.deviceId })
  const [picked, setPicked] = useState<{ rootId: string; account?: string; model?: string } | null>(null)
  const live = root !== null && root.status !== `ended`
  if (live) return undefined
  if (root === null) {
    const options = launch.buildOptions()
    return {
      account: {
        value: launch.accountKey ?? null,
        options: accountPickerOptions(launch.accountOptions),
        onChange: launch.setAccountKey,
      },
      model: { value: launch.model || null, models: modelChoices(launch.agent), onChange: launch.setModel },
      send: {
        cli: options.agent,
        ...(options.model ? { model: options.model } : {}),
        ...(options.account ? { account: options.account } : {}),
      },
    }
  }
  const held = picked?.rootId === root.id ? picked : { rootId: root.id }
  const ended = endedRootRunner(root, launch.accountOptions, held)
  return {
    ...(ended.options.length > 0
      ? {
          account: {
            value: ended.accountKey,
            options: accountPickerOptions(ended.options),
            onChange: (key: string) => setPicked({ ...held, account: key }),
          },
        }
      : {}),
    model: {
      value: ended.model,
      models: modelChoices(ended.agent),
      onChange: (model: string) => setPicked({ ...held, model }),
    },
    send: ended.send,
  }
}
