// EXP-637: how a tool call knows which session it is running inside.
//
// The desktop/CLI launcher injects this header into the MCP config it writes
// for the agent (claude via the JSON `headers` block, codex via
// `-c mcp_servers.exponential.http_headers`), so every request the agent makes
// carries the id of the `sessions` row (EXP-1291) it runs as. That is what
// lets `exponential_sessions_end` close out the right session, the Guide land
// on the right thread and `exponential_pr_open` name the opening session.
//
// The id is not a secret and grants nothing on its own: ownership is enforced
// per tool against the resolved MCP user, never at request-parse time. A
// malformed value is treated as absent — a bad header must degrade to the
// header-less behaviour, never fail the request.
export const MCP_SESSION_HEADER = `x-exp-session-id`

const UUID_RE =
  /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i

/** The calling session's id, or null when absent/malformed. */
export function parseMcpSessionHeader(request: Request): string | null {
  // Header lookup is case-insensitive per the Fetch spec, so the launcher's
  // `X-Exp-Session-Id` casing resolves here unchanged.
  const raw = request.headers.get(MCP_SESSION_HEADER)
  if (!raw) return null
  const value = raw.trim()
  return UUID_RE.test(value) ? value.toLowerCase() : null
}
