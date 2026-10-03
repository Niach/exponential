# Slops: the consumer app with a face

State of the brainstorm on EXP-1174 (2026-10-03, revised 2026-10-04). It sits on top of the EXP-1171 direction (VAPP-1 D15 to D19, epic VAPP-69) and changes nothing underneath it: two brands, one backend, threads, visual runs, the Caretaker, claims, the host on your own computer.

Working name **slops**, working domain **slopnet.app** (nothing bought). The characters are called **mascots** until the name is decided. Drafts: `drafts/*.webp` (rendered from `mockups/`, recipe at the end), same pictures on the Results of EXP-1174.

## 1. The nouns

| Noun | Underneath | What the user sees |
| --- | --- | --- |
| **slop** | `vapps` row owned by a person: a mascot + an app (code + data on a host) + the threads about it | "Momo", a face, the app it built |
| **server** | a team + its devices | "Danny's Mac", "Lena", "Homelab": a place with computers and people |
| **mascot** | one 512² PNG with alpha, prompted, in our house style | the face everywhere |
| **caretaker** | the hidden hourly session per device (EXP-1173) | "Looks around hourly · 12 min ago" |

Everyone creates their own slops. A slop joins a server to work: the server has the computer it runs on and the people who can see it. A slop with no server exists only on the phone ("No server yet"). Your own computer is your own server (the host app registers the device, the team is created with it). The pro view of a slop is its board in Exponential.

Underneath: `vapps.owner_id` + nullable `team_id`. Without a team the row syncs per user like pins (never trash-scoped). Joining a server sets `team_id` and creates the board there (threads, runs, results). Moving later = re-parenting the board while no run is live. Collaborators are the server's members; "friends" grants (VAPP-65) stay for sharing one slop into another server.

## 2. What the app shows

Phone first, every screen built from rows and bands Exponential already draws, as little text as possible:

1. **Create a slop** (VAPP-80): the box is the screen. Type "a stoned towel", the mascot appears big with three alternatives under it, **Again** / **Keep**, the name under the picture (tap to edit). Suggestion chips. Nothing explained.
2. **Home** (VAPP-70): the server rail on top (square tiles with a status dot: own computers, friends' servers, Join), band **Your slops** (mascot · name · app · server, a dot or spinner), band **Ideas** (one tile, a title, ✕ / ✓), the box at the bottom.
3. **Server**: the computer tile with its dot and subscription headroom, **Slops here**, **People** with Invite, **Caretaker** toggle.
4. **Where should I live?** (VAPP-77): the native card after the first request without a server: this Mac or PC, join a server (invite / QR), later.
5. **The thread** (VAPP-71, VAPP-72): bubbles, the slop's replies with its face, **Shape it** cards with visual options (images now, live A2UI drafts later, VAPP-81).
6. **Building** (EXP-1172 for consumers): one status row with the busy mascot, tiles arriving, the slop's short reply with Open / Change something. **Peek** = Show work.
7. **The app** (renderers VAPP-13 to VAPP-16) with the mascot peeking in a corner ("Ask Momo").
8. **Host tray** (VAPP-76): "Slops on Danny's Mac", the slops awake / building / asleep, the subscription, the Caretaker toggle, Open in Exponential.
9. **Public page** (VAPP-24, VAPP-29): `momo.slopnet.app` via the edge bridge, read-only app, the author card, Open in Slops.

Words that never appear: issue, run, session, PR, git, branch, repo, device, team, board, deploy. Words that do: slop, mascot, server, app, computer, people, awake / asleep / building, Peek, Shape it, Ideas.

## 3. The mascot pipeline (VAPP-79)

The requirement: "I want a stoned towel as my buddy the same way I want a cat", endless, one style, identical on every platform, cheap for us. The answer is a bitmap with alpha, generated in ONE house style, served from a library first and generated on the user's own computer second.

### What is possible today (checked 2026-10-04)

| Option | Speed | Where | Verdict |
| --- | --- | --- | --- |
| **FLUX.2 [klein] 4B** (Black Forest Labs, Apache 2.0 for 4B, distilled 4-step + base) | 0.19 s @512² and 0.57 s @1024² on an H100, "under 0.5 s" on a 4090, ~16 GB VRAM bf16 (fp8/GGUF less); Apple Silicon: ~1 s on M5 Max in Draw Things, reported ~24 s on M4 Pro | NVIDIA hosts, newest Macs | the quality model for the library and for NVIDIA hosts; LoRA in ~60 min on a 4090 from 15–40 images |
| **Z-Image-Turbo** (Tongyi, Apache 2.0, 6B, 8 steps) | ~2.3 s on a 4090; a WebNN/ONNX browser port exists | hosts | runner-up; bigger download |
| **SD-Turbo / SDXL-Lightning class** (1–4 steps, ~2.5 GB fp16) | **measured here on the M4 Pro: 0.8 s at 1 step, 1.45 s at 4 steps, 512², 95 s one-time load** | any Mac/PC with a GPU | the Apple-Silicon host path today; needs the LoRA, stock output drifts in style (see the gallery) |
| **NanoFLUX** (ICML 2026, 2.4B distilled from FLUX schnell) | 512² in ~2.5 s on a phone, per the paper | phones, when weights ship | watch |
| **Phone, today**: Core ML SD1.5 (Off Grid) / MediaPipe SD1.5 | 8–15 s on iPhone 15 Pro+, ~15 s on high-end Android, 1–4 GB model | phones | optional later, not v1 |
| **Apple ImageCreator** (iOS 18.4+ beta) | fast, but three Apple styles only; `notSupported` on this macOS 27 beta; reported discontinued in the 27 OS line | Apple only | no |
| **Browser WebGPU** (ONNX Runtime Web, SD-Turbo / Z-Image-Turbo ports) | 1–3 s on an M1/M2 laptop, ~1.5 GB INT8 download, not on phones | laptops | no; web uses the library |
| **Text→SVG** (OmniSVG, NeurIPS 2025; StarVector) | seconds to minutes, autoregressive; style not controllable | server GPU | no for v1; a vector upgrade later |
| **Image→3D** (TripoSR <0.5 s on an A100 with 6 GB VRAM, SPAR3D <1 s, TRELLIS.2 ~3 s on an H100, MIT, Dec 2025; Hunyuan3D 2.1 with PBR) | seconds, but on data-center or big consumer GPUs | hosts with big GPUs | later: a host-side upgrade from the chosen 2D mascot to a GLB; v1 is 2D |
| **Style consistency**: a style LoRA (multi-token DreamBooth-LoRA paper: artists rated generated characters 3.87/5 vs 2.58 for plain DreamBooth, 10–30 references) + **LayerDiffuse** or a rembg pass for alpha | | | the method |

### The pipeline

1. **One LoRA, trained once by us** on 20–40 curated reference mascots (klein 4B base; the SD-Turbo-class fallback gets the same style LoRA). Fixed prompt frame and negative prompt, alpha background. Output: one 512² PNG with alpha. A bitmap is the same on iOS, Android, web and desktop, so style parity comes for free; no per-platform renderer.
2. **Library + vector search = the default for everyone.** We generate tens of thousands offline once (nouns × moods × accessories, a few rented GPU hours: 20k images at 0.5 s is about 3 hours on a 4090), embed with CLIP/SigLIP, store in `pgvector`. "Create a slop" embeds the TEXT on the server (CPU, milliseconds) and returns the nearest four. Instant, no GPU per request, works on web and on a phone with no computer.
3. **Generate on the host = the endless path.** With a server online, Again = a fresh generation there with the LoRA. The result becomes the slop's mascot and joins the library, so the farm grows with its users. "Stoned towel" works because moods are part of the prompt frame; if the library has nothing close (low similarity) and a host is online, the app generates instead of showing a poor match.
4. **Poses**: busy and asleep variants by image-to-image from the chosen mascot with the same seed, made on the host; clients animate between the three (breathe, blink, tilt).
5. **Later**: on-phone generation (Core ML / MediaPipe with the LoRA) if people without a computer want Again; 3D from the 2D mascot on big-GPU hosts.

The local test (`mockups/bench.py`, stock SD-Turbo, no LoRA) proves the speed and shows why the LoRA is not optional: six prompts came back in one sticker-ish family but drifted between line art and painted looks, with random backgrounds, and the towel became a plant. The gallery is in `drafts/gallery.webp`.

The part-based DNA renderer from the first round (`mockups/slop.js`) stays as the offline placeholder while a mascot loads and as seed data for the LoRA training set.

## 4. A2UI first, where it pays

- All CONTENT is A2UI from day one: every built app, every Shape it option, every Ideas card, every results tile. One `exp` catalog, two themes (Exponential's and a rounder, warmer slops token set).
- The SHELL stays native in v1 (nine screens of rows). VAPP-64's slots can take it over later.
- Interactive proposals (VAPP-81): a Shape it option can be a static A2UI bundle the person taps through; a "video" is an A2UI replay (timestamped messages the renderer plays, scrubs, loops). Never rasterized.

## 5. Sequence

1. Unslop (SLOP-1) until Exponential is in a good state.
2. Shared-UI readiness (SLOP-18) + the mascot pipeline (VAPP-79: LoRA, library, search endpoint) in parallel; the pipeline needs no renderer.
3. Renderers VAPP-13 to VAPP-16 on `crates/vapp-client`.
4. The slops shells: web `/v/*` (VAPP-70) with Create a slop (VAPP-80), then iOS + Android (VAPP-75), host tray + CLI alias (VAPP-76), host-first onboarding (VAPP-77).
5. Live drafts + replay (VAPP-81), public pages + claims (VAPP-24, VAPP-29), sharing (VAPP-65), brand + store (VAPP-78).

## 6. Edit map for the open issues (run once the name is final)

| Issue | Change |
| --- | --- |
| VAPP-69 | title + body: "vapps" → the name; the nouns table (section 1: slop, server, mascot, caretaker); sub-issues VAPP-79, VAPP-80, VAPP-81 |
| VAPP-1 | D20: the slop noun, the server model, the prompted mascot |
| VAPP-8 | `vapps.owner_id`, nullable `team_id`, `mascot` jsonb; the per-user sync for server-less slops |
| VAPP-70, VAPP-75, VAPP-76, VAPP-77, VAPP-78 | brand swap (`vapps.lol` → the domain), server rail, copy from section 2 |
| VAPP-78 | the app icon IS a mascot; store listings around "create a slop" |
| VAPP-24, VAPP-29 | `<slug>.vapps.lol` → `<slug>.slopnet.app`, scheme `slops://` |
| VAPP-27 | Store → Browse stays; cards carry a mascot |
| VAPP-65 | "friends" → People, per server and per slop |
| VAPP-72, VAPP-73, VAPP-56 | cards carry the mascot; options may be live drafts (VAPP-81) |
| VAPP-5 | `Mascot` catalog component (image + state), the replay fixture with timestamps |
| VAPP-61 | wording ("Connect Claude / ChatGPT on this computer") |
| VAPP-74 | image generation shares the host runtime with the mascot pipeline (one model loader on the host) |
| EXP-1172 | consumer word "Peek" in the copy fixture |
| EXP-1173 | Ideas cards show the mascot; the Caretaker line copy |
| SLOP-1, SLOP-18 | sequence text names the mascot pipeline beside SLOP-18 |
| MKT-9 | answered by the two brands; close or point here |
| board "vApps" | rename to the name; prefix VAPP stays |

Untouched: the peer link stack (VAPP-33 to VAPP-52), the renderers (VAPP-13 to VAPP-16, VAPP-42), the decisions research (VAPP-53 to VAPP-59), VAPP-67/68, everything outside the Exponential team.

## Mockup recipe

```sh
cd docs/slops/mockups && mkdir -p out
node render.mjs create:390:844:2 home2:390:844:2 server:390:844:2 make:390:844:2 nohost:390:844:2 home:390:844:2 thread:390:844:2 building:390:844:2 app:390:844:2 tray:410:470:2 public:1100:680:1.5 sheet:1000:700:2 gallery:1000:600:2
node montage.mjs
```

`render.mjs` resolves Playwright and sharp from the main checkout's `node_modules` (a worktree has none); `slop.js` is the part-based placeholder generator, `parts.js` the icon snippets, `base.css` the app's tokens. `bench.sh` + `bench.py` = the SD-Turbo test (a throwaway venv under `/tmp`, downloads ~2.5 GB).
