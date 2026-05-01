import { eq } from "drizzle-orm"
import { db } from "@/db/connection"
import { users } from "@/db/auth-schema"
import { jsonResponse } from "./helpers"

export type McpUser = typeof users.$inferSelect

export type McpAuthResult =
  | { user: McpUser }
  | { errorResponse: Response }

export async function mcpAuthenticate(
  request: Request
): Promise<McpAuthResult> {
  const token = process.env.MCP_API_TOKEN
  const email = process.env.MCP_USER_EMAIL

  if (!token || !email) {
    return {
      errorResponse: jsonResponse(503, {
        error: `MCP server is not configured. Set MCP_API_TOKEN and MCP_USER_EMAIL.`,
      }),
    }
  }

  const header = request.headers.get(`authorization`) ?? ``
  const provided = header.startsWith(`Bearer `) ? header.slice(7) : ``

  if (!provided || provided !== token) {
    return {
      errorResponse: jsonResponse(401, {
        error: `Invalid or missing MCP token`,
      }),
    }
  }

  const [existing] = await db
    .select()
    .from(users)
    .where(eq(users.email, email))
    .limit(1)

  if (existing) return { user: existing }

  const [created] = await db
    .insert(users)
    .values({
      id: crypto.randomUUID(),
      email,
      name: email.split(`@`)[0],
      emailVerified: true,
    })
    .returning()

  return { user: created }
}
