/* NEW-15: /talk/licences/. Source: docs/third-party-licences.md
   ("exponentialTalk: one voice engine on the GPU") and the crates' Cargo.toml. */
import { DocsCallout, DocsLayout, DocsSection, type DocsSection as DocsSectionType } from "@exp/site-shell"
import { DocsTable } from "../components/Content"
import type { PageProps } from "../lib/routes"
import { LINKS } from "../lib/site"
import { TALK_DOCS_NAV } from "./nav"

const SECTIONS: DocsSectionType[] = [
  { id: `table`, num: `01`, label: `What and how` },
  { id: `parakeet`, num: `02`, label: `Parakeet` },
  { id: `watermark`, num: `03`, label: `Watermark` },
]

const SHIPPED = `in the app`
const FETCHED = `your machine downloads it`

const ROWS: [string, string, string][] = [
  [`eTalk's crates (etalk, etalk-engine, etalk-crispasr, etalk-codec, etalk-wakeword, etalk-gpui)`, `Apache-2.0`, SHIPPED],
    [`cpal (audio, macOS and Windows)`, `Apache-2.0`, SHIPPED],
  [`global-hotkey (shortcut, macOS and Windows)`, `MIT or Apache-2.0`, SHIPPED],
  [`tray-icon (tray, macOS and Windows)`, `MIT or Apache-2.0`, SHIPPED],
  [`ashpd (shortcut portal, Linux)`, `MIT`, SHIPPED],
  [`zbus (shortcut portal and tray, Linux)`, `MIT or Apache-2.0`, SHIPPED],
  [`CrispASR 0.8.45 runtime and ggml, macOS`, `MIT`, SHIPPED],
  [`CrispASR 0.8.45 runtime and ggml, Linux and Windows (GPU and CPU-only builds)`, `MIT`, FETCHED],
  [`NVIDIA CUDA runtime and cuBLAS, Linux`, `NVIDIA CUDA Toolkit EULA`, FETCHED],
  [`libgomp, libgfortran (in the Linux runtime)`, `GPL-3.0+ with the GCC Runtime Library Exception`, FETCHED],
  [`OpenBLAS (Linux CPU-only runtime)`, `BSD-3-Clause`, FETCHED],
  [`libquadmath (Linux CPU-only runtime)`, `LGPL-2.1`, FETCHED],
  [`Qwen3-TTS 12 Hz 0.6B Base and its codec`, `Apache-2.0`, FETCHED],
  [`Parakeet TDT 0.6B v3 (NVIDIA)`, `CC BY 4.0`, FETCHED],
  [`Silero VAD v6.2`, `MIT`, FETCHED],
]

export default function TalkLicences({ path }: PageProps) {
  return (
    <>
      <section className="docs-hero content-hero">
        <div className="shell docs-hero-content">
          <h1>Licences</h1>
        </div>
      </section>
      <DocsLayout nav={TALK_DOCS_NAV} title="eTalk" sections={SECTIONS} currentPath={path}>
        <DocsSection id="table" num="01" label="What and how">
          <h2>What and how</h2>
          <DocsTable head={[`Component`, `Licence`, `How you get it`]} rows={ROWS} />
          <p>
            Downloads come from the publishers&apos; own servers, pinned by sha256. The only GPL code is the GCC runtime, under its
            runtime exception; CrispASR&apos;s espeak-ng path is off. The app&apos;s notices carry CrispASR&apos;s licence and third-party notices. Details:{` `}
            <a href={LINKS.source(`docs/third-party-licences.md`)}>docs/third-party-licences.md</a>.
          </p>
        </DocsSection>

        <DocsSection id="parakeet" num="02" label="Parakeet">
          <h2>Parakeet</h2>
          <DocsCallout kind="note" title="Attribution">
            <p>
              Speech recognition uses Parakeet TDT 0.6B v3 by NVIDIA, licensed under CC BY 4.0. Converted to GGUF and quantised to
              Q8_0 by cstr. We change nothing and redistribute nothing.
            </p>
          </DocsCallout>
        </DocsSection>

        <DocsSection id="watermark" num="03" label="Watermark">
          <h2>Watermark</h2>
          <p>
            CrispASR marks every clip it speaks with an inaudible AI-content watermark (EU AI Act Art. 50(2)). It stays on. The
            built-in clips carry it too.
          </p>
        </DocsSection>
      </DocsLayout>
    </>
  )
}
