import { formatDistanceToNowStrict } from "date-fns"
import type { User } from "@/db/schema"
import { displayUserName } from "@/lib/user-display"

export function relativeTime(date: Date | string | null | undefined): string {
  if (!date) return ``
  const value = typeof date === `string` ? new Date(date) : date
  if (Number.isNaN(value.getTime())) return ``
  return formatDistanceToNowStrict(value, { addSuffix: true })
}

export function authorLabel(
  author: User | undefined,
  authorId?: string | null
): string {
  return displayUserName(author, authorId)
}
