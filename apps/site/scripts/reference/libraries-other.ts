import type { ReferenceSource } from "../sync-reference.ts"

/* The modules Voice, P2P, Computer Use, MCP Proxy, Agents and Core: their
   reference pages, generated from the crate and package READMEs, the
   library table (docs/libraries.md) and Voice's licence sections
   (docs/third-party-licences.md). Hand-written pages next to them
   (Voice's api and numbers) have no markdown source in the repo. */

const CRATES = `apps/desktop/crates`

/** The markdown from the H2 `from` up to (not including) the next H2 `until`, or the end. */
export function section(from: string, until?: string): (markdown: string) => string {
  return (markdown) => {
    const start = markdown.indexOf(`\n## ${from}`)
    if (start < 0) throw new Error(`section "${from}" not found`)
    const end = until ? markdown.indexOf(`\n## ${until}`, start + 1) : -1
    if (until && end < 0) throw new Error(`section "${until}" not found`)
    return markdown.slice(start + 1, end < 0 ? undefined : end).trimEnd() + `\n`
  }
}

/** Drops the leading H2 of a section cut (the page title already says it). */
const untitled = (cut: (markdown: string) => string) => (markdown: string) => cut(markdown).replace(/^## .*\n+/, ``)

/** The pages any library's reference may list (slug → source). */
export const REFERENCE: ReferenceSource[] = [
  /* Shared: how to depend on a git-tag library. */
  {
    source: `docs/libraries.md`,
    slug: `docs/sdk/install`,
    title: `Install from a release tag`,
    description: `How to depend on Voice, Computer Use, MCP Proxy and Agents: a git dependency pinned to the module's release tag, the semver rules and what gpui consumers copy.`,
    transform: (md) => untitled(section(`Consuming from another repo`, `Migrating`))(`\n` + md),
  },

  /* Voice */
  {
    source: `${CRATES}/etalk/README.md`,
    slug: `docs/sdk/voice/reference`,
    title: `Voice reference`,
    description: `Voice's crates, its contract, hosting the speech worker and consuming the crates.`,
  },
  {
    source: `packages/etalk/README.md`,
    slug: `docs/sdk/voice/reference/contract`,
    title: `Contract (@exponential_app/etalk)`,
    description: `Voice's speech contract: voice settings, caller mode and meeting mode as TypeScript references plus the JSON fixtures every platform replays.`,
  },
  {
    source: `${CRATES}/etalk-gpui/README.md`,
    slug: `docs/sdk/voice/reference/gpui`,
    title: `etalk-gpui`,
    description: `Voice's UI for gpui apps: tray voice, the push-to-talk overlay, the call panel, the reply speaker and meeting mode behind the TalkHost sink.`,
  },
  {
    source: `${CRATES}/etalk-ffi/README.md`,
    slug: `docs/sdk/voice/reference/ffi`,
    title: `etalk-ffi (Swift and Kotlin)`,
    description: `The UniFFI facade over Voice's call machine and Opus codec for iOS, macOS and Android.`,
  },
  {
    source: `${CRATES}/etalk-wakeword/README.md`,
    slug: `docs/sdk/voice/reference/wakeword`,
    title: `etalk-wakeword`,
    description: `The wake word with a model of its own: an openWakeWord-compatible detector on tract.`,
  },
  {
    source: `docs/third-party-licences.md`,
    slug: `docs/sdk/voice/reference/licences`,
    title: `Licences`,
    description: `The licences of everything Voice downloads or ships: the CrispASR runtime, the CUDA libraries, Qwen3-TTS, Parakeet, Silero VAD and the wake word models.`,
    transform: (md) =>
      section(`exponentialTalk: one voice engine on the GPU`, `Exponential Desktop's bundled Xwayland`)(md)
        .replace(/^## exponentialTalk: one voice engine on the GPU.*$/m, `## The voice engine`)
        .replace(/^## Voice wake word models.*$/m, `## Wake word models`),
  },

  /* P2P */
  {
    source: `${CRATES}/enet-core/README.md`,
    slug: `docs/sdk/p2p/reference`,
    title: `P2P reference`,
    description: `enet-core: the node, identities and tickets, the channels, push encryption, the security model, targets and features.`,
  },
  {
    source: `${CRATES}/exponential-net-ffi/README.md`,
    slug: `docs/sdk/p2p/reference/ffi`,
    title: `exponential-net-ffi (Swift and Kotlin)`,
    description: `P2P for iOS and Android through UniFFI.`,
  },
  {
    source: `${CRATES}/exponential-net-wasm/README.md`,
    slug: `docs/sdk/p2p/reference/wasm`,
    title: `exponential-net-wasm (browser)`,
    description: `P2P in the browser through wasm-bindgen, relay only.`,
  },

  /* Computer Use */
  {
    source: `${CRATES}/epilot/README.md`,
    slug: `docs/sdk/computer-use/reference`,
    title: `Computer Use reference`,
    description: `epilot: plan, act, observe, the public API, the tools, the input lock, permissions per OS.`,
  },
  {
    source: `${CRATES}/epilot-screen/README.md`,
    slug: `docs/sdk/computer-use/reference/screen`,
    title: `epilot-screen`,
    description: `Screen memory: the last minutes of the screen as deduplicated frames on the device, and its get_screenshots tool.`,
  },

  /* MCP Proxy */
  {
    source: `${CRATES}/emcp/README.md`,
    slug: `docs/sdk/mcp-proxy/reference`,
    title: `MCP Proxy reference`,
    description: `emcp: the loopback server, the client, OAuth and the vault, the local hub, code mode, the public API and the dependency rule.`,
  },
  {
    source: `packages/emcp/README.md`,
    slug: `docs/sdk/mcp-proxy/reference/contract`,
    title: `Contract (@exp/emcp)`,
    description: `MCP Proxy's fixtures: exposes, access, audit, vault and upstream, with the TypeScript references that replay them.`,
  },

  /* Agents */
  {
    source: `${CRATES}/eagent/README.md`,
    slug: `docs/sdk/agents/reference`,
    title: `Agents reference`,
    description: `eagent: the rules, the seams a host implements, what stays in Exponential, the extraction order and using it without a server.`,
  },
  {
    source: `packages/eagent/README.md`,
    slug: `docs/sdk/agents/reference/orchestration`,
    title: `Orchestration spec (@exp/eagent)`,
    description: `The session tree, the bus, wake-ups, end reports, caps and the watchdog as fixtures and a pure TypeScript reducer.`,
  },
  {
    source: `${CRATES}/eagent-hub-core/README.md`,
    slug: `docs/sdk/agents/reference/hub-core`,
    title: `eagent-hub-core`,
    description: `The orchestration reducer in Rust, replaying the same fixtures as the TypeScript reference.`,
  },
  {
    source: `${CRATES}/eagent-hub-core-wasm/README.md`,
    slug: `docs/sdk/agents/reference/hub-core-wasm`,
    title: `eagent-hub-core-wasm`,
    description: `The orchestration reducer as WebAssembly.`,
  },
  {
    source: `docs/libraries.md`,
    slug: `docs/sdk/agents/reference/migrating`,
    title: `Migrating from agent-runtime`,
    description: `Moving from exponential-agent-runtime to eagent: the one-release shim and the new tag.`,
    transform: (md) => untitled(section(`Migrating`, `Rules for the extractions`))(`\n` + md),
  },

  /* Core */
  {
    source: `${CRATES}/ecore/README.md`,
    slug: `docs/sdk/core/reference`,
    title: `Core reference`,
    description: `ecore: ids, grants, capabilities, the delivery verdict, the error shape and the event envelope; types and pure rules only.`,
  },
  {
    source: `packages/ecore/README.md`,
    slug: `docs/sdk/core/reference/contract`,
    title: `Contract (@exp/ecore)`,
    description: `contract.json, the one source of Core's types, and what it generates in Rust, TypeScript, Swift and Kotlin.`,
  },

  /* Code */
  {
    source: `${CRATES}/ecode-core/README.md`,
    slug: `docs/sdk/code/reference`,
    title: `Code reference`,
    description: `ecode-core: the forge's PR-tree model, checks, the pipeline file, runner scheduling and remotes; pure, server-free.`,
  },
  {
    source: `packages/ecode/README.md`,
    slug: `docs/sdk/code/reference/contract`,
    title: `Contract (@exp/ecode)`,
    description: `eCode's fixtures (forge, checks, pipeline, runner, remote), the provider capabilities and the traits a host implements.`,
  },
]
