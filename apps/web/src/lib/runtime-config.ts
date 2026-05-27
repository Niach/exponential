import { createServerFn } from "@tanstack/react-start"

export type RuntimeConfig = {
  publicFeedbackUrl: string | null
  isCloud: boolean
  creemProProductId: string | null
  creemBusinessProductId: string | null
}

export function buildRuntimeConfig(): RuntimeConfig {
  const isCloud = process.env.SELF_HOSTED !== `true`
  const raw = process.env.PUBLIC_FEEDBACK_URL?.trim()
  return {
    publicFeedbackUrl: raw && raw.length > 0 ? raw.replace(/\/$/, ``) : null,
    isCloud,
    creemProProductId: isCloud
      ? (process.env.CREEM_PRO_PRODUCT_ID ?? null)
      : null,
    creemBusinessProductId: isCloud
      ? (process.env.CREEM_BUSINESS_PRODUCT_ID ?? null)
      : null,
  }
}

export const getRuntimeConfig = createServerFn({ method: `GET` }).handler(() =>
  buildRuntimeConfig()
)
