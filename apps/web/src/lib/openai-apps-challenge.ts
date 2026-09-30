// EXP-1153: the ChatGPT/Codex plugin directory verifies that the publisher
// controls the MCP host by fetching `/.well-known/openai-apps-challenge` and
// comparing the body to the token it showed in the portal — "only the exact
// token, not JSON or a list of tokens". Pure so the route stays a one-liner
// and the test needs no server.
export function handleOpenAiAppsChallenge(token: string | undefined): Response {
  const trimmed = token?.trim()
  if (!trimmed) {
    return new Response(`Not found`, {
      status: 404,
      headers: { "content-type": `text/plain; charset=utf-8` },
    })
  }
  return new Response(trimmed, {
    status: 200,
    headers: {
      "content-type": `text/plain; charset=utf-8`,
      // Never cache a verification token: a rotated one must show at once.
      "cache-control": `no-store`,
    },
  })
}
