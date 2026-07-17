import { createContext, useContext, useEffect, useState } from "react"

// The global agent-coding dock (EXP-106): one expanded live viewer at a time,
// IDE-style, mounted in the workspace layout. Issue detail and the Agents page
// only ever FOCUS the dock — the live AgentSessionView (and its single relay
// socket) lives here alone. The single expanded id guarantees at most one
// viewer socket; consumers remount the panel via `key={expandedSessionId}`.

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
  workspaceId,
  children,
}: {
  workspaceId: string
  children: React.ReactNode
}) {
  const [expandedSessionId, setExpandedSessionId] = useState<string | null>(null)

  // The dock survives $workspaceSlug PARAM changes (same layout instance), but
  // a real workspace switch must collapse any expanded viewer — that session
  // belongs to the previous workspace.
  useEffect(() => {
    setExpandedSessionId(null)
  }, [workspaceId])

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
