// Execution-time context for the WebMCP tool handlers (EXP-245). Registered
// tools are long-lived — instead of re-registering on every route change, the
// WebMcpProvider keeps this module-level store fresh and handlers read it at
// call time, so a tool invoked minutes after registration still sees the
// team/board/issue currently on screen.

export type WebMcpNavigateTarget =
  | { kind: `board`; boardSlug: string }
  | { kind: `issue`; boardSlug: string; issueIdentifier: string }
  | { kind: `inbox` }
  | { kind: `reviews` }

export interface WebMcpAppContext {
  teamId: string
  teamSlug: string
  teamName: string
  boardSlug: string | null
  issueIdentifier: string | null
  userId: string
  userName: string | null
  userEmail: string | null
  isMember: boolean
  navigate: (target: WebMcpNavigateTarget) => void
}

let current: WebMcpAppContext | null = null

export function setWebMcpAppContext(ctx: WebMcpAppContext | null): void {
  current = ctx
}

export function getWebMcpAppContext(): WebMcpAppContext {
  if (!current) {
    throw new Error(
      `No team is currently open in the app — open a team page first`
    )
  }
  return current
}
