import { useNavigate } from "@tanstack/react-router"
import { chatRunMarkState, GlassSectionHeader, SessionRow, SETTINGS_LIST_CLASS } from "@exp/ui"
import type { ChatsListGroup, ChatsListRow } from "@/lib/session-tree"
import type { Team } from "@/db/schema"
import { useChatsList } from "@/hooks/use-session-tree"
import { sessionCollection } from "@/lib/collections"
import { Empty } from "@/components/empty"

// #EXP-2027 (sidebar-tree.json DEVICES): the device page's Running list: the
// live chats (Needs you, then Running, `partitionChats`) of every team of the
// viewer whose root runs on this machine, each row THE session row with its
// team's mark and name trailing; a row opens the session in its team.

export const DEVICE_RUNNING_LABEL = `Running`

/** The roots of the live bands (needs you, running) on `deviceId` (pure). */
export function deviceRunningRows(groups: readonly ChatsListGroup[], deviceId: string): ChatsListRow[] {
  return groups
    .filter((group) => group.key === `needs_you` || group.key === `running`)
    .flatMap((group) => group.rows)
    .filter((row) => row.depth === 0 && row.deviceId === deviceId)
}

export function DeviceRunningSection({ deviceId, teams }: { deviceId: string; teams: readonly Team[] }) {
  const navigate = useNavigate()
  const { groups } = useChatsList(teams.map((team) => team.id))
  const rows = deviceRunningRows(groups, deviceId)
  const teamById = new Map(teams.map((team) => [team.id, team]))
  return (
    <section data-testid="device-running">
      <GlassSectionHeader label={DEVICE_RUNNING_LABEL} count={rows.length} />
      {rows.length === 0 ? (
        <Empty id="running-agents" />
      ) : (
        <div className={`${SETTINGS_LIST_CLASS} pt-1`}>
          {rows.map((row) => {
            const team = teamById.get(row.teamId)
            return (
              <SessionRow
                key={row.sessionId}
                agent={sessionCollection.get(row.sessionId)?.cli ?? null}
                markState={chatRunMarkState(row)}
                title={row.title}
                unread={row.unread}
                team={team ? { iconUrl: team.iconUrl, name: team.name } : null}
                data-testid={`device-running-${row.sessionId}`}
                trailing={
                  team && (
                    <span className="ml-auto flex shrink-0 items-center gap-1.5 pl-3 text-muted-foreground">
                      <span className="truncate">{team.name}</span>
                    </span>
                  )
                }
                onClick={() => {
                  if (!team) return
                  void navigate({
                    to: `/t/$teamSlug/sessions/$sessionId`,
                    params: { teamSlug: team.slug, sessionId: row.sessionId },
                  })
                }}
              />
            )
          })}
        </div>
      )}
    </section>
  )
}
