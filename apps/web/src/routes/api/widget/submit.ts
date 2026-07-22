import { createFileRoute } from "@tanstack/react-router"
import { isOriginAllowed } from "@/lib/widget/origin"
import { corsHeaders, jsonResponse, preflightResponse } from "@/lib/widget/cors"
import {
  clientIpFromRequest,
  getWidgetRateLimiters,
} from "@/lib/widget/rate-limit"
import {
  createWidgetSubmission,
  createWidgetSupportSubmission,
  loadWidgetConfigByKey,
  maxSubmitRequestBytes,
  WidgetRequestError,
} from "@/lib/widget/service"

async function handleWidgetSubmit(request: Request): Promise<Response> {
  // Reject oversized bodies before buffering the multipart payload.
  const contentLength = Number.parseInt(
    request.headers.get(`content-length`) ?? ``,
    10
  )
  if (Number.isFinite(contentLength) && contentLength > maxSubmitRequestBytes) {
    return jsonResponse(413, { error: `Request too large` })
  }

  let formData: FormData
  try {
    formData = await request.formData()
  } catch {
    return jsonResponse(400, { error: `Expected multipart form data` })
  }

  const key = formData.get(`key`)
  let config
  try {
    config = await loadWidgetConfigByKey(typeof key === `string` ? key : ``)
  } catch (error) {
    if (error instanceof WidgetRequestError) {
      return jsonResponse(error.status, { error: error.message })
    }
    throw error
  }

  const origin = isOriginAllowed(
    request.headers.get(`origin`),
    request.headers.get(`referer`),
    config.allowedDomains
  )
  if (!origin.allowed) {
    return jsonResponse(403, { error: `Origin not allowed` })
  }
  const cors = corsHeaders(origin.echoOrigin)

  if (!config.enabled) {
    return jsonResponse(403, { error: `Widget is disabled` }, cors)
  }

  const { perKeyLimiter, perIpLimiter, perEmailLimiter } =
    getWidgetRateLimiters()
  // Per-IP bucket first, short-circuiting: a request already throttled by its
  // own IP must not keep draining the shared per-key bucket — otherwise one
  // hostile IP 429s every legitimate reporter of that widget.
  const ipLimit = perIpLimiter.tryTake(`ip:${clientIpFromRequest(request)}`)
  if (!ipLimit.ok) {
    return jsonResponse(
      429,
      { error: `Too many submissions, try again later` },
      { ...cors, "Retry-After": String(ipLimit.retryAfterSeconds) }
    )
  }
  const keyLimit = perKeyLimiter.tryTake(`key:${config.publicKey}`)
  if (!keyLimit.ok) {
    return jsonResponse(
      429,
      { error: `Too many submissions, try again later` },
      { ...cors, "Retry-After": String(keyLimit.retryAfterSeconds) }
    )
  }
  // Per-recipient-address bucket: a submission carrying an email can trigger
  // outbound mail to that address, so one address can't be mail-bombed via
  // repeated submits regardless of key/IP rotation.
  const reporterEmail = formData.get(`email`)
  if (typeof reporterEmail === `string` && reporterEmail.trim().length > 0) {
    const emailLimit = perEmailLimiter.tryTake(
      `email:${reporterEmail.trim().toLowerCase()}`
    )
    if (!emailLimit.ok) {
      return jsonResponse(
        429,
        { error: `Too many submissions, try again later` },
        { ...cors, "Retry-After": String(emailLimit.retryAfterSeconds) }
      )
    }
  }

  // Honeypot: the real widget never fills this hidden field. Pretend success
  // so bots don't adapt; nothing is created.
  const honeypot = formData.get(`website`)
  if (typeof honeypot === `string` && honeypot.length > 0) {
    return jsonResponse(201, { ok: true }, cors)
  }

  try {
    // mode=support files a helpdesk ticket (EXP-130); everything else is the
    // classic feedback submission — absent mode keeps old cached bundles
    // working unchanged.
    const submit =
      formData.get(`mode`) === `support`
        ? createWidgetSupportSubmission
        : createWidgetSubmission
    const result = await submit({
      config,
      formData,
      userAgent: request.headers.get(`user-agent`),
    })
    return jsonResponse(201, { ok: true, ...result }, cors)
  } catch (error) {
    if (error instanceof WidgetRequestError) {
      return jsonResponse(
        error.status,
        { error: error.message, ...(error.code ? { code: error.code } : {}) },
        cors
      )
    }
    console.error(`widget submit error`, error)
    return jsonResponse(500, { error: `Internal error` }, cors)
  }
}

export const Route = createFileRoute(`/api/widget/submit`)({
  server: {
    handlers: {
      POST: ({ request }) => handleWidgetSubmit(request),
      // Permissive preflight echo — see config.ts for rationale.
      OPTIONS: ({ request }) =>
        preflightResponse(request.headers.get(`origin`)),
    },
  },
})
