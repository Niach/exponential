import { createContext, useContext, useEffect, useState } from "react"

// The global agent-coding dock (EXP-106): one expanded live viewer at a time,
// IDE-style, mounted in the team layout. Issue detail and the Agents page
// only ever FOCUS the dock — the live AgentSessionView lives here alone;
// consumers remount the panel via `key={expandedSessionId}`. Since EXP-621
// the relay sockets themselves live OUTSIDE the view in the per-session
// stores of lib/steer-session-store.ts (the relay allows multiple viewers),
// so collapsing or switching tabs detaches the view, not the connection —
// the dock's reaper in agent-dock.tsx bounds how many stay alive.

interface AgentDockValue {
  expandedSessionId: string | null
  openDock: (sessionId: string) => void
  collapseDock: () => void
}

const AgentDockContext = createContext<AgentDockValue | null>(null)

export function useAgentDock(): AgentDockValue | null {
  return useContext(AgentDockContext)
}

export function AgentDockProvider({
  teamId,
  children,
}: {
  teamId: string
  children: React.ReactNode
}) {
  const [expandedSessionId, setExpandedSessionId] = useState<string | null>(null)

  // The dock survives $teamSlug PARAM changes (same layout instance), but
  // a real team switch must collapse any expanded viewer — that session
  // belongs to the previous team.
  useEffect(() => {
    setExpandedSessionId(null)
  }, [teamId])

  const value: AgentDockValue = {
    expandedSessionId,
    openDock: (sessionId) => setExpandedSessionId(sessionId),
    collapseDock: () => setExpandedSessionId(null),
  }

  return (
    <AgentDockContext.Provider value={value}>
      {children}
    </AgentDockContext.Provider>
  )
}
