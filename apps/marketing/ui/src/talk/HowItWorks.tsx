/* NEW-15: /talk/how-it-works/, one docs page of the Talk section. */
import { DocsLayout, DocsSection, type DocsSection as DocsSectionType } from "@exp/site-shell"
import { DocsTable } from "../components/Content"
import type { PageProps } from "../lib/routes"
import { TALK_DOCS_NAV } from "./nav"

const SECTIONS: DocsSectionType[] = [
  { id: `device`, num: `01`, label: `The voice device` },
  { id: `in`, num: `02`, label: `Speech in` },
  { id: `out`, num: `03`, label: `Speech out` },
  { id: `ptt`, num: `04`, label: `Push to talk` },
  { id: `coming`, num: `05`, label: `Coming` },
]

export default function HowItWorks({ path }: PageProps) {
  return (
    <>
      <section className="docs-hero content-hero">
        <div className="shell docs-hero-content">
          <h1>How it works</h1>
        </div>
      </section>
      <DocsLayout nav={TALK_DOCS_NAV} title="Talk" sections={SECTIONS} currentPath={path}>
        <DocsSection id="device" num="01" label="The voice device">
          <h2>The voice device</h2>
          <p>
            One of your desktops listens and speaks for you. Pick it in Settings → Voice, with the agent it talks to. Phones and
            servers can&apos;t be the voice device.
          </p>
          <p>
            The desktop app runs a speech worker beside itself. Models download on first use, pinned and checked. Audio never
            leaves the machine.
          </p>
        </DocsSection>
        <DocsSection id="in" num="02" label="Speech in">
          <h2>Speech in</h2>
          <p>
            The microphone runs at 16 kHz. Silero VAD finds speech, Parakeet turns it into text. On a GPU you see the words while
            you speak (with 1.6 GB of VRAM free); on the CPU you get the text when you let go.
          </p>
          <DocsTable
            head={[``, `GPU (CUDA)`, `CPU, 8 threads`]}
            rows={[
              [`Text after you let go`, `10–41 ms`, `102–450 ms`],
              [`Words while you speak`, `every 300 ms`, `no`],
              [`VRAM`, `1,246 MiB`, `none`],
            ]}
          />
        </DocsSection>
        <DocsSection id="out" num="03" label="Speech out">
          <h2>Speech out</h2>
          <p>
            Beta. Qwen3-TTS reads the agent&apos;s replies in English or German, in your voice: give it an 8–15 s clip and what it
            says, or leave it empty for a built-in voice. No training, no upload. It needs a GPU with at least 4 GB; without one,
            replies are shown, not spoken.
          </p>
        </DocsSection>
        <DocsSection id="ptt" num="04" label="Push to talk">
          <h2>Push to talk</h2>
          <p>
            Beta, on the desktop app only. Hold the shortcut and speak. The overlay shows what it heard, then sends it to the agent as a plain message. Speak
            replies reads the answer aloud. The tray mutes the microphone.
          </p>
        </DocsSection>
        <DocsSection id="coming" num="05" label="Coming">
          <h2>Coming</h2>
          <p>
            Calling your agent from your phone, and a wake word. The wake word switch is already in Settings → Voice;
            nothing listens for it yet.
          </p>
        </DocsSection>
      </DocsLayout>
    </>
  )
}
