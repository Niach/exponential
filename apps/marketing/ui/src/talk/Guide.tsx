/* NEW-15: /talk/guides/<slug>/, both Talk guides by the path's slug. Facts:
   crates/talk (custom_voice_file), crates/voice/src/builtin.rs,
   fixtures/voice.json (shortcutGrammar, shortcutSupportCases, pttCopy). */
import type { ReactNode } from "react"
import { DocsCallout, DocsCode, DocsLayout, DocsSection } from "@exp/site-shell"
import { DocShot } from "../../../src/components/DocShot"
import { DocsTable, IcArrow } from "../components/Content"
import type { PageProps } from "../lib/routes"
import { TALK_DOCS_NAV, TALK_GUIDES, talkGuidePath } from "./nav"

type Section = { id: string; label: string; body: ReactNode }

const SKILL = `/talk/skills/voice-reference-clip/SKILL.md`

const GUIDE_SECTIONS: Record<string, Section[]> = {
  "your-voice": [
    {
      id: `how`,
      label: `How it works`,
      body: (
        <>
          <p>
            Talk speaks in a voice cloned from ONE short clip, in English and in German. No training, no upload. The clip stays on
            your voice device. Settings → Voice stores only its path and its transcript.
          </p>
          <p>
            Leave the Voice file empty and each language uses its built-in clip. Cloning, like all speech out, runs only on a GPU.
          </p>
          <p>
            For an agent: <a href={SKILL}>the same guide as a skill file</a>.
          </p>
        </>
      ),
    },
    {
      id: `consent`,
      label: `Consent first`,
      body: (
        <p>
          Use your own voice, or a voice whose owner agreed. Do not clone anyone without consent.
        </p>
      ),
    },
    {
      id: `take`,
      label: `Pick the take`,
      body: (
        <ul>
          <li>8 to 15 seconds of one speaker, speaking calmly in the language you picked.</li>
          <li>Clean speech: no music, no background noise.</li>
        </ul>
      ),
    },
    {
      id: `cut`,
      label: `Cut it to a WAV`,
      body: (
        <>
          <p>With ffmpeg (start and length in seconds):</p>
          <DocsCode>{`mkdir -p ~/voice
ffmpeg -ss 12.5 -t 10 -i recording.m4a -ac 1 -ar 24000 -c:a pcm_s16le ~/voice/ref.wav`}</DocsCode>
          <ul>
            <li>Mono, 24 kHz, 16-bit PCM, like the built-in clips.</li>
            <li>Listen to it once, all the way through.</li>
          </ul>
        </>
      ),
    },
    {
      id: `transcript`,
      label: `Write the transcript`,
      body: (
        <p>
          Type exactly what the clip says, word for word, with normal punctuation.
        </p>
      ),
    },
    {
      id: `enter`,
      label: `Enter it`,
      body: (
        <>
          <p>In Settings → Voice, with your desktop as the voice device:</p>
          <DocShot view="settings-voice" platform="web" caption="Settings → Voice on the web: Language, Voice file and Transcript in the first group." />
          <DocsTable
            head={[`Row`, `Value`]}
            rows={[
              [<strong key="r">Language</strong>, `English or Deutsch`],
              [<strong key="r">Voice file</strong>, <>the WAV&apos;s full path on that desktop, e.g. <code>/home/you/voice/ref.wav</code></>],
              [<strong key="r">Transcript</strong>, `the text from the step before`],
            ]}
          />
          <DocsCallout kind="note" title="What the voice device accepts">
            <p>
              An absolute path to a local <code>.wav</code> of at most 10 MB, with a transcript. No URL, no <code>..</code>, on
              Windows no network share. Anything else is ignored and the built-in voice speaks.
            </p>
          </DocsCallout>
        </>
      ),
    },
  ],
  "push-to-talk": [
    {
      id: `where`,
      label: `What works where`,
      body: (
        <>
          <p>Push to talk is beta and runs on the voice device&apos;s desktop app only. The shortcut is held: press to listen, release to send.</p>
          <DocsTable
            head={[``, `Keys`, `Mouse buttons`]}
            rows={[
              [<strong key="o">Windows</strong>, `global`, `global`],
              [<strong key="o">Linux, X11</strong>, `global`, `global`],
              [<strong key="o">macOS</strong>, `global`, `no`],
              [<strong key="o">Linux, Wayland with the portal</strong>, `global, through the portal`, `no`],
              [<strong key="o">Linux, Wayland without it</strong>, `only while Exponential is focused`, `no`],
            ]}
          />
          <p>
            The portal is xdg GlobalShortcuts: KDE 5.27+, GNOME 48+ and Hyprland have it. The compositor may bind another key; the
            shortcut then shows its label. On macOS, Ctrl + Space is also Select previous input source.
          </p>
        </>
      ),
    },
    {
      id: `shortcuts`,
      label: `The shortcuts`,
      body: (
        <>
          <p>
            Record one in Settings → Voice on the voice device, under On {`{device}`} (tray): click the row and press the key or
            combination. A letter, digit or most other keys need Ctrl, Alt, Shift or Super with them; these may stand alone:
          </p>
          <DocsTable
            head={[`Binding`, ``]}
            rows={[
              [`F1 … F24`, ``],
              [`Insert, Pause, Scroll Lock, Print Screen`, ``],
              [`Mouse button 3 … 9`, `Windows, X11`],
            ]}
          />
        </>
      ),
    },
    {
      id: `fallback`,
      label: `When a key can't be grabbed`,
      body: (
        <>
          <p>
            It never fails silently. The tray item reads Push to talk · Ctrl + Space unavailable, and Settings → Voice says why
            under the Shortcut row:
          </p>
          <DocsTable
            head={[`Why`, `What it says`]}
            rows={[
              [`Another app holds it`, `Another app holds {shortcut}. It works while Exponential is focused.`],
              [`No portal`, `This desktop has no global shortcuts. {shortcut} works while Exponential is focused.`],
              [`The portal said no`, `The desktop didn't allow {shortcut}. It works while Exponential is focused.`],
              [`A mouse button`, `Mouse buttons can't be a shortcut here. Pick a key.`],
            ]}
          />
          <p>In every case but the mouse button, the key still works while an Exponential window is focused.</p>
        </>
      ),
    },
    {
      id: `hold`,
      label: `Holding it`,
      body: (
        <p>
          Holding the key stops any reply being spoken, unless the microphone is muted. Mute microphone in the tray keeps it closed;
          the overlay then reads Microphone muted, Unmute in the tray to talk, with an Unmute pill. A key held for 120 s ends as if you let go.
        </p>
      ),
    },
  ],
}

export default function TalkGuide({ path }: PageProps) {
  const slug = path.split(`/`).filter(Boolean)[2] ?? ``
  const index = TALK_GUIDES.findIndex((g) => g.slug === slug)
  const guide = TALK_GUIDES[index]
  const body = GUIDE_SECTIONS[slug]
  if (!guide || !body) return <div className="shell">Unknown guide.</div>

  const sections = body.map((s, i) => ({ ...s, num: String(i + 1).padStart(2, `0`) }))
  const next = TALK_GUIDES[index + 1]

  return (
    <>
      <section className="docs-hero content-hero">
        <div className="shell docs-hero-content">
          <h1>{guide.title}</h1>
        </div>
      </section>
      <DocsLayout nav={TALK_DOCS_NAV} title="Talk" sections={sections} currentPath={path}>
        {sections.map((s) => (
          <DocsSection key={s.id} id={s.id} num={s.num} label={s.label}>
            <h2>{s.label}</h2>
            {s.body}
          </DocsSection>
        ))}
        {next && (
          <a className="guide-next" href={talkGuidePath(next.slug)}>
            <span className="guide-next-label">Next guide</span>
            <span className="guide-next-title">
              {next.title} <IcArrow />
            </span>
          </a>
        )}
      </DocsLayout>
    </>
  )
}
