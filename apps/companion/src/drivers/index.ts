export type DriverName = `claude` | `codex`

export interface DriverEvent {
  kind: `text` | `tool` | `reasoning` | `usage` | `error`
  text?: string
  toolName?: string
  toolInput?: unknown
  toolOutput?: unknown
  inputTokens?: number
  outputTokens?: number
  errorMessage?: string
}

export interface DriverRunOptions {
  cwd: string
  systemPrompt?: string
  userPrompt: string
  mcpServer?: {
    url: string
    token: string
  }
  allowedTools?: string[]
  signal?: AbortSignal
  maxTurns?: number
  /**
   * Optional callback fired for every streamed event. The driver also returns
   * the final result; consumers that only care about the final text can skip
   * this.
   */
  onEvent?: (event: DriverEvent) => void
}

export interface DriverRunResult {
  finalText: string
  inputTokens: number
  outputTokens: number
}

export interface CodingAgentDriver {
  readonly name: DriverName
  run(opts: DriverRunOptions): Promise<DriverRunResult>
}

export class UnsafePermissionError extends Error {
  constructor(message: string) {
    super(message)
    this.name = `UnsafePermissionError`
  }
}

import { ClaudeDriver } from "./claude"
import { CodexDriver } from "./codex"

export function createDriver(name: DriverName): CodingAgentDriver {
  switch (name) {
    case `claude`:
      return new ClaudeDriver()
    case `codex`:
      return new CodexDriver()
  }
}
