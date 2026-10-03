# Slops: the consumer app with a face

State of the brainstorm on EXP-1174 (2026-10-03). It sits on top of the EXP-1171 direction (VAPP-1 D15 to D19, epic VAPP-69) and changes nothing underneath it: two brands, one backend, threads, visual runs, the Caretaker, claims, the host on your own computer. What is new is ONE noun and a face.

Drafts: `drafts/*.webp` (rendered from `mockups/`, see the recipe at the end). Same pictures on the Results of EXP-1174.

## 1. The noun

A **slop** is three things the user never has to tell apart:

| Part | What it is underneath | What the user sees |
| --- | --- | --- |
| the mascot | `vapps.mascot` DNA (VAPP-79) | the face everywhere: list row, thread avatar, busy spinner, the corner of the built app, the public page |
| the app | a board with a vapp row (VAPP-8), code + data on the host (D5) | "Recipe box", opened with one tap, running on "Danny's Mac" |
| the caretaker | the hidden hourly session on that device (EXP-1173) | "Momo looked around 12 min ago" + Ideas for you |

"vapp" is gone as a word. The pro view of a slop is its board in Exponential ("Open in Exponential" from the host tray only; the phone app never says it).

Versus Dots: a Dot is a cloud agent that does chores; a slop is a character that BUILDS and HOSTS an app on your own computer and lives inside it. The mascot is the hook that makes the app feel owned, the app is the product.

## 2. Name and domain (open)

Candidates: getslops.com, slops.build, slop.dance, slops.farm, viblets.com (every ending free).

Recommendation: **slops.farm** as the product and the public address (`momo.slops.farm` reads like a place a slop lives; "farm" fits "grow your slop", and the Caretaker is literally the farmer), **getslops.com** as the install redirect, **viblets.com** kept as the hedge if "slop" turns out too loaded with the "AI slop" meme for people who do not know the joke. Not "slops.build" (developer word) and not "slop.dance" (singular, hard to say in German).

Risk to decide with eyes open: "slop" is a negative word in tech discourse. Owning it ironically is memorable for the audience that knows the meme; the audience that does not (my mother) just hears a cute word. The mascot carries the tone either way.

## 3. What the consumer app shows

Exactly these screens, phone first, each built from the rows and bands Exponential already draws (one spec per surface, the SLOP-16 rule):

1. **Make your slop** (hatch, VAPP-80): mascot big, Shuffle, picker rows Body / Color / Eyes / Extra, Name, "Hatch Momo". One sentence that building needs a computer with Claude or ChatGPT, addable later.
2. **Home** (VAPP-70): the box ("What should we make?") with two chips (which slop, which computer), band **Your slops** with rows mascot · "Momo · Recipe box" · state line (awake on Danny's Mac / building · 4 min / asleep · Danny's Mac is off), band **Ideas for you** with the Caretaker's cards (title, one sentence of why, a mock tile, Show me / Not now), the one Caretaker line.
3. **Where should I live?** (VAPP-77): the native card after the first request without a host: this Mac or PC, a friend's shared computer, Later. Plus the one honest sentence: Slops never runs AI for you, your computer and your subscription do.
4. **The thread** (VAPP-71, VAPP-72): the person's bubbles, the slop's replies with its face, **Shape it** cards with two or three visual options (images now, live A2UI drafts later, VAPP-81) and "Something else…".
5. **Building** (EXP-1172 as consumers see it): one status row with the busy mascot ("Building on Danny's Mac · 4 min", the last step as a caption, **Peek** on the right = Show work), screenshot tiles arriving, the slop's short reply with Open / Change something.
6. **The app** (the renderers VAPP-13 to VAPP-16): the slop's own screens, with the mascot peeking in a corner as the way to talk to it ("Ask Momo"). People icon top right.
7. **People** (VAPP-65): who is in this slop, invite link, "Make it public" (VAPP-24).
8. **Host tray** (VAPP-76): "Slops on Danny's Mac", rows per slop (awake / building / asleep, Wake), the signed-in subscription and its headroom, "Look around every hour" toggle (the Caretaker), Open in Exponential, Pause all.
9. **Public page** (VAPP-24, VAPP-29): `momo.slops.farm` via the edge bridge, the app read-only, the author card with the mascot, Open in Slops, Make your own slop.

Words that never appear: issue, run, session, PR, git, branch, repo, device (it says "computer"), team, board, workflow, deploy. Words that do: slop, app, computer, people, awake / asleep / building, Peek, Shape it, Ideas for you, looked around.

## 4. The mascot (VAPP-79)

- **DNA** `{ body, hue, eyes, mouth, extra, tilt }`, seeded on the device (fnv1a over a random seed or the typed name), Shuffle = a new seed. Nothing runs on the server; no model, no upload.
- **Registry** `packages/mascots/parts.json`, append-only like `icons.json`: 10 bodies (bean, pear, cloud, pill, blob, square, heart, drop, flower, star), 8 hues (the avatar palette), 6 eyes, 4 mouths, 8 extras (beret, bowtie, glasses, monocle, sunglasses, sprout, antenna, none). About 200k looks before the name.
- **Look**: 2.5D, soft radial gradient over a solid base, a highlight, a colored shadow, a slight tilt. Dots-adjacent, not a copy: our bodies are flatter and the palette is ours.
- **States** ×4: idle (breathe + blink), busy (tilt + spinner badge), asleep (sleepy eyes), needs you (wide eyes). Sizes 22 / 36 / 44 / 120 / 190.
- **In the app**: a `Mascot` component in the `exp` catalog so a slop can render itself inside what it built. Full 3D is a later upgrade on the same DNA (a rigged blob per body); not needed to ship.
- **Server**: one PNG per DNA change (resvg) for push icons, OG images, link previews. That is the whole server cost.

## 5. Teams, people, devices

- **Hatch creates a personal team silently** (signups get no team today; the consumer app never shows the word). A slop = a board in it. Settings → nothing about teams.
- **Collaborators** join a slop, not a team: VAPP-65 cross-team grants, shown as People. They vibe in the same thread; the owner picks on Shape it cards, others suggest (a vote is a later nicety).
- **Devices** stay as they are: own computers, shared computers (`shared_team_ids`), the Lena case in the drafts. A slop runs on exactly one computer; "Where should I live?" picks it.
- **The e2e / peer engine** (VAPP-36 to VAPP-52) is unchanged: private slops are peer to peer, public ones go through the bridge. The claim is the same sentence as Exponential's: we never see what is built, the model provider still does.
- **Open in Exponential**: the team is a normal team, so the pro tool shows the board, the issues (= threads), the runs. Nothing to map; the mapping IS the one backend.

## 6. A2UI first, where it pays

- The CONTENT is A2UI from day one: every built app, every Shape it option, every Ideas card, every results tile. One catalog (`exp`), two themes: Exponential's and a rounder, warmer slops token set (bigger radii, the mascot hue as the accent). Not two catalogs.
- The SHELL (nav, box, bands, rows, pickers) stays native in v1. It is 9 screens of rows we already draw; A2UI chrome would cost the renderers before they exist and buy nothing a user sees. VAPP-64's slots can take the shell over later.
- **Interactive proposals** (VAPP-81): a Shape it option can be a static A2UI bundle the person taps through, and a "video" is an A2UI replay (timestamped messages the renderer plays, scrubs and loops). Vector, tiny, themed by the client, never rasterized. This answers "can we do videos" without a codec.

## 7. Sequence

Unchanged from EXP-1171, with the mascot pulled early because it is cheap and it is the brand:

1. Unslop (SLOP-1) until Exponential is in a good state.
2. Shared-UI readiness (SLOP-18) + **the mascot registry (VAPP-79)** in parallel (it is an icons.json-style package, no renderer needed).
3. Renderers VAPP-13 to VAPP-16 on `crates/vapp-client`.
4. The slops shells: web `/v/*` (VAPP-70) with the hatch flow (VAPP-80), then iOS + Android (VAPP-75), the host tray + CLI alias (VAPP-76), host-first onboarding (VAPP-77).
5. Live drafts + replay (VAPP-81), public pages + claims (VAPP-24, VAPP-29), People (VAPP-65), brand + store (VAPP-78).

## 8. Edit map for the open issues (run after the name is picked)

| Issue | Change |
| --- | --- |
| VAPP-69 | title + body: "vapps" → the name; add the noun table (section 1) and the mascot; sub-issues VAPP-79, VAPP-80, VAPP-81 |
| VAPP-1 | D20: the slop noun + mascot DNA (proposed on the issue) |
| VAPP-70, VAPP-75, VAPP-76, VAPP-77, VAPP-78 | brand swap (`vapps.lol` → the domain, `vapps` alias → the name), copy from section 3 (computer, awake/asleep/building, Peek) |
| VAPP-78 | absorbs the mascot as the mark: the app icon IS a slop; store listings around "make your slop" |
| VAPP-24, VAPP-29 | `<slug>.vapps.lol` → `<slug>.<domain>`, scheme `slops://` |
| VAPP-27 | Store → "Browse" stays; blueprints get a slop on the card |
| VAPP-65 | "friends" → People, per slop |
| VAPP-72, VAPP-73, VAPP-56 | cards carry the mascot; options may be live drafts (VAPP-81) |
| VAPP-8 | `vapps.mascot` jsonb |
| VAPP-5 | `Mascot` catalog component + the replay fixture with timestamps |
| VAPP-61 | wording only ("Connect Claude / ChatGPT on this computer") |
| EXP-1172 | consumer word "Peek" for Show work in the copy fixture |
| EXP-1173 | Ideas cards show the slop; the Caretaker line copy ("looked around") |
| SLOP-1, SLOP-18 | sequence text names the mascot registry beside SLOP-18 |
| MKT-9 | the branding question is answered by the two brands; close or point here |
| EXP-1118 | unchanged (StyleX later) |
| board "vApps" | rename to the name; prefix VAPP stays (identifiers are stored) |

Untouched: the peer link stack (VAPP-33 to VAPP-52), the renderers (VAPP-13 to VAPP-16, VAPP-42), the decisions layer research (VAPP-53 to VAPP-59), VAPP-67/68, everything outside the Exponential team.

## Mockup recipe

```sh
cd docs/slops/mockups && mkdir -p out
node render.mjs make:390:844:2 nohost:390:844:2 home:390:844:2 thread:390:844:2 building:390:844:2 app:390:844:2 tray:410:470:2 public:1100:680:1.5 sheet:1000:700:2
node montage.mjs
```

`render.mjs` resolves Playwright and sharp from the main checkout's `node_modules` (a worktree has none); `slop.js` is the mascot generator, `parts.js` the icon snippets, `base.css` the app's tokens.
