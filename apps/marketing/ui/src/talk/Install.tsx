/* NEW-15: /talk/install/, turning Talk on (Settings → Voice, fixtures/voice.json). */
import { DocsCallout, DocsLayout, DocsSection, type DocsSection as DocsSectionType } from "@exp/site-shell"
import { DocsTable } from "../components/Content"
import type { PageProps } from "../lib/routes"
import { TALK_DOCS_NAV, talkGuidePath } from "./nav"

const SECTIONS: DocsSectionType[] = [
  { id: `settings`, num: `01`, label: `Settings → Voice` },
  { id: `download`, num: `02`, label: `The download` },
  { id: `gpu`, num: `03`, label: `GPU` },
  { id: `microphone`, num: `04`, label: `Microphone` },
  { id: `ptt`, num: `05`, label: `Push to talk` },
  { id: `tray`, num: `06`, label: `The tray` },
]

export default function Install({ path }: PageProps) {
  return (
    <>
      <section className="docs-hero content-hero">
        <div className="shell docs-hero-content">
          <h1>Install</h1>
        </div>
      </section>
      <DocsLayout nav={TALK_DOCS_NAV} title="Talk" sections={SECTIONS} currentPath={path}>
        <DocsSection id="settings" num="01" label="Settings → Voice">
          <h2>Settings → Voice</h2>
          <p>
            Talk is part of the Exponential desktop app. There is nothing else to install. Open Settings → Voice on the web, the
            desktop app or your phone. The settings are yours, not your team&apos;s.
          </p>
          <DocsTable
            head={[`Row`, `What it does`]}
            rows={[
              [<strong key="r">Voice device</strong>, `The desktop that listens and speaks. Only your own desktops are listed.`],
              [<strong key="r">Agent</strong>, `The agent you talk to. Your words go into your chat with it.`],
              [<strong key="r">Speak replies</strong>, `Read the agent's replies aloud on the voice device.`],
              [<strong key="r">Language</strong>, `English or Deutsch. Replies are spoken, and speech is heard, in this language.`],
              [<strong key="r">Voice file, Transcript</strong>, <>A clip of your voice and what it says. Empty = a built-in voice. See <a href={talkGuidePath(`your-voice`)}>Your voice</a>.</>],
            ]}
          />
          <p>Every switch stays off until both a voice device and an agent are picked.</p>
        </DocsSection>
        <DocsSection id="download" num="02" label="The download">
          <h2>The download</h2>
          <p>
            Turn on Speak replies and the voice device downloads the voice. The row shows the progress. Every file is pinned and
            checked against its sha256 before it is loaded. A failed download retries later on its own.
          </p>
          <DocsTable
            head={[``, `Linux`, `macOS`, `Windows`]}
            rows={[
              [`Speech out (first use)`, `~2.97 GB`, `~2.19 GB`, `~2.28 GB`],
              [`Speech in (first push to talk)`, `+ 675 MB`, `+ 675 MB`, `+ 675 MB`],
            ]}
          />
          <p>
            On macOS the runtime ships inside the app, so only the models download. The first push to talk answers
            &ldquo;Downloading speech recognition…&rdquo; while its models arrive.
          </p>
        </DocsSection>
        <DocsSection id="gpu" num="03" label="GPU">
          <h2>GPU</h2>
          <DocsTable
            head={[``, `Needs`]}
            rows={[
              [`Linux`, `x86_64. NVIDIA GPU with its driver (CUDA).`],
              [`macOS`, `Apple silicon (Metal).`],
              [`Windows`, `x86_64. A Vulkan driver.`],
            ]}
          />
          <p>
            Speech out needs a GPU with at least 4 GB that passes a speed test on one sentence. Without one, replies are shown,
            not spoken. A GPU that fails the test is tried again after 30 days or a driver change.
          </p>
          <p>
            Speech in also runs on the CPU, slower and without words while you speak. On Windows the CPU path needs the Visual C++
            runtime.
          </p>
        </DocsSection>
        <DocsSection id="microphone" num="04" label="Microphone">
          <h2>Microphone</h2>
          <p>
            The microphone opens only while you hold the shortcut. Linux reads it through PipeWire or PulseAudio. Windows uses the
            default input device.
          </p>
          <DocsCallout kind="note" title="macOS">
            <p>
              The first push to talk asks for the microphone. If you said no, allow Exponential in System Settings → Privacy &amp;
              Security → Microphone. Until then the overlay reads &ldquo;Microphone not allowed&rdquo;.
            </p>
          </DocsCallout>
        </DocsSection>
        <DocsSection id="ptt" num="05" label="Push to talk">
          <h2>Push to talk</h2>
          <p>
            Beta, desktop only. With a voice device picked, a second group appears: On {`{device}`} (tray). Turn on Push to talk and
            pick a Shortcut. The default is Ctrl + Space.
          </p>
          <p>
            Hold the shortcut, speak, let go. Not every desktop lets an app grab every key:{` `}
            <a href={talkGuidePath(`push-to-talk`)}>Push-to-talk shortcuts</a> lists what works where.
          </p>
        </DocsSection>
        <DocsSection id="tray" num="06" label="The tray">
          <h2>The tray</h2>
          <p>
            On the voice device the desktop app adds a tray icon. Its menu has Speak replies, Push to talk, Mute microphone and Voice
            settings. With the tray there, closing the last window keeps the app running, so the voice stays loaded.
          </p>
          <p>On GNOME the tray needs the AppIndicator extension.</p>
        </DocsSection>
      </DocsLayout>
    </>
  )
}
