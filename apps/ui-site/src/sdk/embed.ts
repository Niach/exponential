/* The component page's "how to embed" tabs: the current surface as JSONL,
   fed through each SDK's MemoryTransport into its host and painted by its
   HostSurface (the same API the guides compile in CI, guides/*). Pure
   strings, no SDK import. */
import type { Message } from "./a2ui"

export type EmbedTarget = `react` | `swiftui` | `compose` | `gpui`

export const EMBED_TARGETS: { id: EmbedTarget; label: string; language: string; guide: string }[] = [
  { id: `react`, label: `React`, language: `tsx`, guide: `/guides/react/` },
  { id: `swiftui`, label: `SwiftUI`, language: `swift`, guide: `/guides/swiftui/` },
  { id: `compose`, label: `Compose`, language: `kotlin`, guide: `/guides/compose/` },
  { id: `gpui`, label: `gpui`, language: `rust`, guide: `/guides/gpui/` },
]

export interface EmbedOptions {
  messages: readonly Message[]
  surfaceId: string
  theme: string
  mode: `light` | `dark`
  /** The studio's Direction switch: React takes it as a prop; the native
   *  hosts follow the device locale, so their code says so. */
  direction?: `ltr` | `rtl`
  /** The studio's Width preset (px), as a frame on the surface. */
  width?: number
}

const NATIVE_RTL = `RTL follows the device locale (an RTL language).`

/** One message per line, as a JSONL stream carries them. */
export const jsonl = (messages: readonly Message[]) => messages.map((m) => JSON.stringify(m)).join(`\n`)

const cap = (s: string) => s.charAt(0).toUpperCase() + s.slice(1)

/** The fewest `#`s a raw string needs so no `seq` + hashes occurs inside
 *  (the closing delimiter; Swift's `\#` escape too). */
function rawHashes(text: string, ...seqs: string[]): string {
  let n = 1
  while (seqs.some((seq) => text.includes(`${seq}${`#`.repeat(n)}`))) n++
  return `#`.repeat(n)
}

export function embedCode(target: EmbedTarget, { messages, surfaceId, theme, mode, direction = `ltr`, width }: EmbedOptions): string {
  const lines = jsonl(messages)
  const rtl = direction === `rtl`
  switch (target) {
    case `react`: {
      const body = lines.replace(/\\/g, () => `\\\\`).replace(/`/g, () => `\\\``).replace(/\$\{/g, () => `\\\${`)
      return `import { ExponentialHost, MemoryTransport } from "@exponential-at/ui"
import { HostSurface } from "@exponential-at/ui-react"

const SURFACE = \`${body}\`

const transport = new MemoryTransport()
const host = new ExponentialHost({ transport })
transport.feedJsonl(SURFACE)
host.connect()

export function Preview() {
  return <HostSurface host={host} surfaceId="${surfaceId}" theme="${theme}" mode="${mode}"${rtl ? ` direction="rtl"` : ``}${width ? ` width={${width}}` : ``} />
}`
    }
    case `swiftui`: {
      const h = rawHashes(lines, `"""`, `\\`)
      return `import SwiftUI
import ExponentialUI

let surface = ${h}"""
${lines}
"""${h}

struct Preview: View {
    @State private var host: ExponentialHost = {
        let transport = MemoryTransport()
        transport.feedJsonl(surface)
        return ExponentialHost(HostOptions(transport: transport, theme: ThemeHandle.builtin("${theme}"), mode: .${mode}))
    }()

    var body: some View {
        ScrollView {
${rtl ? `            // ${NATIVE_RTL}
` : ``}            HostSurface(host: host, surfaceId: "${surfaceId}") { ProgressView() }
${width ? `                .frame(width: ${width})
` : ``}                .padding(16)
        }
        .task { host.connect() }
    }
}`
    }
    case `compose`: {
      const body = lines.replace(/\$/g, () => `\${'$'}`)
      return `${width ? `import androidx.compose.foundation.layout.width
` : ``}import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.remember
${width ? `import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
` : ``}import at.exponential.ui.host.ExponentialHost
import at.exponential.ui.host.HostOptions
import at.exponential.ui.host.HostSurface
import at.exponential.ui.host.MemoryTransport
import at.exponential.ui.theme.Mode
import at.exponential.ui.theme.ThemeHandle

private val SURFACE = """
${body}
"""

@Composable
fun Preview() {
    val host = remember {
        val transport = MemoryTransport().apply { feedJsonl(SURFACE) }
        ExponentialHost(HostOptions(transport = transport, theme = ThemeHandle.builtin("${theme}"), mode = Mode.${cap(mode)}))
    }
    DisposableEffect(host) {
        host.connect()
        onDispose { host.dispose() }
    }
${rtl ? `    // ${NATIVE_RTL}
` : ``}    HostSurface(host, "${surfaceId}"${width ? `, Modifier.width(${width}.dp)` : ``})
}`
    }
    case `gpui`: {
      const h = rawHashes(lines, `"`)
      return `use exponential_ui::theme::Mode;
use exponential_ui::themes::builtin_theme;
use exponential_ui_gpui::runtime::{ExponentialHost, HostOptions};
use exponential_ui_gpui::transport::MemoryTransport;
use gpui::{prelude::*, App, Entity};

const SURFACE: &str = r${h}"
${lines}
"${h};

/// Render it with \`div()${width ? `.w(px(${width}.))` : ``}.children(host.read(cx).surface("${surfaceId}"))\`.${rtl ? `
/// ${NATIVE_RTL}` : ``}
pub fn preview_host(cx: &mut App) -> Entity<ExponentialHost> {
    let transport = MemoryTransport::new();
    transport.feed_jsonl(SURFACE);
    let options = HostOptions { transport: Some(Box::new(transport)), theme: builtin_theme("${theme}"), mode: Mode::${cap(mode)}, ..Default::default() };
    let host = cx.new(|cx| ExponentialHost::new(options, cx));
    host.update(cx, |host, cx| host.connect(cx));
    host
}`
    }
  }
}
