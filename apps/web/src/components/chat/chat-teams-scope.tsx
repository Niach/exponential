import type { ReactNode } from "react"
import { ChatTeamsIssueRefProvider, useIssueRefs } from "@/components/issue-ref-provider"
import { ChatTeamsMentionProvider } from "@/components/mention-provider"
import type { ChatTeams } from "@/hooks/use-chat-teams"

// #EXP-2079 (agent-settings.json `assistant.chatTeams`): everything under a
// chat reads the CHAT TEAMS: `#` refs (the composer's search, chips, pills,
// Link an issue) and `@` members (the composer's list, mention pills). A
// chat of the team the page already resolves in (every session of a synced
// team on its own team's page) is left exactly as it was; the Assistant's
// conversation, whose personal team never syncs, gets its turn's teams.

/** The scope's identity on the refs context: nested scopes of one chat
 *  (the screen, its session view, its composer) mount the providers once. */
export function chatTeamsScopeKey(teamIds: readonly string[]): string {
  return teamIds.join(`,`)
}

export function ChatTeamsScope({ chatTeams, children }: { chatTeams: ChatTeams; children: ReactNode }) {
  const parent = useIssueRefs()
  const key = chatTeamsScopeKey(chatTeams.teamIds)
  const { teams } = chatTeams
  // Nothing synced to read from: the enclosing providers stand.
  if (teams.length === 0) return <>{children}</>
  if (parent?.scopeKey === key) return <>{children}</>
  if (!parent?.scopeKey && teams.length === 1 && parent?.teamId === teams[0]!.id) return <>{children}</>
  return (
    <ChatTeamsIssueRefProvider teams={teams} scopeKey={key}>
      <ChatTeamsMentionProvider teamIds={chatTeams.teamIds}>{children}</ChatTeamsMentionProvider>
    </ChatTeamsIssueRefProvider>
  )
}
