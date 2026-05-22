import { mkdir, readFile, unlink, writeFile } from "node:fs/promises"
import { join } from "node:path"
import TOML from "@iarna/toml"
import { CONFIG_DIR } from "./config"

const TOKEN_PATH = join(CONFIG_DIR, `github.token`)
const REFRESH_LEEWAY_MS = 5 * 60_000

interface StoredAuth {
  accessToken: string
  refreshToken?: string
  accessTokenExpiresAt?: number
  clientId: string
}

interface RawDeviceCode {
  device_code: string
  user_code: string
  verification_uri: string
  expires_in: number
  interval: number
}

interface RawTokenResponse {
  access_token?: string
  token_type?: string
  refresh_token?: string
  expires_in?: number
  refresh_token_expires_in?: number
  scope?: string
  error?: string
  error_description?: string
}

async function readStored(): Promise<StoredAuth | null> {
  try {
    const raw = await readFile(TOKEN_PATH, `utf-8`)
    const parsed = TOML.parse(raw) as Record<string, unknown>
    if (typeof parsed.access_token !== `string`) return null
    if (typeof parsed.client_id !== `string`) return null
    return {
      accessToken: parsed.access_token,
      refreshToken:
        typeof parsed.refresh_token === `string`
          ? parsed.refresh_token
          : undefined,
      accessTokenExpiresAt:
        typeof parsed.access_token_expires_at === `string`
          ? new Date(parsed.access_token_expires_at).getTime()
          : undefined,
      clientId: parsed.client_id,
    }
  } catch {
    return null
  }
}

async function writeStored(auth: StoredAuth): Promise<void> {
  await mkdir(CONFIG_DIR, { recursive: true })
  const payload: TOML.JsonMap = {
    access_token: auth.accessToken,
    client_id: auth.clientId,
  }
  if (auth.refreshToken) payload.refresh_token = auth.refreshToken
  if (auth.accessTokenExpiresAt) {
    payload.access_token_expires_at = new Date(
      auth.accessTokenExpiresAt
    ).toISOString()
  }
  await writeFile(TOKEN_PATH, TOML.stringify(payload), { mode: 0o600 })
}

async function refresh(stored: StoredAuth): Promise<StoredAuth> {
  if (!stored.refreshToken) {
    throw new Error(
      `GitHub access token expired and no refresh token available. Re-run \`companion github login\`.`
    )
  }
  const res = await fetch(`https://github.com/login/oauth/access_token`, {
    method: `POST`,
    headers: {
      "content-type": `application/x-www-form-urlencoded`,
      accept: `application/json`,
    },
    body: new URLSearchParams({
      client_id: stored.clientId,
      grant_type: `refresh_token`,
      refresh_token: stored.refreshToken,
    }),
  })
  const body = (await res.json()) as RawTokenResponse
  if (!res.ok || body.error || !body.access_token) {
    throw new Error(
      `GitHub token refresh failed: ${body.error_description ?? body.error ?? res.status}`
    )
  }
  const fresh: StoredAuth = {
    accessToken: body.access_token,
    refreshToken: body.refresh_token ?? stored.refreshToken,
    accessTokenExpiresAt: body.expires_in
      ? Date.now() + body.expires_in * 1000
      : undefined,
    clientId: stored.clientId,
  }
  await writeStored(fresh)
  return fresh
}

/**
 * Returns a usable access token, refreshing if it expires within the next
 * five minutes. Returns null when no auth has been performed yet.
 */
export async function loadAccessToken(): Promise<{ token: string } | null> {
  const stored = await readStored()
  if (!stored) return null
  const expiresAt = stored.accessTokenExpiresAt ?? Infinity
  if (expiresAt - Date.now() < REFRESH_LEEWAY_MS) {
    const refreshed = await refresh(stored)
    return { token: refreshed.accessToken }
  }
  return { token: stored.accessToken }
}

export async function loadClientId(): Promise<string | null> {
  const stored = await readStored()
  return stored?.clientId ?? null
}

export async function logoutGithub(): Promise<void> {
  await unlink(TOKEN_PATH).catch(() => {})
}

/**
 * Walk the user through the GitHub OAuth device flow. Calls `onPrompt`
 * once when a verification URL + user code are ready; resolves with the
 * fresh token once the user authorizes.
 */
export async function runDeviceFlow(args: {
  clientId: string
  scope?: string
  onPrompt: (info: { verificationUri: string; userCode: string }) => void
}): Promise<void> {
  const scope = args.scope ?? `repo read:user`

  const codeRes = await fetch(`https://github.com/login/device/code`, {
    method: `POST`,
    headers: {
      "content-type": `application/x-www-form-urlencoded`,
      accept: `application/json`,
    },
    body: new URLSearchParams({ client_id: args.clientId, scope }),
  })
  if (!codeRes.ok) {
    throw new Error(`device-code request failed: ${codeRes.status}`)
  }
  const code = (await codeRes.json()) as RawDeviceCode
  if (!code.device_code) {
    throw new Error(
      `GitHub did not return a device code. Make sure "Enable Device Flow" is checked on the OAuth App.`
    )
  }
  args.onPrompt({
    verificationUri: code.verification_uri,
    userCode: code.user_code,
  })

  const deadline = Date.now() + code.expires_in * 1000
  let interval = code.interval * 1000

  while (Date.now() < deadline) {
    await new Promise((r) => setTimeout(r, interval))
    const tokenRes = await fetch(
      `https://github.com/login/oauth/access_token`,
      {
        method: `POST`,
        headers: {
          "content-type": `application/x-www-form-urlencoded`,
          accept: `application/json`,
        },
        body: new URLSearchParams({
          client_id: args.clientId,
          device_code: code.device_code,
          grant_type: `urn:ietf:params:oauth:grant-type:device_code`,
        }),
      }
    )
    const body = (await tokenRes.json()) as RawTokenResponse
    if (body.access_token) {
      await writeStored({
        accessToken: body.access_token,
        refreshToken: body.refresh_token,
        accessTokenExpiresAt: body.expires_in
          ? Date.now() + body.expires_in * 1000
          : undefined,
        clientId: args.clientId,
      })
      return
    }
    switch (body.error) {
      case `authorization_pending`:
        // Keep polling.
        break
      case `slow_down`:
        interval += 5_000
        break
      case `expired_token`:
        throw new Error(`Device code expired before authorization completed.`)
      case `access_denied`:
        throw new Error(`Authorization was denied by the user.`)
      default:
        throw new Error(
          `Unexpected device-flow error: ${body.error ?? `unknown`} ${body.error_description ?? ``}`
        )
    }
  }
  throw new Error(`Timed out waiting for GitHub authorization.`)
}
