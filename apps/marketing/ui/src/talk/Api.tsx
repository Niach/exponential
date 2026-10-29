/* NEW-15: /talk/api/, the speech worker's wire (crates/etalk/src/lib.rs),
   CrispASR's types (crates/etalk-crispasr) and the Settings → Voice fields
   (fixtures/voice.json, user_voice_settings). */
import { DocsCallout, DocsCode, DocsLayout, DocsSection, type DocsSection as DocsSectionType } from "@exp/site-shell"
import { DocsTable } from "../components/Content"
import type { PageProps } from "../lib/routes"
import { LINKS } from "../lib/site"
import { TALK_DOCS_NAV } from "./nav"

const SECTIONS: DocsSectionType[] = [
  { id: `crates`, num: `01`, label: `The crates` },
  { id: `worker`, num: `02`, label: `The worker` },
  { id: `requests`, num: `03`, label: `Requests` },
  { id: `events`, num: `04`, label: `Events` },
  { id: `failures`, num: `05`, label: `Listen failures` },
  { id: `crispasr`, num: `06`, label: `CrispASR` },
  { id: `settings`, num: `07`, label: `Settings fields` },
  { id: `examples`, num: `08`, label: `Try it` },
]

const crate = (name: string) => <a href={LINKS.source(`apps/desktop/crates/${name}`)}>{name}</a>

export default function Api({ path }: PageProps) {
  return (
    <>
      <section className="docs-hero content-hero">
        <div className="shell docs-hero-content">
          <h1>API</h1>
        </div>
      </section>
      <DocsLayout nav={TALK_DOCS_NAV} title="Talk" sections={SECTIONS} currentPath={path}>
        <DocsSection id="crates" num="01" label="The crates">
          <h2>The crates</h2>
          <DocsTable
            head={[`Crate`, `What`]}
            rows={[
              [crate(`talk`), `The host side: SpeechClient and the wire types.`],
              [crate(`voice`), `The worker: microphone, VAD, transcriber, speaker.`],
              [crate(`crispasr`), `The CrispASR runtime and the pinned models.`],
            ]}
          />
          <DocsCallout kind="note">
            <p>
              The crates live in the Exponential repo and are <code>publish = false</code>. There is no crate on a registry yet.
            </p>
          </DocsCallout>
        </DocsSection>

        <DocsSection id="worker" num="02" label="The worker">
          <h2>The worker</h2>
          <p>
            The desktop app runs the speech in a second process: its own executable, started again with a marker as the first
            argument.
          </p>
          <DocsCode>{`<exe> __speech-worker --data-dir <dir>`}</DocsCode>
          <p>
            Requests go to its stdin, events come back on its stdout: one JSON object per line, tagged by <code>&quot;t&quot;</code>,
            camelCase. The worker logs to stderr. When the app dies, stdin closes and the worker exits. Models live under{` `}
            <code>{`{data-dir}/talk/`}</code>.
          </p>
          <DocsCode language="jsonl">{`{"t":"listen","id":"u1","language":"de"}
{"t":"level","id":"u1","level":0.5,"bands":[0.0,0.25,0.5,0.75,1.0,0.5,0.25,0.0]}
{"t":"endListen","id":"u1"}
{"t":"heard","id":"u1","text":"Move NEW-78 to done."}`}</DocsCode>
          <p>
            <code>SpeechClient</code> starts the worker on the first request and again after it dies. The wire is{` `}
            <code>PROTOCOL</code> 3; the worker says so in <code>ready</code>.
          </p>
        </DocsSection>

        <DocsSection id="requests" num="03" label="Requests">
          <h2>Requests</h2>
          <DocsTable
            head={[`t`, `Fields`, `Does`]}
            rows={[
              [<code key="t">prepare</code>, <code key="f">voice</code>, `Download and load the voice. Cheap to repeat.`],
              [<code key="t">speak</code>, <code key="f">id, text</code>, `Say the text. Replaces anything playing.`],
              [<code key="t">stop</code>, ``, `End what is being said.`],
              [<code key="t">listen</code>, <code key="f">id, language</code>, `Open the microphone for one utterance. Stops any speech.`],
              [<code key="t">endListen</code>, <code key="f">id</code>, `The key was released: close the microphone, transcribe.`],
              [<code key="t">cancelListen</code>, <code key="f">id</code>, `Drop the utterance. No answer.`],
            ]}
          />
          <p>
            <code>voice</code> is a <code>VoiceChoice</code>: <code>language</code> (<code>en</code> | <code>de</code>),{` `}
            <code>file</code> (empty = the built-in clip) and <code>transcript</code>. A <code>speak</code> that arrives while a
            listen runs is answered <code>finished</code> with <code>stopped: true</code>, unplayed.
          </p>
        </DocsSection>

        <DocsSection id="events" num="04" label="Events">
          <h2>Events</h2>
          <DocsTable
            head={[`t`, `Fields`, `Means`]}
            rows={[
              [<code key="t">ready</code>, <code key="f">protocol</code>, `The worker is up.`],
              [<code key="t">downloading</code>, <code key="f">received, total</code>, `A download is running (total 0 = unknown).`],
              [<code key="t">downloadFailed</code>, <code key="f">error</code>, `The download or its check failed.`],
              [<code key="t">modelReady</code>, ``, `The voice is on disk, checked and loaded.`],
              [<code key="t">voices</code>, <code key="f">voices</code>, `The languages this machine can speak. Empty = no usable GPU.`],
              [<code key="t">started</code>, <code key="f">id</code>, `The first audio of id is playing.`],
              [<code key="t">finished</code>, <code key="f">id, stopped</code>, `id ended.`],
              [<code key="t">failed</code>, <code key="f">id, error</code>, `id could not be spoken.`],
              [<code key="t">level</code>, <code key="f">id, level, bands</code>, `The microphone level while listening.`],
              [<code key="t">partial</code>, <code key="f">id, text</code>, `What was heard so far. GPU only.`],
              [<code key="t">heard</code>, <code key="f">id, text</code>, `The final transcript.`],
              [<code key="t">empty</code>, <code key="f">id</code>, `Nobody spoke.`],
              [<code key="t">listenFailed</code>, <code key="f">id, reason, detail</code>, `Could not listen or transcribe.`],
            ]}
          />
          <p>
            <code>level</code> comes every 32 ms (one Silero frame, 512 samples at 16 kHz): <code>level</code> and 8{` `}
            <code>bands</code> from low to high pitch, each 0 to 1, already smoothed for drawing. <code>partial</code> comes every
            300 ms, only when the transcriber runs on the GPU. The host also sees <code>exited</code> when the worker ends; the
            worker never sends it.
          </p>
        </DocsSection>

        <DocsSection id="failures" num="05" label="Listen failures">
          <h2>Listen failures</h2>
          <DocsTable
            head={[`reason`, `Means`]}
            rows={[
              [<code key="r">no_microphone</code>, `No input device, or it would not open.`],
              [<code key="r">not_granted</code>, `The OS refused the microphone (macOS).`],
              [<code key="r">downloading</code>, `The speech model is still downloading.`],
              [<code key="r">unavailable</code>, `The model or runtime failed to download, check or load.`],
              [<code key="r">failed</code>, `Transcribing failed.`],
            ]}
          />
        </DocsSection>

        <DocsSection id="crispasr" num="06" label="CrispASR">
          <h2>CrispASR</h2>
          <p>
            The worker loads CrispASR 0.8.45 at runtime, after re-hashing every file. One process loads one build: the GPU build
            where the driver exists, else the CPU build.
          </p>
          <DocsTable
            head={[`Type`, `What`]}
            rows={[
              [<code key="t">Runtime</code>, `The loaded library, its GPUs, and the sessions below.`],
              [<code key="t">TtsSession</code>, <><code>set_voice(wav, ref_text, language)</code>, <code>synthesize(text)</code>: Qwen3-TTS.</>],
              [<code key="t">AsrSession</code>, <><code>transcribe(pcm, language)</code>: Parakeet.</>],
              [<code key="t">VadStream</code>, <><code>push(samples)</code>: Silero speech probability per 32 ms frame.</>],
            ]}
          />
        </DocsSection>

        <DocsSection id="settings" num="07" label="Settings fields">
          <h2>Settings fields</h2>
          <p>One row per member, read and written through Exponential&apos;s API and handed to your devices.</p>
          <DocsTable
            head={[`Field`, `Type`, `Default`]}
            rows={[
              [<code key="f">voiceDeviceId</code>, `one of your desktops, or null`, `null`],
              [<code key="f">agentId</code>, `an agent of one of your teams, or null`, `null`],
              [<code key="f">pttShortcut</code>, <>a <a href="/talk/guides/push-to-talk/">key binding</a> (empty = push to talk off)</>, `""`],
              [<code key="f">wakeWord</code>, `boolean, stored, not used yet`, `false`],
              [<code key="f">wakePhrase</code>, `one line, ≤ 64 chars (empty = Hey {agent})`, `""`],
              [<code key="f">language</code>, <><code>en</code> | <code>de</code></>, <code key="d">en</code>],
              [<code key="f">voiceFile</code>, `one line, ≤ 512 chars`, `""`],
              [<code key="f">voiceTranscript</code>, `one line, ≤ 500 chars`, `""`],
            ]}
          />
          <p>Replies to your voice are always spoken. Picking None as the device or agent turns Wake word off; a bound key stays bound.</p>
        </DocsSection>

        <DocsSection id="examples" num="08" label="Try it">
          <h2>Try it</h2>
          <p>From <code>apps/desktop</code> in the repo. Both download what they need on first run.</p>
          <DocsCode>{`cargo run -p voice --example speak -- <data-dir> <en|de> "Text to say." [voice.wav "its transcript"]
cargo run -p voice --example listen -- <data-dir> <en|de> [--cpu] <clip.wav>...`}</DocsCode>
          <p>
            <code>speak</code> runs the worker the way the app does and prints every event. <code>listen</code> runs WAV files
            through the same pipeline as the microphone and prints one JSON line per clip. They run on Linux and Windows: on macOS
            the runtime comes only from the signed app bundle, which <code>cargo run</code> doesn&apos;t have.
          </p>
        </DocsSection>
      </DocsLayout>
    </>
  )
}
