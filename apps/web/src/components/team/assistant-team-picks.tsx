import { useEffect, useMemo, useState } from "react"
import { useLiveQuery } from "@tanstack/react-db"
import { GlassGroup, GlassSectionHeader, Picker, PickerTrigger } from "@exp/ui"
import { assistantPickAgent } from "@exp/domain-contract/agent-settings"
import { agentSkillsValue, SKILLS } from "@exp/domain-contract/skills"
import { teamCollection } from "@/lib/collections"
import { trpc } from "@/lib/trpc-client"
import { failedAction, SKIP_ERROR_TOAST } from "@/lib/action-error"
import { useSession } from "@/hooks/use-session"
import { useSkills } from "@/hooks/use-skills"
import { useMcpServers } from "@/hooks/use-mcp-servers"
import { useTeamMemberships } from "@/hooks/use-team-data"
import { useAllAgentRows } from "@/hooks/use-agent-row"
import { skillPickerItems } from "@/components/launch-dialog/skill-picker"
import { McpServerPicker } from "@/components/launch-dialog/mcp-server-picker"
import type { SyncedAgent } from "@/components/team/agents-section"

// #EXP-1988 (agent-settings.json `assistant.teamPicks`, `page.sections`
// `team-picks`): Settings › Assistant draws ONE section per team of its
// member (by name), header = the team's name, card = THE
// skills row and THE mcp-servers row over that team's rows. A section edits
// `assistantPickAgent`'s agent (the ONE; old data still in the store: that team's own Assistant) and
// writes only that team's picks (`agents.update {pickTeamId}`).
export function AssistantTeamPicks({ pageAgent }: { pageAgent: SyncedAgent }) {
  const { data: auth } = useSession()
  const me = auth?.user?.id ?? null
  const { myTeams } = useTeamMemberships(me ?? undefined)
  const agents = useAllAgentRows()
  const { data: teamRows } = useLiveQuery((query) => query.from({ teams: teamCollection }))
  const synced = useMemo(() => (teamRows ?? []).map((team) => team.id), [teamRows])
  // By name, case-insensitive, then id (`teamPicks`: every platform's order).
  const ordered = useMemo(
    () =>
      [...myTeams].sort((a, b) => {
        const an = a.name.toLowerCase()
        const bn = b.name.toLowerCase()
        return an !== bn ? (an < bn ? -1 : 1) : a.id < b.id ? -1 : a.id > b.id ? 1 : 0
      }),
    [myTeams]
  )
  return (
    <>
      {ordered.map((team) => {
        const agentId = assistantPickAgent(agents, team.id, synced, me, pageAgent.id)
        const agent = agents.find((row) => row.id === agentId) ?? pageAgent
        return <TeamPicksSection key={team.id} team={team} agent={agent} />
      })}
    </>
  )
}

function TeamPicksSection({
  team,
  agent,
}: {
  team: { id: string; name: string; slug: string }
  agent: SyncedAgent
}) {
  const { skills } = useSkills(team.id)
  const { servers } = useMcpServers(team.id)
  const [skillIds, setSkillIds] = useState<string[]>(agent.skillIds ?? [])
  const [mcpServerIds, setMcpServerIds] = useState<string[]>(agent.mcpServerIds ?? [])
  // The synced row wins as it lands (another platform's edit too).
  useEffect(() => setSkillIds(agent.skillIds ?? []), [agent.id, agent.skillIds])
  useEffect(() => setMcpServerIds(agent.mcpServerIds ?? []), [agent.id, agent.mcpServerIds])

  const save = (patch: { skillIds: string[] } | { mcpServerIds: string[] }, revert: () => void) =>
    void trpc.agents.update
      .mutate({ id: agent.id, pickTeamId: team.id, ...patch }, SKIP_ERROR_TOAST)
      .catch((err: unknown) => {
        revert()
        failedAction(`save`, `the agent`, err, { retry: () => save(patch, revert) })
      })

  // The team's picks, in the team list's order (skills.json `agentValueCases`).
  const pickedSkills = skills.filter((skill) => skillIds.includes(skill.id))
  const pickedSkillIds = pickedSkills.map((skill) => skill.id)
  const pickSkills = (next: readonly string[]) => {
    const previous = skillIds
    const others = skillIds.filter((id) => !skills.some((skill) => skill.id === id))
    setSkillIds([...others, ...next])
    save({ skillIds: [...next] }, () => setSkillIds(previous))
  }

  const teamServers = servers ?? []
  const pickedServerIds = teamServers.filter((server) => mcpServerIds.includes(server.id)).map((server) => server.id)
  const toggleServer = (id: string) => {
    const previous = mcpServerIds
    const next = pickedServerIds.includes(id)
      ? pickedServerIds.filter((current) => current !== id)
      : [...pickedServerIds, id]
    const others = mcpServerIds.filter((current) => !teamServers.some((server) => server.id === current))
    setMcpServerIds([...others, ...next])
    save({ mcpServerIds: next }, () => setMcpServerIds(previous))
  }

  return (
    <div className="space-y-2" data-testid={`assistant-team-picks-${team.id}`}>
      <GlassSectionHeader label={team.name} />
      <GlassGroup>
        <Picker
          mode="multi"
          width="md"
          search
          mobileTitle={SKILLS.agentRow.label}
          value={pickedSkillIds}
          items={skillPickerItems(skills)}
          onChange={pickSkills}
          emptyText={SKILLS.list.empty.title}
          trigger={
            <PickerTrigger
              variant="row"
              label={SKILLS.agentRow.label}
              value={pickedSkills.length > 0 ? agentSkillsValue(pickedSkills.map((skill) => skill.name)) : undefined}
              placeholder={SKILLS.agentRow.none}
              className={pickedSkills.length === 0 ? `[&>span:nth-child(2)]:text-foreground/50` : undefined}
            />
          }
          data-testid="agent-skills"
        />
        {servers !== null && (servers.length > 0 || pickedServerIds.length > 0) ? (
          <McpServerPicker
            servers={teamServers}
            selectedIds={pickedServerIds}
            onToggle={toggleServer}
            connectHref={(serverId) =>
              `/t/${encodeURIComponent(team.slug)}/settings/mcp?connect=${encodeURIComponent(serverId)}`
            }
            testId="agent-mcp-servers"
          />
        ) : null}
      </GlassGroup>
    </div>
  )
}
