import { Codex, type SandboxMode, type ThreadEvent } from "@openai/codex-sdk"
import type {
  CodingAgentDriver,
  DriverRunOptions,
  DriverRunResult,
} from "./index"
import { UnsafePermissionError } from "./index"

const FORBIDDEN_SANDBOX_MODES = new Set<SandboxMode>([`danger-full-access`])

function processEnvWith(
  overrides: Record<string, string>
): Record<string, string> {
  const env: Record<string, string> = {}
  for (const [key, value] of Object.entries(process.env)) {
    if (typeof value === `string`) env[key] = value
  }
  return { ...env, ...overrides }
}

type CodexClientOptions = ConstructorParameters<typeof Codex>[0]

export function codexOptionsForMcp(
  mcpServer: DriverRunOptions[`mcpServer`]
): CodexClientOptions {
  return mcpServer
    ? {
        env: processEnvWith({
          EXPONENTIAL_MCP_TOKEN: mcpServer.token,
        }),
        config: {
          mcp_servers: {
            exponential: {
              url: mcpServer.url,
              bearer_token_env_var: `EXPONENTIAL_MCP_TOKEN`,
            },
          },
        },
      }
    : undefined
}

export class CodexDriver implements CodingAgentDriver {
  readonly name = `codex` as const

  async run(opts: DriverRunOptions): Promise<DriverRunResult> {
    const sandboxMode: SandboxMode = `workspace-write`
    if (FORBIDDEN_SANDBOX_MODES.has(sandboxMode)) {
      throw new UnsafePermissionError(
        `Codex driver refuses sandboxMode=${sandboxMode}`
      )
    }

    const codex = new Codex(codexOptionsForMcp(opts.mcpServer))
    const thread = codex.startThread({
      workingDirectory: opts.cwd,
      sandboxMode,
      skipGitRepoCheck: false,
      approvalPolicy: `never`,
    })

    const fullPrompt = opts.systemPrompt
      ? `${opts.systemPrompt}\n\n<user_issue>\n${opts.userPrompt}\n</user_issue>`
      : opts.userPrompt

    const streamed = await thread.runStreamed(fullPrompt, {
      signal: opts.signal,
    })

    let finalText = ``
    let inputTokens = 0
    let outputTokens = 0

    for await (const event of streamed.events as AsyncIterable<ThreadEvent>) {
      switch (event.type) {
        case `item.completed`:
        case `item.updated`:
        case `item.started`: {
          const item = event.item
          if (item.type === `agent_message`) {
            opts.onEvent?.({ kind: `text`, text: item.text })
            if (event.type === `item.completed`) finalText = item.text
          } else if (item.type === `reasoning`) {
            opts.onEvent?.({ kind: `reasoning`, text: item.text })
          } else if (item.type === `command_execution`) {
            opts.onEvent?.({
              kind: `tool`,
              toolName: `bash`,
              toolInput: item.command,
              toolOutput: item.aggregated_output,
            })
          } else if (item.type === `mcp_tool_call`) {
            opts.onEvent?.({
              kind: `tool`,
              toolName: `${item.server}.${item.tool}`,
              toolInput: item.arguments,
              toolOutput: item.result ?? item.error,
            })
          } else if (item.type === `error`) {
            opts.onEvent?.({ kind: `error`, errorMessage: item.message })
          }
          break
        }
        case `turn.completed`: {
          inputTokens += event.usage.input_tokens
          outputTokens += event.usage.output_tokens
          break
        }
        case `turn.failed`: {
          throw new Error(`Codex turn failed: ${event.error.message}`)
        }
        case `error`: {
          throw new Error(`Codex stream error: ${event.message}`)
        }
        default:
          break
      }
    }

    opts.onEvent?.({ kind: `usage`, inputTokens, outputTokens })
    return { finalText, inputTokens, outputTokens }
  }
}
