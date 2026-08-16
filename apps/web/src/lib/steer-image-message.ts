// EXP-511: steer messages carry attached images as markdown embeds. The host
// device localizes each embed to a file path before the agent sees it, and the
// activity echo restores the embed, so this exact shape is load-bearing across
// web, iOS (SteerImageMessage.swift) and Android (SteerImageMessage.kt) — keep
// the three builders byte-identical.
export const MAX_STEER_IMAGES = 4

export function buildSteerImageMessage(
  text: string,
  attachmentIds: string[]
): string {
  const trimmed = text.trim()
  if (attachmentIds.length === 0) return trimmed
  const embeds = attachmentIds
    .map((id) => `![image](/api/attachments/${id})`)
    .join(`\n`)
  if (!trimmed) return embeds
  return `${trimmed}\n\n${embeds}`
}
