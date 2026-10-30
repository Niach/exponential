/* NEW-15: exponential.at/talk/, the voice library's front page. The
   overlay is the real push-to-talk capsule (src/ui-demos/VoiceOverlay.tsx,
   VoiceWave from @exp/ui); every feature is marked beta. */
import { TalkOverlay } from "../../../src/components/TalkOverlay"
import { StatusChip, type Status } from "../../../src/components/blog/StatusChip"
import { CardGrid, DocsTable, IcArrow } from "../components/Content"
import type { PageProps } from "../lib/routes"
import { talkGuidePath } from "./nav"

/* The owner's four parts, all beta since the 2026-10-27 release (calls and
   the wake word joined push to talk and spoken replies), nothing more about
   the future. The engines ride in the What column. */
const PARTS: { part: string; what: string; status: Status; label: string }[] = [
  { part: `Push to talk`, what: `Hold a shortcut on your desktop. Silero VAD and Parakeet through CrispASR`, status: `in-progress`, label: `beta` },
  { part: `Spoken replies`, what: `English and German. Qwen3-TTS, cloned from a clip of your voice`, status: `in-progress`, label: `beta` },
  { part: `Call your agent`, what: `From your phone, the web or the desktop app`, status: `in-progress`, label: `beta` },
  { part: `Wake word`, what: `“Hey Triage”, instead of holding the shortcut`, status: `in-progress`, label: `beta` },
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
            <TalkOverlay />
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
