import { createContext, useContext } from "react"
import { authClient } from "@/lib/auth/client"

type SessionResult = ReturnType<typeof authClient.useSession>

const SessionContext = createContext<SessionResult | null>(null)

export function SessionProvider({ children }: { children: React.ReactNode }) {
  const session = authClient.useSession()
  return (
    <SessionContext.Provider value={session}>
      {children}
    </SessionContext.Provider>
  )
}

export function useSession(): SessionResult {
  const ctx = useContext(SessionContext)
  if (ctx) return ctx
  // eslint-disable-next-line react-hooks/rules-of-hooks
  return authClient.useSession()
}
