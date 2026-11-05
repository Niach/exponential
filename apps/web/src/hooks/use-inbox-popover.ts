import { useSyncExternalStore } from "react"

// #FEED-191: the wide Inbox is a popover off the sidebar header. A module store
// because its opener can be a route (`/inbox` redirects home and opens it) while
// the popover lives in the sidebar.

let open = false
const listeners = new Set<() => void>()

export function setInboxPopoverOpen(next: boolean): void {
  if (open === next) return
  open = next
  for (const listener of listeners) listener()
}

const subscribe = (listener: () => void) => {
  listeners.add(listener)
  return () => listeners.delete(listener)
}

export function useInboxPopoverOpen(): boolean {
  return useSyncExternalStore(subscribe, () => open, () => false)
}
