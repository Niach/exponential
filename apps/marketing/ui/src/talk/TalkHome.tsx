/* NEW-15: exponential.at/talk/, the voice library's front page. The
   overlay is the real push-to-talk capsule (src/ui-demos/VoiceOverlay.tsx,
   VoiceWave from @exp/ui); every claim is marked shipped, beta or coming. */
import { UiDemo } from "../../../src/components/UiDemo"
import { StatusChip, type Status } from "../../../src/components/blog/StatusChip"
import { CardGrid, DocsTable, IcArrow } from "../components/Content"
import type { PageProps } from "../lib/routes"
import { talkGuidePath } from "./nav"

const PARTS: { part: string; what: string; status: Status; label: string }[] = [
  { part: `Speech in`, what: `Silero VAD and Parakeet through CrispASR`, status: `shipped`, label: `shipped` },
  { part: `Speech out`, what: `Qwen3-TTS, cloned from a clip of your voice`, status: `shipped`, label: `shipped` },
  { part: `Push to talk`, what: `A global shortcut on your desktop`, status: `in-progress`, label: `beta` },
  { part: `Call your agent`, what: `From your phone or the web`, status: `planned`, label: `coming` },
  { part: `Wake word`, what: `“Hey Triage”`, status: `planned`, label: `coming` },
]

export default function TalkHome(_: PageProps) {
  return (
    <>
      <section className="home-hero">
        <div className="shell">
          <div className="home-hero-text">
            <h1 className="home-title">Talk to your agents</h1>
            <p className="home-sub">Speech in and out on your own computer, in English and German. Open source.</p>
            <div className="home-cta">
              <a className="btn btn-primary" href="/talk/install/">
                Install <IcArrow />
              </a>
              <a className="btn btn-ghost" href="/talk/how-it-works/">
                How it works
              </a>
            </div>
          </div>
          <div className="home-demo talk-demo">
            <UiDemo id="voice-overlay" />
          </div>
        </div>
      </section>

      <section className="home-section">
        <div className="shell">
          <h2 className="section-title">What works today</h2>
          <DocsTable
            head={[`Part`, `What`, ``]}
            rows={PARTS.map((p) => [<strong key="p">{p.part}</strong>, p.what, <StatusChip key="s" status={p.status}>{p.label}</StatusChip>])}
          />
        </div>
      </section>

      <section className="home-section">
        <div className="shell">
          <h2 className="section-title">Start here</h2>
          <CardGrid
            items={[
              { href: `/talk/how-it-works/`, title: `How it works`, desc: `The voice device, speech in, speech out.` },
              { href: talkGuidePath(`your-voice`), title: `Your voice`, desc: `Record a clip, write its transcript.` },
              { href: `/talk/numbers/`, title: `Numbers`, desc: `Latency, VRAM and accuracy, measured.` },
              { href: `/talk/api/`, title: `API`, desc: `The speech worker's requests and events.` },
            ]}
          />
        </div>
      </section>
    </>
  )
}
