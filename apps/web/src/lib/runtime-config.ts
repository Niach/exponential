import { createServerFn } from "@tanstack/react-start"

export type FeedbackWidgetConfig = {
  // Origin-relative loader URL (cloud-only — self-hosted instances never
  // embed the cloud widget, its expw_ key is domain-allowlisted; without a
  // widget the sidebar FeedbackButton simply doesn't render).
  scriptUrl: string
  widgetKey: string
}

// The cloud feedback widget (EXP-364): the `Exponential App` widget config is
// an ordinary hand-managed row in the cloud DB now — nothing seeds or heals
// it, and the key is no longer looked up at runtime. It's a PUBLIC key by
// design (the same one ships in marketing page snippets), so hardcoding it is
// fine; FEEDBACK_WIDGET_KEY overrides it for instances whose config row has a
// different key (staging). Deleting the config row in team settings leaves a
// dead key — the loader's config fetch 404s and the launcher never mounts.
const CLOUD_FEEDBACK_WIDGET_KEY = `expw_ATLHZ5hFiV5CqApwbCawPh72bPkpbHUp`

export type RuntimeConfig = {
  isCloud: boolean
  // Team plan product ids: monthly (€15/seat/mo) and yearly (€12/seat/mo,
  // billed annually). When the yearly id is present the plan card shows a
  // monthly/yearly toggle.
  creemTeamProductId: string | null
  creemTeamYearlyProductId: string | null
  feedbackWidget: FeedbackWidgetConfig | null
}

export function buildRuntimeConfig(): RuntimeConfig {
  // Mirrors isCloudInstance() (lib/bootstrap-cloud.ts) — not imported, so this
  // client-reachable module never pulls the server-only db graph.
  const isCloud = (process.env.CLOUD_INSTANCE ?? ``).toLowerCase() === `true`

  return {
    isCloud,
    creemTeamProductId: isCloud
      ? (process.env.CREEM_TEAM_PRODUCT_ID ?? null)
      : null,
    creemTeamYearlyProductId: isCloud
      ? (process.env.CREEM_TEAM_YEARLY_PRODUCT_ID ?? null)
      : null,
    feedbackWidget: isCloud
      ? {
          scriptUrl: `/widget/v1/loader.js`,
          widgetKey:
            process.env.FEEDBACK_WIDGET_KEY ?? CLOUD_FEEDBACK_WIDGET_KEY,
        }
      : null,
  }
}

export const getRuntimeConfig = createServerFn({ method: `GET` }).handler(
  async () => buildRuntimeConfig()
)

// The config is env-derived and immutable for the life of the server process,
// so client callers that re-mount per navigation share one fetch instead of a
// round trip each (EXP-457).
let cachedConfig: Promise<RuntimeConfig> | null = null
export function getRuntimeConfigCached(): Promise<RuntimeConfig> {
  cachedConfig ??= getRuntimeConfig()
  return cachedConfig
}
