import { useMemo, useState } from "react"
import { useLiveQuery } from "@tanstack/react-db"
import { GlassGroup, GlassSectionHeader, GlassToggleRow } from "@exp/ui"
import {
  settingsRows,
  sourceAllowed,
  toggleSourcePref,
  type SourcePrefs,
} from "@exp/domain-contract/notification-delivery"
import { agentCollection, teamCollection } from "@/lib/collections"
import { trpc } from "@/lib/trpc-client"
import { failedAction } from "@/lib/action-error"

// #EXP-1815 (notification-delivery.json SETTINGS): the session-source
// switches above the email card: Sessions (Personal Assistant, Temporary
// sessions), then Team agents (one per agent of the member's teams). The
// server applies them before push, desktop and email.
export function NotificationSourcesCard({ initial }: { initial: SourcePrefs | undefined }) {
  const [prefs, setPrefs] = useState<SourcePrefs>(initial ?? {})
  const { data: teams } = useLiveQuery((query) => query.from({ t: teamCollection }), [])
  const { data: agents } = useLiveQuery((query) => query.from({ a: agentCollection }), [])
  const groups = useMemo(
    () =>
      settingsRows(
        (teams ?? []).map((team) => ({ id: team.id, name: team.name })),
        (agents ?? []).map((agent) => ({ id: agent.id, teamId: agent.teamId, name: agent.name, role: String(agent.role ?? `agent`) }))
      ),
    [teams, agents]
  )

  const toggle = (key: string, on: boolean) => {
    const before = prefs
    const next = toggleSourcePref(prefs, key, on)
    setPrefs(next)
    trpc.notifications.updateEmailPrefs.mutate({ sourcePrefs: next }).catch((error: unknown) => {
      setPrefs(before)
      failedAction(`update`, `notification settings`, error, { retry: () => toggle(key, on) })
    })
  }

  return (
    <div className="space-y-3">
      {groups.map((group) => (
        <div key={group.band} className="space-y-1">
          <GlassSectionHeader label={group.band} />
          <GlassGroup>
            {group.rows.map((row) => (
              <GlassToggleRow
                key={row.key}
                id={`source-${row.key}`}
                label={row.label}
                description={row.detail ?? undefined}
                checked={sourceAllowed(prefs, row.key)}
                onCheckedChange={(next) => toggle(row.key, next)}
              />
            ))}
          </GlassGroup>
        </div>
      ))}
    </div>
  )
}
