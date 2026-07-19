import { useState } from "react"
import { Check, Copy, Plug } from "lucide-react"
import { Button } from "@/components/ui/button"
import {
  Tabs,
  TabsContent,
  TabsList,
  TabsTrigger,
} from "@/components/ui/tabs"
import {
  ClaudeIcon,
  CursorIcon,
  OpenAiIcon,
} from "@/components/icons/brand-icons"
import {
  buildCodexMcpAddCommand,
  buildMcpAddCommand,
  buildMcpEndpoint,
  buildMcpRemoteBridgeCommand,
  buildMcpServersConfig,
} from "@/lib/widget-snippet"
import { docsUrl } from "@/lib/docs-links"

// Per-client MCP setup instructions for the getting-started checklist
// (EXP-141). Content verified July 2026 — concise numbered steps + one
// copyable snippet per client; the full walkthroughs live in the docs.

export function CopySnippetButton({
  label,
  text,
}: {
  label: string
  text: string
}) {
  const [copied, setCopied] = useState(false)
  return (
    <Button
      size="sm"
      variant="outline"
      onClick={() => {
        void navigator.clipboard.writeText(text)
        setCopied(true)
        window.setTimeout(() => setCopied(false), 1_500)
      }}
    >
      {copied ? (
        <>
          <Check className="mr-1 size-4" /> Copied
        </>
      ) : (
        <>
          <Copy className="mr-1 size-4" /> {label}
        </>
      )}
    </Button>
  )
}

function Snippet({ text, copyLabel = `Copy` }: { text: string; copyLabel?: string }) {
  return (
    <div className="space-y-2">
      <pre className="max-h-48 overflow-auto rounded-md border bg-muted/30 p-3 text-left text-xs">
        {text}
      </pre>
      <CopySnippetButton label={copyLabel} text={text} />
    </div>
  )
}

function Steps({ children }: { children: React.ReactNode }) {
  return (
    <ol className="list-decimal space-y-1.5 pl-5 text-sm text-muted-foreground">
      {children}
    </ol>
  )
}

function Note({ children }: { children: React.ReactNode }) {
  return <p className="text-xs text-muted-foreground/80">{children}</p>
}

export function McpSetupTabs() {
  // SSR-safe origin (defaultSsr is off, but keep the guard — same pattern as
  // the previous cards): the cloud origin is only a placeholder fallback.
  const origin =
    typeof window === `undefined`
      ? `https://app.exponential.at`
      : window.location.origin
  const endpoint = buildMcpEndpoint(origin)

  return (
    <div className="space-y-3">
      {/* Two-level tabs (EXP-184): six flat client tabs overflowed the card
          horizontally — group by brand (Claude / OpenAI) with a compact
          sub-switcher inside; each nested Tabs is its own Radix root, so the
          original per-client panels move in unchanged. */}
      <Tabs defaultValue="claude">
        <TabsList className="w-full justify-start overflow-x-auto">
          <TabsTrigger value="claude" title="Claude & Claude Code">
            <ClaudeIcon className="size-4" />
            <span className="max-sm:hidden">Claude</span>
          </TabsTrigger>
          <TabsTrigger value="openai" title="ChatGPT & Codex CLI">
            <OpenAiIcon className="size-4" />
            <span className="max-sm:hidden">OpenAI</span>
          </TabsTrigger>
          <TabsTrigger value="cursor" title="Cursor">
            <CursorIcon className="size-4" />
            <span className="max-sm:hidden">Cursor</span>
          </TabsTrigger>
          <TabsTrigger value="other" title="Other MCP clients">
            <Plug className="size-4" />
            <span className="max-sm:hidden">Other</span>
          </TabsTrigger>
        </TabsList>

        <TabsContent value="claude">
          <Tabs defaultValue="claude" className="gap-3">
            <TabsList className="h-8 p-[2px]">
              <TabsTrigger value="claude" className="px-2.5 text-xs">
                Claude app
              </TabsTrigger>
              <TabsTrigger value="claude-code" className="px-2.5 text-xs">
                Claude Code
              </TabsTrigger>
            </TabsList>

            <TabsContent value="claude" className="space-y-3">
              <Steps>
                <li>Settings → Connectors → Add custom connector.</li>
                <li>Paste the URL below and click Add.</li>
                <li>
                  Click Connect — the browser OAuth flow lets you pick which
                  teams and boards to share.
                </li>
              </Steps>
              <Snippet text={endpoint} copyLabel="Copy URL" />
              <Note>
                Connectors connect from Anthropic&apos;s cloud — a self-hosted
                instance must be reachable from the internet.
              </Note>
            </TabsContent>

            <TabsContent value="claude-code" className="space-y-3">
              <Steps>
                <li>Register the server:</li>
              </Steps>
              <Snippet
                text={buildMcpAddCommand(origin)}
                copyLabel="Copy command"
              />
              <Steps>
                <li value={2}>
                  Run <code>/mcp</code> inside claude to sign in (OAuth).
                </li>
              </Steps>
              <Note>
                API-key alternative: append{` `}
                <code>
                  --header &quot;Authorization: Bearer expu_...&quot;
                </code>{" "}
                to the add command — with a header set it won&apos;t fall back
                to OAuth.
              </Note>
            </TabsContent>
          </Tabs>
        </TabsContent>

        <TabsContent value="openai">
          <Tabs defaultValue="chatgpt" className="gap-3">
            <TabsList className="h-8 p-[2px]">
              <TabsTrigger value="chatgpt" className="px-2.5 text-xs">
                ChatGPT
              </TabsTrigger>
              <TabsTrigger value="codex" className="px-2.5 text-xs">
                Codex CLI
              </TabsTrigger>
            </TabsList>

            <TabsContent value="chatgpt" className="space-y-3">
              <Steps>
                <li>
                  On chatgpt.com, enable Settings → Apps &amp; Connectors →
                  Advanced → Developer mode (menu location may vary; Plus, Pro,
                  or Business).
                </li>
                <li>
                  Create a connector with the MCP server URL below and
                  Authentication: OAuth.
                </li>
                <li>In a chat, choose it via + → More.</li>
              </Steps>
              <Snippet text={endpoint} copyLabel="Copy URL" />
              <Note>
                ChatGPT connectors are OAuth-only — API-key headers are not
                supported.
              </Note>
            </TabsContent>

            <TabsContent value="codex" className="space-y-3">
              <Steps>
                <li>Add the server, then sign in via OAuth:</li>
              </Steps>
              <Snippet
                text={buildCodexMcpAddCommand(origin)}
                copyLabel="Copy commands"
              />
              <Note>
                Alternatively, configure the server in{" "}
                <code>~/.codex/config.toml</code>
                {` `}
                with <code>bearer_token_env_var</code> pointing at an env var
                holding a personal <code>expu_</code> API key.
              </Note>
            </TabsContent>
          </Tabs>
        </TabsContent>

        <TabsContent value="cursor" className="space-y-3">
          <Steps>
            <li>
              Add this to <code>~/.cursor/mcp.json</code> (or a board&apos;s
              {` `}
              <code>.cursor/mcp.json</code>):
            </li>
          </Steps>
          <Snippet
            text={buildMcpServersConfig(origin)}
            copyLabel="Copy config"
          />
          <Steps>
            <li value={2}>
              Hit Connect / login in Cursor&apos;s MCP list to complete the
              OAuth flow.
            </li>
          </Steps>
          <Note>
            Optional for headless use: add{` `}
            <code>
              &quot;headers&quot;: {`{"Authorization": "Bearer expu_..."}`}
            </code>
            {` `}
            to the server entry.
          </Note>
        </TabsContent>

        <TabsContent value="other" className="space-y-3">
          <Steps>
            <li>
              Most MCP clients accept an <code>mcpServers</code> config:
            </li>
          </Steps>
          <Snippet
            text={buildMcpServersConfig(origin)}
            copyLabel="Copy config"
          />
          <Note>
            Clients that only speak stdio can bridge via{` `}
            <code>{buildMcpRemoteBridgeCommand(origin)}</code>; VS Code uses a
            top-level <code>servers</code> key with{` `}
            <code>&quot;type&quot;: &quot;http&quot;</code>.
          </Note>
          <Note>
            <a
              href={docsUrl(`mcp`)}
              target="_blank"
              rel="noreferrer"
              className="underline underline-offset-2 hover:text-foreground"
            >
              Full setup guides in the docs
            </a>
          </Note>
        </TabsContent>
      </Tabs>
      <Note>
        Signing in via OAuth lets you scope access per team/board; personal API
        keys (Bearer expu_…) work for headless use.
      </Note>
    </div>
  )
}
