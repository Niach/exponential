// One-click email unsubscribe handler (RFC 8058). The opaque token minted on
// the user_notification_prefs row IS the auth — no session required, so the
// link works from any mail client. The DB flip is injected so the route stays
// thin and this logic is unit-testable without a live database.
//
// Only POST mutates: mail-security link scanners (SafeLinks, URL Defense)
// prefetch every GET link in an email body, so the footer link's GET renders
// a confirmation page instead — its form POSTs back to the same URL, keeping
// the token in the query string and out of the HTML. RFC 8058 one-click
// clients POST directly (List-Unsubscribe-Post) and skip the interstitial.

function page(title: string, message: string, extra = ``): string {
  return `<!doctype html>
<html>
  <head><meta charset="utf-8"/><meta name="viewport" content="width=device-width, initial-scale=1"/><title>${title}</title></head>
  <body style="margin:0;padding:48px 16px;background:#09090b;font-family:-apple-system,Segoe UI,Roboto,Helvetica,Arial,sans-serif;color:#fafafa;">
    <div style="max-width:480px;margin:0 auto;background:#18181b;border:1px solid #27272a;border-radius:12px;padding:32px;">
      <h1 style="margin:0 0 12px;font-size:18px;">${title}</h1>
      <p style="margin:0;font-size:14px;line-height:1.6;color:#a1a1aa;">${message}</p>${extra}
    </div>
  </body>
</html>`
}

function htmlResponse(
  status: number,
  title: string,
  message: string,
  extra = ``
): Response {
  return new Response(page(title, message, extra), {
    status,
    headers: { "Content-Type": `text/html; charset=utf-8` },
  })
}

// Posts back to the current URL, so the token never appears in the markup.
const CONFIRM_FORM = `
      <form method="post" style="margin:20px 0 0;">
        <button type="submit" style="appearance:none;border:0;border-radius:8px;padding:10px 20px;background:#fafafa;color:#09090b;font-size:14px;font-weight:600;cursor:pointer;">Unsubscribe</button>
      </form>`

export async function handleUnsubscribe(
  token: string | null,
  unsubscribe: (token: string) => Promise<boolean>,
  method = `POST`
): Promise<Response> {
  if (!token) {
    return htmlResponse(
      400,
      `Missing unsubscribe link`,
      `This unsubscribe link is incomplete. Please use the link from the bottom of a notification email.`
    )
  }

  if (method.toUpperCase() !== `POST`) {
    return htmlResponse(
      200,
      `Unsubscribe from email notifications?`,
      `You will no longer receive email notifications from Exponential. You can re-enable them anytime from Account → Notifications.`,
      CONFIRM_FORM
    )
  }

  let ok = false
  try {
    ok = await unsubscribe(token)
  } catch (err) {
    console.error(`[email] unsubscribe failed:`, err)
    return htmlResponse(
      500,
      `Something went wrong`,
      `We couldn't process your unsubscribe request. Please try again in a moment.`
    )
  }

  if (!ok) {
    return htmlResponse(
      404,
      `Unknown unsubscribe link`,
      `This unsubscribe link is invalid or has been replaced. You can manage email notifications from your account settings in Exponential.`
    )
  }

  return htmlResponse(
    200,
    `You're unsubscribed`,
    `You will no longer receive email notifications from Exponential. You can re-enable them anytime from Account → Notifications.`
  )
}
