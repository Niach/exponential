import { createServerFn } from "@tanstack/react-start"

export type RuntimeConfig = {
  // When set, this instance is "self-hosted": the Send-feedback button should
  // open `${publicFeedbackUrl}/feedback?...` in a new tab so the user lands
  // on the cloud's public workspace. When null, this is the cloud itself —
  // the button opens the local /feedback route instead.
  publicFeedbackUrl: string | null
}

export function buildRuntimeConfig(): RuntimeConfig {
  const raw = process.env.PUBLIC_FEEDBACK_URL?.trim()
  return {
    publicFeedbackUrl: raw && raw.length > 0 ? raw.replace(/\/$/, ``) : null,
  }
}

export const getRuntimeConfig = createServerFn({ method: `GET` }).handler(() =>
  buildRuntimeConfig()
)
