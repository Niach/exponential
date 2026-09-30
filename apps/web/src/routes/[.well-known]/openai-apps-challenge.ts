import { createFileRoute } from "@tanstack/react-router"
import { handleOpenAiAppsChallenge } from "@/lib/openai-apps-challenge"

// EXP-1153: domain verification for the ChatGPT/Codex plugin directory. The
// submission portal hands the publisher a token and expects it back, as the
// bare plain-text body, from this exact path on the MCP host. The token is
// per plugin and static once set, so it rides an env var (OPENAI_APPS_CHALLENGE)
// rather than a deploy. Unset = 404, like any instance that never submits.
export const Route = createFileRoute(`/.well-known/openai-apps-challenge`)({
  server: {
    handlers: {
      GET: () => handleOpenAiAppsChallenge(process.env.OPENAI_APPS_CHALLENGE),
    },
  },
})
