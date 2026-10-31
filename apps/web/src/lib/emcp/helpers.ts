export function serialize(value: unknown): unknown {
  if (value == null) return value ?? null
  if (value instanceof Date) return value.toISOString()
  if (Array.isArray(value)) return value.map(serialize)
  if (typeof value === `object`) {
    const out: Record<string, unknown> = {}
    for (const [k, v] of Object.entries(value as Record<string, unknown>)) {
      out[k] = serialize(v)
    }
    return out
  }
  return value
}

export function ok(data: unknown) {
  return {
    content: [
      {
        type: `text` as const,
        text: JSON.stringify(serialize(data), null, 2),
      },
    ],
  }
}

export function err(error: unknown) {
  const message = error instanceof Error ? error.message : String(error)
  console.error(`[mcp] tool error:`, error)
  return {
    isError: true,
    content: [{ type: `text` as const, text: message }],
  }
}

export function jsonResponse(status: number, data: unknown): Response {
  return new Response(JSON.stringify(data), {
    status,
    headers: { "content-type": `application/json` },
  })
}
