import { query } from "@anthropic-ai/claude-agent-sdk"
import type {
  CodingAgentDriver,
  DriverRunOptions,
  DriverRunResult,
} from "./index"
import { UnsafePermissionError } from "./index"

// Allowlist must not contain a permission mode that would let the agent
// execute network/filesystem commands without the daemon's say-so.
const FORBIDDEN_PERMISSION_MODES = new Set([`bypassPermissions`])

interface SdkMessageLike {
  type?: string
  message?: {
    content?: Array<{
      type?: string
      text?: string
      name?: string
      input?: unknown
    }>
    usage?: { input_tokens?: number; output_tokens?: number }
  }
  result?: string
}

export function claudeMcpServers(mcpServer: DriverRunOptions[`mcpServer`]) {
  return mcpServer
    ? {
        exponential: {
          type: `http` as const,
          url: mcpServer.url,
          headers: {
            Authorization: `Bearer ${mcpServer.token}`,
          },
          alwaysLoad: true,
        },
      }
    : undefined
}

export class ClaudeDriver implements CodingAgentDriver {
  readonly name = `claude` as const

  async run(opts: DriverRunOptions): Promise<DriverRunResult> {
    // Hard-coded safe defaults. Never let callers override these to unsafe
    // values. See FORBIDDEN_PERMISSION_MODES.
    const permissionMode = `acceptEdits` as const
    if (FORBIDDEN_PERMISSION_MODES.has(permissionMode)) {
      throw new UnsafePermissionError(
        `Claude driver refuses permissionMode=${permissionMode}`
      )
    }

    const abortController = new AbortController()
    if (opts.signal) {
      opts.signal.addEventListener(`abort`, () => abortController.abort(), {
        once: true,
      })
    }

    const fullPrompt = opts.systemPrompt
      ? `${opts.systemPrompt}\n\n<user_issue>\n${opts.userPrompt}\n</user_issue>`
      : opts.userPrompt

    const stream = query({
      prompt: fullPrompt,
      options: {
        cwd: opts.cwd,
        permissionMode,
        allowedTools: opts.allowedTools,
        maxTurns: opts.maxTurns,
        mcpServers: claudeMcpServers(opts.mcpServer),
        abortController,
        settingSources: [`project`],
      },
    })

    let finalText = ``
    let inputTokens = 0
    let outputTokens = 0

    for await (const raw of stream as AsyncIterable<SdkMessageLike>) {
      switch (raw.type) {
        case `assistant`: {
          const usage = raw.message?.usage
          if (usage?.input_tokens) inputTokens += usage.input_tokens
          if (usage?.output_tokens) outputTokens += usage.output_tokens
          for (const block of raw.message?.content ?? []) {
            if (block.type === `text` && block.text) {
              opts.onEvent?.({ kind: `text`, text: block.text })
            } else if (block.type === `tool_use`) {
              opts.onEvent?.({
                kind: `tool`,
                toolName: block.name,
                toolInput: block.input,
              })
            }
          }
          break
        }
        case `result`: {
          if (raw.result) finalText = raw.result
          break
        }
        default:
          break
      }
    }

    opts.onEvent?.({ kind: `usage`, inputTokens, outputTokens })
    return { finalText, inputTokens, outputTokens }
  }
}
