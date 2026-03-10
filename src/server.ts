// src/server.ts
import handler from "@tanstack/react-start/server-entry"

export default {
  idleTimeout: 255, // seconds — max Bun allows, needed for Electric long-poll
  fetch(request: Request) {
    return handler.fetch(request)
  },
}
