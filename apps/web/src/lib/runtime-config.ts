import { createServerFn } from "@tanstack/react-start"

export type FeedbackWidgetConfig = {
  // Origin-relative loader URL (cloud-only — self-hosted instances never
  // embed the cloud widget, its expw_ key is domain-allowlisted; without a
  // widget the sidebar FeedbackButton simply doesn't render).
  scriptUrl: string
  widgetKey: string
}

export type RuntimeConfig = {
  isCloud: boolean
  creemProProductId: string | null
  creemBusinessProductId: string | null
  creemBusinessYearlyProductId: string | null
  feedbackWidget: FeedbackWidgetConfig | null
}

export function buildRuntimeConfig(): RuntimeConfig {
  const isCloud = process.env.SELF_HOSTED !== `true`

  return {
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
    // Cloud-only — filled in from the DB by getRuntimeConfig.
    feedbackWidget: null,
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
      if (!widgetKey) {
        // No key yet (bootstrap still running, or the config is toggled off)
        // — don't memoize the null, or the embedded widget stays disabled for
        // the process lifetime even after the key appears.
        cloudWidgetPromise = null
        return null
      }
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
