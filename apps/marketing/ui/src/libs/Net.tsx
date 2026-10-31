/* EXP-1421: exponential.at/net/, eNet's one docs page. The decisions of
   2026-10-29 (iroh, no WebRTC; relay-only in the browser; the channels);
   the package names are docs/libraries.md's. EXP-1427 built the core, the
   UniFFI and wasm facades and push sealing (apps/desktop/crates/enet-core*);
   EXP-1435 ships the push-only build in the mobile apps. EXP-1438 published
   the core (enet-core 0.1.0, tag enet-v0.1.0); the bindings say planned.
   EXP-1457: every install runs a node (VAPP-37/40), session content is
   sealed per tree (EXP-1458/1459), steering rides the chat channel
   (VAPP-33) and the steer relay stopped carrying content (VAPP-43). */
import { DocsLayout, DocsSection, type DocsSection as DocsSectionType } from "@exp/site-shell"
import { StatusChip } from "../../../src/components/blog/StatusChip"
import { DocsTable, PageHero } from "../components/Content"
import { LIBRARIES_NAV } from "../lib/libraries"
import type { PageProps } from "../lib/routes"

const SECTIONS: DocsSectionType[] = [
  { id: `status`, num: `01`, label: `Status` },
  { id: `why`, num: `02`, label: `In Exponential` },
  { id: `how`, num: `03`, label: `How it connects` },
  { id: `channels`, num: `04`, label: `Channels` },
  { id: `browser`, num: `05`, label: `In the browser` },
  { id: `packages`, num: `06`, label: `Packages` },
]

const PLANNED = <StatusChip status="planned">planned</StatusChip>
const PUBLISHED = <StatusChip status="shipped">0.1.0</StatusChip>
const BUILT = <StatusChip status="in-progress">built</StatusChip>
const IN_APP = <StatusChip status="in-progress">in the app</StatusChip>

export default function Net({ path }: PageProps) {
  return (
    <>
      <PageHero eyebrow="Library" title="eNet">
        Peer to peer and end-to-end encrypted tunnels between every Exponential client: desktop, phone, server and browser.
      </PageHero>
      <DocsLayout nav={LIBRARIES_NAV} title="Libraries" sections={SECTIONS} currentPath={path}>
        <DocsSection id="status" num="01" label="Status">
          <h2>Status</h2>
          <p>
            The Rust core is published (<code>enet-core</code> 0.1.0) and runs on iroh 1.4 with its Swift, Kotlin and
            WebAssembly facades. Every Exponential install runs an eNet node: the desktop app, the CLI daemon, iOS,
            Android and the browser. The bindings are not published on their own yet.
          </p>
        </DocsSection>
        <DocsSection id="why" num="02" label="In Exponential">
          <h2>In Exponential</h2>
          <p>
            Session content is sealed end to end. Each session tree has its own key, wrapped for every install allowed to
            read it; messages, cards, titles and previews are sealed on the device that writes them, and the server stores
            and syncs only ciphertext plus the metadata it needs for routing and wake-ups.
          </p>
          <p>
            Watching and steering a coding session goes over eNet&apos;s chat channel, straight from your phone or browser to
            the device running it. The steer relay no longer carries any session content: it keeps device presence,
            remote control and voice calls. Agents on different devices message each other over eNet too, and pushes
            are sealed to the phone that opens them.
          </p>
          <p>
            Keys for a new install of yours arrive automatically from any online device that holds them. That protects
            against a database leak and a passive server, not against someone who controls the server and signs in as
            you. MCP clients without a device gateway, like a hosted chat app, see metadata only.
          </p>
        </DocsSection>
        <DocsSection id="how" num="03" label="How it connects">
          <h2>How it connects</h2>
          <p>
            eNet is built on <a className="content-link" href="https://www.iroh.computer/">iroh</a> (MIT / Apache-2.0): every
            device has a key, peers dial each other by that key, and QUIC connections punch through NATs where they can. When
            they can&apos;t, traffic falls back to an iroh relay that we host; it sees only encrypted packets. No WebRTC.
          </p>
        </DocsSection>
        <DocsSection id="channels" num="04" label="Channels">
          <h2>Channels</h2>
          <DocsTable
            head={[`Channel`, `What`, ``]}
            rows={[
              [<strong key="c">Chat</strong>, `Reliable, ordered, acknowledged messages between devices: steering, sealed delivery, agent to agent`, IN_APP],
              [<strong key="c">Files</strong>, `Content-addressed transfers (iroh-blobs), verified on arrival`, BUILT],
              [<strong key="c">Records</strong>, `Signed records synced between peers over gossip, last writer wins`, BUILT],
              [<strong key="c">Calls</strong>, `Audio frames over QUIC datagrams, for calling your agent with eTalk`, IN_APP],
              [<strong key="c">Push</strong>, `Notifications sealed to the receiving device; the push relay forwards bytes it can't read`, IN_APP],
            ]}
          />
        </DocsSection>
        <DocsSection id="browser" num="05" label="In the browser">
          <h2>In the browser</h2>
          <p>
            A web page can&apos;t open raw UDP, so the browser build (WebAssembly) always goes through the relay. It stays end
            to end encrypted: the relay forwards, it can&apos;t read.
          </p>
        </DocsSection>
        <DocsSection id="packages" num="06" label="Packages">
          <h2>Packages</h2>
          <p>
            One per platform, built from one Rust core: the crates <code>enet-core</code>,{` `}
            <code>exponential-net-ffi</code> and <code>exponential-net-wasm</code> in the Exponential repo. MIT or Apache-2.0.
            The core is on crates.io (<code>cargo add enet-core</code>); the bindings come next.
          </p>
          <DocsTable
            head={[`Package`, `Registry`, `What`, ``]}
            rows={[
              [<code key="n">enet-core</code>, `crates.io`, `The Rust core`, PUBLISHED],
              [<code key="n">@exponential_app/enet</code>, `npm`, `The WebAssembly build, relay only`, PLANNED],
              [<code key="n">at.exponential:enet</code>, `Maven Central`, `Android, through UniFFI`, PLANNED],
              [<code key="n">ExponentialNet</code>, `SwiftPM`, `iOS and macOS, through UniFFI`, PLANNED],
            ]}
          />
        </DocsSection>
      </DocsLayout>
    </>
  )
}
