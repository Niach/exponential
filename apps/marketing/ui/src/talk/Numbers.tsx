/* NEW-15: /talk/numbers/. Sources: docs/talk-spike.md ("Speech in through
   CrispASR"), crates/etalk-engine (MIN_GPU_BYTES, MAX_WARMUP_RTF),
   docs/third-party-licences.md (download sizes), NEW-60 (the targets). */
import { DocsLayout, DocsSection, type DocsSection as DocsSectionType } from "@exp/site-shell"
import { DocsTable } from "../components/Content"
import type { PageProps } from "../lib/routes"
import { LINKS } from "../lib/site"
import { TALK_DOCS_NAV } from "./nav"

const SECTIONS: DocsSectionType[] = [
  { id: `in`, num: `01`, label: `Speech in` },
  { id: `out`, num: `02`, label: `Speech out` },
  { id: `downloads`, num: `03`, label: `Downloads` },
  { id: `targets`, num: `04`, label: `Targets` },
]

export default function TalkNumbers({ path }: PageProps) {
  return (
    <>
      <section className="docs-hero content-hero">
        <div className="shell docs-hero-content">
          <h1>Numbers</h1>
        </div>
      </section>
      <DocsLayout nav={TALK_DOCS_NAV} title="eTalk" sections={SECTIONS} currentPath={path}>
        <DocsSection id="in" num="01" label="Speech in">
          <h2>Speech in</h2>
          <p>
            Silero VAD and Parakeet TDT 0.6B v3 (Q8_0), on an RTX 4090 and an i9. Ten synthetic English clips, run through the
            worker&apos;s own pipeline. Word error rate counts against the spoken form, so &ldquo;New 54&rdquo; for &ldquo;NEW
            fifty four&rdquo; is an error.
          </p>
          <DocsTable
            head={[``, `GPU (CUDA)`, `CPU, 8 threads`]}
            rows={[
              [`Text after you let go`, `10–41 ms`, `102–450 ms`],
              [`Word error rate`, `14.9 %`, `12.6 %`],
              [`Words while you speak`, `every 300 ms`, `no`],
              [`Model load`, `0.72 s`, `0.48 s`],
              [`VRAM`, `1,246 MiB`, `none`],
            ]}
          />
          <p>
            Words while you speak need 1.6 GB of VRAM free. Our German and English built-in clips came through word for word on
            both. No human recordings yet. Full notes: <a href={LINKS.source(`docs/talk-spike.md`)}>docs/talk-spike.md</a>.
          </p>
        </DocsSection>

        <DocsSection id="out" num="02" label="Speech out">
          <h2>Speech out</h2>
          <p>Qwen3-TTS 0.6B Base (F16) and its codec, on the GPU only.</p>
          <DocsTable
            head={[``, ``]}
            rows={[
              [`VRAM, peak`, `3.7 GB`],
              [`GPU needed`, `4 GB or more`],
              [`Real-time factor, RTX 4090`, `0.13–0.27`],
              [`Real-time factor, CPU`, `~2.0, so no speech on the CPU`],
            ]}
          />
          <p>
            A real-time factor below 1 is faster than real time. A GPU must reach 0.6 in a warm-up sentence. Time to first audio is
            not measured on this model yet; the earlier CustomVoice Q8_0 model took 0.3–0.5 s for a short sentence.
          </p>
        </DocsSection>

        <DocsSection id="downloads" num="03" label="Downloads">
          <h2>Downloads</h2>
          <DocsTable
            head={[``, `Linux`, `macOS`, `Windows`]}
            rows={[
              [`Speech out, first use`, `~2.97 GB`, `~2.19 GB`, `~2.28 GB`],
              [`Speech in models`, `675 MB`, `675 MB`, `675 MB`],
              [`CPU-only runtime (no GPU driver)`, `22.7 MB`, `—`, `58.4 MB`],
            ]}
          />
          <p>
            Linux: runtime 196 MB, cuBLAS 581 MB, model 1.83 GB, codec 358 MB. macOS: the runtime ships in the app, so only the
            models download.
          </p>
        </DocsSection>

        <DocsSection id="targets" num="04" label="Targets">
          <h2>Targets</h2>
          <p>What we aim for. Targets, not measurements.</p>
          <DocsTable
            head={[``, `Target`, `Today`]}
            rows={[
              [`First words while you speak`, `< 300 ms`, `not measured`],
              [`Final text`, `< 500 ms`, `10–41 ms GPU, 102–450 ms CPU`],
              [`First spoken audio`, `< 400 ms`, `not measured on this model`],
              [`Local models, VRAM`, `≤ 4 GB`, `3.7 GB speech out + 1,246 MiB speech in: over, together`],
            ]}
          />
        </DocsSection>
      </DocsLayout>
    </>
  )
}
