/* ─── HelpdeskChatDemo — chat + details rail only (EXP-602) ───
   The home page's helpdesk subsection shows just the conversation view of
   the Support inbox: chathead, bubbles, composer and the details rail — no
   thread list, no widget flow. Static (decorative, rendered inert) and
   context-free: it composes the presentational pieces exported by
   SupportInbox (Bubble, SupportRail) inside the house fixed-canvas +
   useDemoScale pattern. */
import { useDemoScale } from "../lib/use-demo-scale"
import { IcCheck, IcSend } from "../ide/icons"
import { IcMail, IcStickyNote } from "./icons"
import { Bubble, SupportRail } from "./SupportInbox"
import { SUPPORT_THREADS } from "./data"

const BASE_W = 880
const DEMO_H = 460

/* Mara's thread: has widget context and no linked issue, so the rail shows
   Reporter · Context · the Escalate board picker, per the real app. */
const THREAD = SUPPORT_THREADS[0]

export function HelpdeskChatDemo() {
  const { ref, scale } = useDemoScale(BASE_W)

  return (
    <div
      ref={ref}
      className={`web-scale`}
      style={{ height: Math.round(DEMO_H * scale) }}
    >
      <div
        className={`web-root is-static co-helproot`}
        style={
          scale < 1
            ? { width: BASE_W, transform: `scale(${scale})` }
            : undefined
        }
      >
        <div className="web-sup co-helpsup">
          <div className="web-sup-chat">
            <div className="web-sup-chathead">
              <div className="web-sup-chatwho">
                <span className="web-sup-name">{THREAD.reporterName}</span>
                <span className="web-sup-issuetitle">{THREAD.title}</span>
              </div>
              <button className="web-btn-outline" type="button">
                <IcCheck size={12} />
                Close ticket
              </button>
            </div>
            <div className="web-sup-msgs">
              {THREAD.messages.map((m, i) => (
                <Bubble key={i} message={m} reporter={THREAD.reporterName} />
              ))}
            </div>
            <div className="web-sup-composer">
              <div className="web-sup-modes">
                <button type="button" className="web-modepill is-active">
                  <IcMail size={12} />
                  Reply
                </button>
                <button type="button" className="web-modepill is-note">
                  <IcStickyNote size={12} />
                  Internal note
                </button>
              </div>
              <div className="web-sup-inputrow">
                <textarea
                  className="web-composer-input"
                  rows={2}
                  placeholder={`Reply to ${THREAD.reporterName}… (emailed to them)`}
                  readOnly
                />
                <button className="web-send" type="button" disabled>
                  <IcSend size={14} />
                </button>
              </div>
            </div>
          </div>
          <SupportRail thread={THREAD} issue={null} interactive={false} />
        </div>
      </div>
    </div>
  )
}
