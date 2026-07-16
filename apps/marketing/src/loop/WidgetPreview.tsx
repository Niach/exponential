import { X } from "lucide-react"

/* Static mock of the embeddable "Send feedback" dialog — pure site-styled
   markup, deliberately NOT the real widget script. */
export function WidgetPreview({ caption = true }: { caption?: boolean }) {
  return (
    <div className={`wmock`}>
      <div className={`wmock-card`} aria-hidden>
        <div className={`wmock-head`}>
          <span className={`wmock-title`}>Send feedback</span>
          <X size={13} className={`wmock-x`} />
        </div>
        <div className={`wmock-body`}>
          <div className={`wmock-shot`}>
            <div className={`wmock-shot-page`}>
              <span className={`wmock-bar is-w60`} />
              <span className={`wmock-bar is-w80`} />
              <span className={`wmock-bar is-w40`} />
              <span className={`wmock-shot-mark`} />
            </div>
            <div className={`wmock-shot-chips`}>
              <span className={`wmock-chip`}>Annotate</span>
              <span className={`wmock-chip`}>Retake</span>
            </div>
          </div>
          <div className={`wmock-field`}>
            <span className={`wmock-label`}>Title</span>
            <span className={`wmock-input`}>Checkout button does nothing</span>
          </div>
          <div className={`wmock-field`}>
            <span className={`wmock-label`}>Details</span>
            <span className={`wmock-input wmock-textarea`}>
              Clicked “Pay now” on Safari — no response.
            </span>
          </div>
          <div className={`wmock-footer`}>
            <span className={`wmock-send`}>Send feedback</span>
          </div>
        </div>
      </div>
      {caption && (
        <p className={`wmock-caption`}>
          The drop-in feedback widget — screenshot included.
        </p>
      )}
    </div>
  )
}
