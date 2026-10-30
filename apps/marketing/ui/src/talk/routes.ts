/* THE page manifest of exponential.at/talk (NEW-15): every Talk path, its
   page module and its SEO. ui/src/lib/routes.ts appends it to ROUTES, so the
   client entry, the prerender (one dist/<path>/index.html each, sitemap.xml,
   llms.txt) and routes.test.ts read the two sections as one. */
import { breadcrumb } from "@exp/site-shell/seo"
import type { Route } from "../lib/routes"
import { SITE_ORIGIN } from "../lib/site"
import { TALK_GUIDES, TALK_HOME, talkGuidePath } from "./nav"

const NAME = `eTalk`
const OG = `/og/og-talk.png`
const crumbs = (...items: { name: string; path: string }[]) =>
  breadcrumb(SITE_ORIGIN, [{ name: NAME, path: TALK_HOME }, ...items])

const page = (
  path: string,
  load: Route[`load`],
  title: string,
  description: string,
  sources: string[],
  jsonLd?: Route[`jsonLd`]
): Route => ({ path, htmlFile: `${path.slice(1)}index.html`, load, title, description, sources, ogImage: OG, ...(jsonLd ? { jsonLd } : {}) })

const GUIDE_LOADER = () => import("./Guide")

export const TALK_ROUTES: readonly Route[] = [
  page(
    TALK_HOME,
    () => import("./TalkHome"),
    `${NAME} · speech in and out for your agents`,
    `Talk to your Exponential agents: push to talk and spoken replies (beta) on your own GPU, in English and German, with Silero VAD, Parakeet and Qwen3-TTS. Open source.`,
    [`ui/src/talk`, `../desktop/crates/talk`, `../desktop/crates/voice`],
    [
      {
        "@context": `https://schema.org`,
        "@type": `SoftwareSourceCode`,
        name: NAME,
        description: `Speech in and out (beta) for Exponential's agents: Silero VAD and Parakeet for speech recognition, Qwen3-TTS for spoken replies, through CrispASR, on the user's own GPU.`,
        codeRepository: `https://github.com/Niach/exponential`,
        programmingLanguage: [`Rust`],
        license: `https://www.apache.org/licenses/LICENSE-2.0`,
        url: `${SITE_ORIGIN}${TALK_HOME}`,
      },
    ]
  ),
  page(
    `/talk/how-it-works/`,
    () => import("./HowItWorks"),
    `How it works · ${NAME}`,
    `The voice device, speech in (Silero VAD, Parakeet), speech out (Qwen3-TTS cloned from your clip), push to talk, and what is coming.`,
    [`ui/src/talk/HowItWorks.tsx`],
    crumbs({ name: `How it works`, path: `/talk/how-it-works/` })
  ),
  page(
    `/talk/install/`,
    () => import("./Install"),
    `Install · ${NAME}`,
    `Turn on eTalk: pick your voice device and agent in Settings → Voice, let the models download, allow the microphone, and pick a shortcut.`,
    [`ui/src/talk/Install.tsx`],
    crumbs({ name: `Install`, path: `/talk/install/` })
  ),
  page(
    `/talk/guides/`,
    () => import("./Guides"),
    `Guides · ${NAME}`,
    `Step-by-step guides for eTalk: record a reference clip for your own voice, and set up a push-to-talk shortcut on your desktop.`,
    [`ui/src/talk/Guides.tsx`],
    crumbs({ name: `Guides`, path: `/talk/guides/` })
  ),
  ...TALK_GUIDES.map((g) =>
    page(
      talkGuidePath(g.slug),
      GUIDE_LOADER,
      `${g.title} · ${NAME}`,
      g.slug === `your-voice`
        ? `Make a reference clip for your Exponential voice: pick 8 to 15 seconds of calm speech, cut it to a WAV, write the transcript, enter both.`
        : `Push-to-talk shortcuts on the voice device: what X11, Wayland, macOS and Windows allow, and what happens when a key can't be grabbed.`,
      [`ui/src/talk/Guide.tsx`],
      crumbs({ name: `Guides`, path: `/talk/guides/` }, { name: g.title, path: talkGuidePath(g.slug) })
    )
  ),
  page(
    `/talk/api/`,
    () => import("./Api"),
    `API · ${NAME}`,
    `The speech worker's wire (requests, events, level frames), the CrispASR runtime's sessions and the Settings → Voice fields, as they ship today.`,
    [`ui/src/talk/Api.tsx`, `../desktop/crates/talk/src/lib.rs`, `../desktop/crates/crispasr/src`],
    crumbs({ name: `API`, path: `/talk/api/` })
  ),
  page(
    `/talk/numbers/`,
    () => import("./Numbers"),
    `Numbers · ${NAME}`,
    `Measured latency, accuracy, VRAM and download sizes for eTalk's speech in and speech out, on a GPU and on the CPU, with the targets not yet measured.`,
    [`ui/src/talk/Numbers.tsx`, `../../docs/talk-spike.md`],
    crumbs({ name: `Numbers`, path: `/talk/numbers/` })
  ),
  page(
    `/talk/licences/`,
    () => import("./Licences"),
    `Licences · ${NAME}`,
    `The licences of the models and libraries eTalk downloads or links: CrispASR, Parakeet, Qwen3-TTS, Silero VAD, cpal, global-hotkey and the CUDA runtime.`,
    [`ui/src/talk/Licences.tsx`, `../../docs/third-party-licences.md`],
    crumbs({ name: `Licences`, path: `/talk/licences/` })
  ),
]
