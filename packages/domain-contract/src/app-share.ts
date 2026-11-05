import fixture from "../fixtures/app-share.json" with { type: "json" }

// VAPP-241 (fixtures/app-share.json, spec docs/spec/apps.md §9.4): an app's
// PUBLIC LINKS. TS reference; desktop domain::app_share, iOS ExpCore
// AppShare.swift and Android domain/AppShare.kt replay the cases.

export const APP_SHARE = fixture
export const APP_SHARE_COPY = fixture.copy
export const APP_SHARE_CAPS = fixture.caps
/** VAPP-247: the Public links row's expiry items, in menu order (`days` null = never). */
export const APP_SHARE_EXPIRY_CHOICES: readonly { days: number | null; label: string }[] = fixture.expiryChoices

export type ShareLinkState = `off` | `on` | `expired` | `orphaned`
export type ShareMode = `submit` | `view`

export interface ShareSchema {
  required?: string[]
  properties: Record<string, { format?: string; items?: { format?: string }; type?: string }>
}

/** A link's state at `now` (`stateCases`); `declared` = the published
 *  package still declares the share. */
export function linkState(link: { revokedAt: string | null; expiresAt: string | null } | null, declared: boolean, now: string): ShareLinkState {
  if (!link || link.revokedAt) return `off`
  if (!declared) return `orphaned`
  if (link.expiresAt && Date.parse(link.expiresAt) <= Date.parse(now)) return `expired`
  return `on`
}

/** May one more submission land (`capCases`)? */
export function underShareCaps(ipCount: number, linkCount: number): boolean {
  return ipCount < fixture.caps.perIpPerHour && linkCount < fixture.caps.perLinkPerHour
}

const isFile = (f: ShareSchema[`properties`][string]) => f.format === `attachment` || f.items?.format === `attachment`

/** The fields a link exposes (`fieldCases`): the declared `fields` in their
 *  order, else the Data tab's order (required, then by name); unknown names
 *  and files dropped. */
export function shareFields(schema: ShareSchema, fields?: readonly string[]): string[] {
  const names = Object.keys(schema.properties)
  const required = (schema.required ?? []).filter((n, i, all) => names.includes(n) && all.indexOf(n) === i)
  const order = fields ?? [...required, ...names.filter((n) => !required.includes(n)).sort()]
  return order.filter((n) => schema.properties[n] && !isFile(schema.properties[n]!))
}

/** The By/author label (`bylineCases`): a shared record reads Guest. */
export function shareByline(shareName: string | null | undefined, byline: string): string {
  return shareName ? fixture.copy.guest : byline
}

/** A Settings row's subtitle (`rowCases`). */
export function shareRowSubtitle(mode: ShareMode, state: ShareLinkState, expiresAt: string | null): string {
  const parts = [fixture.copy[mode], fixture.copy[state]]
  if (state === `on` && expiresAt) parts.push(fixture.copy.expiresOn.replace(`{date}`, expiresAt))
  return parts.join(` · `)
}

/** A Settings row's Switch (`switchCases`): an orphaned link still reads on,
 *  so its owner can revoke it. */
export function shareSwitchOn(state: ShareLinkState): boolean {
  return state === `on` || state === `orphaned`
}
