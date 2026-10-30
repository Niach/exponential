/* EXP-1421: exponential.at/net/, eNet's one docs page. The decisions of
   2026-10-29 (iroh, no WebRTC; relay-only in the browser; the channels);
   the package names are docs/libraries.md's. EXP-1427 built the core, the
   UniFFI and wasm facades and push sealing (apps/desktop/crates/exponential-net*);
   EXP-1435 ships the push-only build in the mobile apps. Nothing is
   published yet, so the packages say planned. */
import { DocsLayout, DocsSection, type DocsSection as DocsSectionType } from "@exp/site-shell"
import { StatusChip } from "../../../src/components/blog/StatusChip"
import { DocsTable, PageHero } from "../components/Content"
import { LIBRARIES_NAV } from "../lib/libraries"
import type { PageProps } from "../lib/routes"

const SECTIONS: DocsSectionType[] = [
  { id: `status`, num: `01`, label: `Status` },
  { id: `why`, num: `02`, label: `Why` },
  { id: `how`, num: `03`, label: `How it connects` },
  { id: `channels`, num: `04`, label: `Channels` },
  { id: `browser`, num: `05`, label: `In the browser` },
  { id: `packages`, num: `06`, label: `Packages` },
]

const PLANNED = <StatusChip status="planned">planned</StatusChip>
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
            In development. The Rust core runs on iroh 1.4 with its Swift, Kotlin and WebAssembly facades, and the iOS and
            Android apps already use its push sealing to open notifications only they can read. Nothing is published yet,
            and steering a session still goes through the relay described below.
          </p>
        </DocsSection>
        <DocsSection id="why" num="02" label="Why">
          <h2>Why</h2>
          <p>
            Today a phone steering a coding session on your desktop goes through our steer relay, which forwards each message
            and can read it. eNet replaces that with a direct, end-to-end encrypted connection between your devices. A relay
            only carries bytes it cannot read, and only when no direct path exists.
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
              [<strong key="c">Chat</strong>, `Reliable, ordered, acknowledged messages between devices`, BUILT],
              [<strong key="c">Files</strong>, `Content-addressed transfers (iroh-blobs), verified on arrival`, BUILT],
              [<strong key="c">Records</strong>, `Signed records synced between peers over gossip, last writer wins`, BUILT],
              [<strong key="c">Calls</strong>, `Audio frames over QUIC datagrams, for calling your agent with eTalk`, BUILT],
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
            One per platform, built from one Rust core: the crates <code>exponential-net</code>,{` `}
            <code>exponential-net-ffi</code> and <code>exponential-net-wasm</code> in the Exponential repo. MIT or Apache-2.0.
            None is published yet.
          </p>
          <DocsTable
            head={[`Package`, `Registry`, `What`, ``]}
            rows={[
              [<code key="n">exponential-net</code>, `crates.io`, `The Rust core`, PLANNED],
              [<code key="n">@exponential_app/net</code>, `npm`, `The WebAssembly build, relay only`, PLANNED],
              [<code key="n">at.exponential:net</code>, `Maven Central`, `Android, through UniFFI`, PLANNED],
              [<code key="n">ExponentialNet</code>, `SwiftPM`, `iOS and macOS, through UniFFI`, PLANNED],
            ]}
          />
        </DocsSection>
      </DocsLayout>
    </>
  )
}
