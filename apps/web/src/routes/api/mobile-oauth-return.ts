import { createFileRoute } from "@tanstack/react-router"
import { auth } from "@/lib/auth"
import { oauthReturnCodeDeepLink, oauthReturnDeepLink } from "@/lib/deep-link"
import {
  isValidCodeChallenge,
  mintMobileOauthCode,
} from "@/lib/auth/mobile-oauth-code"

const FAILED_REDIRECT = `/auth/login?error=mobile_oauth_failed`
const STATE_COOKIE_NAME = `exp_mobile_oauth_state`
const CLEAR_STATE_COOKIE = `${STATE_COOKIE_NAME}=; Path=/; Max-Age=0; HttpOnly; Secure; SameSite=Lax`

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
// it) AND shows a "you can close this tab" confirmation the browser can render.
// `deepLink` is already percent-encoded (URL-safe), so it's inert in both the
// href attribute and the JSON-stringified script string.
function renderReturnPage(deepLink: string): string {
  return `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8" />
<meta name="viewport" content="width=device-width, initial-scale=1" />
<title>Signed in — Exponential</title>
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
    border-radius: 999px; background: #22c55e1a;
    display: grid; place-items: center; color: #22c55e;
  }
  h1 { font-size: 1.25rem; font-weight: 600; margin: 0 0 0.5rem; }
  p { font-size: 0.9rem; line-height: 1.5; color: #a1a1aa; margin: 0 0 1.5rem; }
  a.btn {
    display: inline-block; text-decoration: none; font-size: 0.875rem; font-weight: 500;
    padding: 0.5rem 1rem; border-radius: 8px; background: #fafafa; color: #09090b;
  }
</style>
</head>
<body>
  <main class="card">
    <div class="check">
      <svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><path d="M20 6 9 17l-5-5"/></svg>
    </div>
    <h1>You're signed in</h1>
    <p>Exponential is opening. You can close this tab and return to the app.</p>
    <a class="btn" href="${deepLink}">Open Exponential</a>
  </main>
  <script>
    // Hand off to the native app immediately; the confirmation card stays put.
    window.location.href = ${JSON.stringify(deepLink)};
  </script>
</body>
</html>
`
}

export const Route = createFileRoute(`/api/mobile-oauth-return`)({
  server: {
    handlers: {
      GET: async ({ request }) => {
        const cookieHeader = request.headers.get(`cookie`) ?? ``
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
          return new Response(`Invalid OAuth state`, {
            status: 400,
            headers: { "Set-Cookie": CLEAR_STATE_COOKIE },
          })
        }

        // PKCE (REV-13): /api/mobile-oauth-start appends the client's S256
        // code_challenge to the state cookie as `<state>.<challenge>`. Its
        // presence flips this page to the code deep link; a malformed second
        // segment is rejected like a missing cookie.
        const [, codeChallenge] = stateCookie.split(`.`)
        if (codeChallenge !== undefined && !isValidCodeChallenge(codeChallenge)) {
          console.warn(
            `[mobile-oauth-return] malformed code_challenge in ${STATE_COOKIE_NAME} cookie — rejecting`
          )
          return new Response(`Invalid OAuth state`, {
            status: 400,
            headers: { "Set-Cookie": CLEAR_STATE_COOKIE },
          })
        }

        const session = await auth.api.getSession({ headers: request.headers })
        if (!session?.session) {
          console.warn(`[mobile-oauth-return] no session — falling back to ${FAILED_REDIRECT}`)
          return new Response(null, {
            status: 302,
            headers: {
              Location: new URL(FAILED_REDIRECT, request.url).toString(),
              "Set-Cookie": CLEAR_STATE_COOKIE,
            },
          })
        }

        const ctx = await auth.$context
        const cookieName = ctx.authCookies.sessionToken.name
        const token = readCookie(cookieHeader, cookieName)

        if (!token) {
          console.warn(
            `[mobile-oauth-return] session present but session-cookie '${cookieName}' missing — falling back`
          )
          return new Response(null, {
            status: 302,
            headers: {
              Location: new URL(FAILED_REDIRECT, request.url).toString(),
              "Set-Cookie": CLEAR_STATE_COOKIE,
            },
          })
        }

        // Two deep-link forms (both built in lib/deep-link.ts), each riding
        // its payload in BOTH the query AND the fragment (EXP-21): the handoff
        // is a client-side `window.location.href = "exponential://…"`, and
        // when a browser hands a custom scheme to the OS it drops the URL
        // #fragment (a client-only construct) — so on Linux the desktop app's
        // xdg handler received a payloadless `exponential://oauth-return` and
        // never signed in. The query survives that hop; the desktop parser
        // reads it. iOS's ASWebAuthenticationSession keeps the whole URL and
        // reads the fragment, so keep the fragment too rather than switching
        // to query-only.
        //
        // - `?code=…#code=…` (REV-13, when the client presented a PKCE
        //   challenge at /api/mobile-oauth-start): a single-use short-TTL
        //   code the app redeems via POST /api/mobile-oauth-exchange with its
        //   code_verifier — the raw session token never rides the deep link.
        // - `?token=…#token=…` (DEPRECATED legacy, challenge-less starts
        //   only): the raw session token, kept so old installed builds keep
        //   signing in until the deprecation window closes.
        const target = codeChallenge
          ? oauthReturnCodeDeepLink(mintMobileOauthCode(token, codeChallenge))
          : oauthReturnDeepLink(token)
        // 200 HTML (not a 302 to the custom scheme) so the browser tab renders
        // a confirmation instead of spinning on an uncompletable navigation.
        return new Response(renderReturnPage(target), {
          status: 200,
          headers: {
            "Content-Type": `text/html; charset=utf-8`,
            "Set-Cookie": CLEAR_STATE_COOKIE,
            "Cache-Control": `no-store`,
          },
        })
      },
    },
  },
})
