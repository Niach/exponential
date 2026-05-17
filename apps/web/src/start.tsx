// src/start.tsx
import { createStart } from "@tanstack/react-start"
import { bootstrapCloud } from "@/lib/bootstrap-cloud"

// Fire-and-forget: seed the public workspace and promote initial admins.
// Idempotent; errors are logged inside bootstrapCloud().
bootstrapCloud().catch(() => {
  // already logged
})

export const startInstance = createStart(() => {
  return {
    defaultSsr: false, // or true for SSR
  }
})
