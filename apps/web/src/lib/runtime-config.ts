import { createServerFn } from "@tanstack/react-start"

export type FeedbackWidgetConfig = {
  // Absolute (self-hosted → cloud) or origin-relative (cloud) loader URL.
  scriptUrl: string
  widgetKey: string
}

export type RuntimeConfig = {
  publicFeedbackUrl: string | null
  isCloud: boolean
  creemProProductId: string | null
  creemBusinessProductId: string | null
  creemBusinessYearlyProductId: string | null
  feedbackWidget: FeedbackWidgetConfig | null
}

export function buildRuntimeConfig(): RuntimeConfig {
  const isCloud = process.env.SELF_HOSTED !== `true`
  const raw = process.env.PUBLIC_FEEDBACK_URL?.trim()

  // Self-hosted instances point their in-app feedback widget at the cloud:
  // both env vars set ⇒ widget on. On cloud the key is resolved from the
  // bootstrap-created dogfood config instead (see getRuntimeConfig).
  const scriptUrl = process.env.FEEDBACK_WIDGET_SCRIPT_URL?.trim()
  const widgetKey = process.env.FEEDBACK_WIDGET_KEY?.trim()

  return {
    publicFeedbackUrl: raw && raw.length > 0 ? raw.replace(/\/$/, ``) : null,
    isCloud,
    creemProProductId: isCloud
      ? (process.env.CREEM_PRO_PRODUCT_ID ?? null)
      : null,
    creemBusinessProductId: isCloud
      ? (process.env.CREEM_BUSINESS_PRODUCT_ID ?? null)
      : null,
    creemBusinessYearlyProductId: isCloud
      ? (process.env.CREEM_BUSINESS_YEARLY_PRODUCT_ID ?? null)
      : null,
    feedbackWidget:
      scriptUrl && widgetKey ? { scriptUrl, widgetKey } : null,
  }
}

// Cloud-only: the dogfood widget key lives in the DB (created idempotently
// by bootstrap-cloud's ensureFeedbackWidgetConfig). Cached for the process
// lifetime — it never changes after bootstrap.
let cloudWidgetPromise: Promise<FeedbackWidgetConfig | null> | null = null

function resolveCloudFeedbackWidget(): Promise<FeedbackWidgetConfig | null> {
  cloudWidgetPromise ??= (async () => {
    try {
      // The dynamic import keeps drizzle/pg out of the client graph: this
      // file is imported by client components for the serverFn + types. It
      // must target the small server-only leaf module — see the warning in
      // lib/widget/dogfood.ts before changing this.
      const { findDogfoodWidgetKey } = await import(`@/lib/widget/dogfood`)
      const widgetKey = await findDogfoodWidgetKey()
      if (!widgetKey) return null
      return { scriptUrl: `/widget/v1/loader.js`, widgetKey }
    } catch {
      cloudWidgetPromise = null
      return null
    }
  })()
  return cloudWidgetPromise
}

export const getRuntimeConfig = createServerFn({ method: `GET` }).handler(
  async () => {
    const config = buildRuntimeConfig()
    if (config.isCloud && !config.feedbackWidget) {
      config.feedbackWidget = await resolveCloudFeedbackWidget()
    }
    return config
  }
)
