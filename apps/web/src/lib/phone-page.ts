import { phonePage, type PhoneTabId } from "@exp/domain-contract/chats"

// #EXP-2109 (chats.json `shell.pages`): which page a web-mobile path is, so
// the docked bar knows the tab it lights and whether it shows. A path that is
// no listed page is a detail: no bar. Pure.

const trim = (path: string) => (path.length > 1 ? path.replace(/\/+$/, ``) : path)

/** The fixture's page id of a path; null = a detail. */
export function phonePageOf(pathname: string): string | null {
  const path = trim(pathname)
  if (path === `/chats`) return `chats`
  if (path === `/teams`) return `teams`
  if (path === `/assistant`) return `assistant`
  if (path === `/settings`) return `settings`
  if (path.startsWith(`/settings/`)) return `settings-page`
  if (path === `/devices`) return `devices`
  if (/^\/devices\/[^/]+(\/settings)?$/.test(path)) return `device`
  if (/^\/t\/[^/]+\/team$/.test(path)) return `team`
  if (/^\/t\/[^/]+\/boards\/[^/]+$/.test(path)) return `board`
  if (/^\/t\/[^/]+\/settings(\/|$)/.test(path)) return `team-settings`
  return null
}

export function phonePagePlace(pathname: string): { tab: PhoneTabId | null; bar: boolean } {
  const page = phonePageOf(pathname)
  return page === null ? { tab: null, bar: false } : phonePage(page)
}
