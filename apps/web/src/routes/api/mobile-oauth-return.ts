import { createFileRoute } from "@tanstack/react-router"
import { auth } from "@/lib/auth"
import {
  normalizeOauthErrorReason,
  oauthErrorMessage,
  oauthReturnCodeDeepLink,
  oauthReturnErrorDeepLink,
} from "@/lib/deep-link"
import {
  isValidCodeChallenge,
  mintMobileOauthCode,
  stateCookieSecureAttribute,
} from "@/lib/auth/mobile-oauth-code"

const STATE_COOKIE_NAME = `exp_mobile_oauth_state`

// Clear with the same `Secure` posture /api/mobile-oauth-start SET the cookie
// with — on an http self-host a Secure clear would be discarded by the
// browser and the state cookie would linger.
function clearStateCookie(request: Request): string {
  const secure = stateCookieSecureAttribute(request)
  return `${STATE_COOKIE_NAME}=; Path=/; Max-Age=0; HttpOnly${secure}; SameSite=Lax`
}

function readCookie(cookieHeader: string, name: string): string | null {
  const entry = cookieHeader
    .split(`;`)
    .map((s) => s.trim())
    .find((c) => c.startsWith(`${name}=`))
  if (!entry) return null
  return decodeURIComponent(entry.slice(name.length + 1))
}

// The final hop hands off to the native app via the `exponential://` custom
// scheme. A bare 302 to it leaves a real desktop browser tab spinning forever —
// the OS grabs the scheme but the browser can never "complete" the navigation.
// So we serve a 200 HTML page that fires the deep link from JS (iOS's
// ASWebAuthenticationSession and desktop's registered handler both intercept
// it) AND shows a card the browser can render.
//
// FAILURES take the exact same shape (REV2-53): an https error page is invisible
// to every native completion channel, so a denied/expired sign-in would leave
// the auth sheet open on a dead end. `deepLink` and `webLink` are built here
// (already percent-encoded, URL-safe) and `body` comes from a fixed message
// table, so both are inert in the href attributes and the JSON-stringified
// script string.
function renderHandoffPage(page: {
  ok: boolean
  title: string
  heading: string
  body: string
  deepLink: string
  webLink?: string
}): string {
  const accent = page.ok ? `#22c55e` : `#f87171`
  const glyph = page.ok
    ? `<path d="M20 6 9 17l-5-5"/>`
    : `<path d="M12 8v5"/><path d="M12 16h.01"/><circle cx="12" cy="12" r="9"/>`
  return `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8" />
<meta name="viewport" content="width=device-width, initial-scale=1" />
<title>${page.title}</title>
<style>
  :root { color-scheme: dark; }
  html, body { height: 100%; margin: 0; }
  body {
    display: grid; place-items: center;
    font-family: ui-sans-serif, system-ui, -apple-system, "Segoe UI", Roboto, sans-serif;
    background: #09090b; color: #fafafa;
    -webkit-font-smoothing: antialiased;
  }
  .card {
    text-align: center; padding: 2.5rem 2rem; max-width: 24rem;
    border: 1px solid #27272a; border-radius: 16px; background: #18181b;
  }
  .check {
    width: 48px; height: 48px; margin: 0 auto 1.25rem;
    border-radius: 999px; background: ${accent}1a;
    display: grid; place-items: center; color: ${accent};
  }
  h1 { font-size: 1.25rem; font-weight: 600; margin: 0 0 0.5rem; }
  p { font-size: 0.9rem; line-height: 1.5; color: #a1a1aa; margin: 0 0 1.5rem; }
  a.btn {
    display: inline-block; text-decoration: none; font-size: 0.875rem; font-weight: 500;
    padding: 0.5rem 1rem; border-radius: 8px; background: #fafafa; color: #09090b;
  }
  a.alt {
    display: block; margin-top: 1rem; font-size: 0.8rem; color: #a1a1aa;
  }
</style>
</head>
<body>
  <main class="card">
    <div class="check">
      <svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">${glyph}</svg>
    </div>
    <h1>${page.heading}</h1>
    <p>${page.body}</p>
    <a class="btn" href="${page.deepLink}">Open Exponential</a>
    ${page.webLink ? `<a class="alt" href="${page.webLink}">Sign in on the web instead</a>` : ``}
  </main>
  <script>
    // Hand off to the native app immediately; the card stays put.
    window.location.href = ${JSON.stringify(page.deepLink)};
  </script>
</body>
</html>
`
}

// Every failure branch answers with the SAME 200 HTML handoff — carrying the
// error deep link instead of a credential — so the native auth sheet completes
// and the app can say what went wrong. The secondary link keeps the desktop
// browser case (no app registered for the scheme) recoverable.
function failureResponse(request: Request, rawReason: unknown): Response {
  const reason = normalizeOauthErrorReason(rawReason)
  return new Response(
    renderHandoffPage({
      ok: false,
      title: `Sign-in failed · Exponential`,
      heading: `Sign-in didn't finish`,
      body: `${oauthErrorMessage(reason)} You can close this tab and try again in the app.`,
      deepLink: oauthReturnErrorDeepLink(reason),
      webLink: new URL(
        `/auth/login?error=${encodeURIComponent(reason)}`,
        request.url
      ).toString(),
    }),
    {
      status: 200,
      headers: {
        "Content-Type": `text/html; charset=utf-8`,
        "Set-Cookie": clearStateCookie(request),
        "Cache-Control": `no-store`,
      },
    }
  )
}

export const Route = createFileRoute(`/api/mobile-oauth-return`)({
  server: {
    handlers: {
      GET: async ({ request }) => {
        const cookieHeader = request.headers.get(`cookie`) ?? ``
        // Provider-side denial (the common failure: the user cancels at
        // Google) or any Better Auth callback error — /api/mobile-oauth-start
        // points `errorCallbackURL` back here, and Better Auth appends its
        // reason as `?error=`. Handled before the state check: the app must
        // learn the flow failed even if the anti-CSRF cookie has expired.
        const providerError = new URL(request.url).searchParams.get(`error`)
        if (providerError) {
          console.warn(`[mobile-oauth-return] provider error: ${providerError}`)
          return failureResponse(request, providerError)
        }

        // Anti-CSRF for the deep-link hop: the cookie was set by
        // /api/mobile-oauth-start, so absence means this URL was visited
        // out-of-band. Better Auth's own state cookie already protected the
        // Google → /api/auth/callback/google leg, and Better Auth doesn't
        // propagate `state` to callbackURL, so we don't compare it here.
        const stateCookie = readCookie(cookieHeader, STATE_COOKIE_NAME)
        if (!stateCookie) {
          console.warn(
            `[mobile-oauth-return] missing ${STATE_COOKIE_NAME} cookie — rejecting`
          )
          return failureResponse(request, `state_missing`)
        }

        // PKCE (REV-13, required since EXP-543): /api/mobile-oauth-start
        // appends the client's S256 code_challenge to the state cookie as
        // `<state>.<challenge>`. A missing or malformed second segment is
        // rejected like a missing cookie — there is no challenge-less flow
        // any more.
        const [, codeChallenge] = stateCookie.split(`.`)
        if (codeChallenge === undefined || !isValidCodeChallenge(codeChallenge)) {
          console.warn(
            `[mobile-oauth-return] missing or malformed code_challenge in ${STATE_COOKIE_NAME} cookie — rejecting`
          )
          return failureResponse(request, `state_invalid`)
        }

        const session = await auth.api.getSession({ headers: request.headers })
        if (!session?.session) {
          console.warn(`[mobile-oauth-return] no session — deep-linking the failure back`)
          return failureResponse(request, `no_session`)
        }

        const ctx = await auth.$context
        const cookieName = ctx.authCookies.sessionToken.name
        const token = readCookie(cookieHeader, cookieName)

        if (!token) {
          console.warn(
            `[mobile-oauth-return] session present but session-cookie '${cookieName}' missing — falling back`
          )
          return failureResponse(request, `session_cookie_missing`)
        }

        // `?code=…#code=…` (REV-13, built in lib/deep-link.ts): a single-use
        // short-TTL code the app redeems via POST /api/mobile-oauth-exchange
        // with its code_verifier — the raw session token never rides the deep
        // link. The payload rides in BOTH the query AND the fragment (EXP-21):
        // the handoff is a client-side `window.location.href =
        // "exponential://…"`, and when a browser hands a custom scheme to the
        // OS it drops the URL #fragment (a client-only construct) — so on
        // Linux the desktop app's xdg handler received a payloadless
        // `exponential://oauth-return` and never signed in. The query survives
        // that hop; the desktop parser reads it. iOS's
        // ASWebAuthenticationSession keeps the whole URL and reads the
        // fragment, so keep the fragment too rather than switching to
        // query-only.
        const target = oauthReturnCodeDeepLink(
          mintMobileOauthCode(token, codeChallenge)
        )
        // 200 HTML (not a 302 to the custom scheme) so the browser tab renders
        // a confirmation instead of spinning on an uncompletable navigation.
        return new Response(
          renderHandoffPage({
            ok: true,
            title: `Signed in · Exponential`,
            heading: `You're signed in`,
            body: `Exponential is opening. You can close this tab and return to the app.`,
            deepLink: target,
          }),
          {
            status: 200,
            headers: {
              "Content-Type": `text/html; charset=utf-8`,
              "Set-Cookie": clearStateCookie(request),
              "Cache-Control": `no-store`,
            },
          }
        )
      },
    },
  },
})
