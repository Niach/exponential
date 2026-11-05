import fixture from "../fixtures/via-app.json" with { type: "json" }
import { APP_ACTIONS, declaredActions, functionPatternMatches, parseActionName } from "./app-actions"

// VAPP-229 (fixtures/via-app.json): the "via <app>" caption of every row an
// app caused (issues, sessions, records; comments read reporter-reply.json's
// `captions.viaApp`, the same string). TS reference; desktop domain::via_app,
// iOS ExpCore ViaApp.swift and Android domain/ViaApp.kt replay the cases.

export const VIA_APP_COPY = fixture.copy

/** "via {app}", or null without a (resolved) app name. */
export function viaAppCaption(appName: string | null | undefined): string | null {
  return appName ? VIA_APP_COPY.viaApp.replace(`{app}`, appName) : null
}

/** `text` with the caption appended after the separator (the text alone
 *  without an app). */
export function withViaApp(text: string, appName: string | null | undefined): string {
  const caption = viaAppCaption(appName)
  return caption ? `${text}${VIA_APP_COPY.separator}${caption}` : text
}

// VAPP-231 (fixture `claim`): a host's claim that a write comes from its app.

/** The claim on an MCP call. */
export const VIA_APP_HEADER = fixture.claim.header
/** What a record write (upsert or batch) is judged by. */
export const VIA_APP_RECORDS_FUNCTION = fixture.claim.recordsFunction

export interface ViaAppClaim {
  appId: string
  version: number
}

/** The claim of a host over `appId`; null without an app or a published version. */
export function viaAppClaim(appId: string | null | undefined, publishedVersion: number | null | undefined): ViaAppClaim | null {
  return appId && publishedVersion && publishedVersion > 0 ? { appId, version: publishedVersion } : null
}

/** A claim as the MCP header's value. */
export function viaAppHeaderValue(claim: ViaAppClaim): string {
  return `${claim.appId}@${claim.version}`
}

const HEADER_VALUE = /^([0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12})@([1-9][0-9]{0,8})$/i

/** The MCP header back into a claim; null when absent or malformed. */
export function parseViaAppHeader(value: string | null | undefined): ViaAppClaim | null {
  const match = value ? HEADER_VALUE.exec(value) : null
  return match ? { appId: match[1]!, version: Number(match[2]) } : null
}

/** What a claim is checked for: a named function, or an MCP tool and its op. */
export type ViaAppCall = { fn: string } | { tool: string; op: string | null }

/** The typed action a tool and op run; null for anything not offered. */
function actionOfTool(tool: string, op: string | null): string | null {
  return (
    APP_ACTIONS.find((name) => {
      const target = parseActionName(name)
      return target?.tool === tool && target.op === op
    }) ?? null
  )
}

/** Does a package's `functions` list declare the call? */
export function viaAppDeclares(functions: unknown, call: ViaAppCall): boolean {
  if (!Array.isArray(functions)) return false
  const list = functions.filter((f): f is string => typeof f === `string`)
  const named = (name: string) => list.some((pattern) => functionPatternMatches(pattern, name))
  if (`fn` in call) return named(call.fn)
  const action = actionOfTool(call.tool, call.op)
  if (!action) return false
  return named(fixture.claim.mcpFunction) || declaredActions(list).includes(action)
}

/** The version and access half of the server's verdict (`verdictCases`). */
export function viaAppAccepted(args: { version: number; publishedVersion: number; access: string; functions: unknown; call: ViaAppCall }): boolean {
  if (args.version < 1 || args.version > args.publishedVersion) return false
  if (args.access === `none`) return false
  return viaAppDeclares(args.functions, args.call)
}
