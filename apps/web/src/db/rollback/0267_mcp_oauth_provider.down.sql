-- Rollback of 0267_mcp_oauth_provider (better-auth 1.7's MCP OAuth tables),
-- for redeploying a better-auth 1.6 image. Run BY HAND with psql, after the
-- 1.6 image is up or before it boots:
--   psql "$DATABASE_URL" -v ON_ERROR_STOP=1 -f 0267_mcp_oauth_provider.down.sql
-- The legacy oauth_* tables were never touched, so 1.6 finds every client,
-- consent and token it had. Lost on rollback: clients REGISTERED on 1.7 (their
-- grants go with them) and tokens ROTATED on 1.7 (those MCP clients
-- re-authenticate once).
BEGIN;
ALTER TABLE "mcp_grants" DROP CONSTRAINT IF EXISTS "mcp_grants_client_id_oauth2_clients_client_id_fk";
DELETE FROM "mcp_grants" WHERE "client_id" NOT IN (SELECT "client_id" FROM "oauth_applications");
ALTER TABLE "mcp_grants" ADD CONSTRAINT "mcp_grants_client_id_oauth_applications_client_id_fk" FOREIGN KEY ("client_id") REFERENCES "public"."oauth_applications"("client_id") ON DELETE cascade ON UPDATE no action;
DROP TABLE IF EXISTS "oauth2_access_tokens", "oauth2_refresh_tokens", "oauth2_consents", "oauth2_client_resources", "oauth2_resources", "oauth2_client_assertions", "oauth2_clients";
DELETE FROM "drizzle"."__drizzle_migrations" WHERE "created_at" = 1794217069461;
COMMIT;
