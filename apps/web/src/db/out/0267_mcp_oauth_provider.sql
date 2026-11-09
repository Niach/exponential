CREATE TABLE "oauth2_access_tokens" (
	"id" text PRIMARY KEY NOT NULL,
	"token" text NOT NULL,
	"client_id" text NOT NULL,
	"session_id" text,
	"user_id" text,
	"reference_id" text,
	"authorization_code_id" text,
	"resources" text[],
	"requested_user_info_claims" text[],
	"refresh_id" text,
	"expires_at" timestamp NOT NULL,
	"created_at" timestamp NOT NULL,
	"revoked" timestamp,
	"confirmation" jsonb,
	"scopes" text[] NOT NULL,
	CONSTRAINT "oauth2_access_tokens_token_unique" UNIQUE("token")
);
--> statement-breakpoint
CREATE TABLE "oauth2_client_assertions" (
	"id" text PRIMARY KEY NOT NULL,
	"expires_at" timestamp NOT NULL
);
--> statement-breakpoint
CREATE TABLE "oauth2_client_resources" (
	"id" text PRIMARY KEY NOT NULL,
	"client_id" text NOT NULL,
	"resource_id" text NOT NULL,
	"metadata" jsonb,
	"created_at" timestamp
);
--> statement-breakpoint
CREATE TABLE "oauth2_clients" (
	"id" text PRIMARY KEY NOT NULL,
	"client_id" text NOT NULL,
	"client_secret" text,
	"client_discovery_id" text,
	"disabled" boolean DEFAULT false,
	"skip_consent" boolean,
	"enable_end_session" boolean,
	"subject_type" text,
	"scopes" text[],
	"client_credentials_scopes" text[],
	"user_id" text,
	"created_at" timestamp,
	"updated_at" timestamp,
	"name" text,
	"uri" text,
	"icon" text,
	"contacts" text[],
	"tos" text,
	"policy" text,
	"software_id" text,
	"software_version" text,
	"software_statement" text,
	"redirect_uris" text[] NOT NULL,
	"post_logout_redirect_uris" text[],
	"backchannel_logout_uri" text,
	"backchannel_logout_session_required" boolean,
	"token_endpoint_auth_method" text,
	"application_type" text,
	"jwks" text,
	"jwks_uri" text,
	"grant_types" text[],
	"response_types" text[],
	"require_pkce" boolean,
	"dpop_bound_access_tokens" boolean DEFAULT false,
	"reference_id" text,
	"metadata" jsonb,
	CONSTRAINT "oauth2_clients_client_id_unique" UNIQUE("client_id")
);
--> statement-breakpoint
CREATE TABLE "oauth2_consents" (
	"id" text PRIMARY KEY NOT NULL,
	"client_id" text NOT NULL,
	"user_id" text,
	"reference_id" text,
	"resources" text[],
	"requested_user_info_claims" text[],
	"scopes" text[] NOT NULL,
	"created_at" timestamp NOT NULL,
	"updated_at" timestamp NOT NULL
);
--> statement-breakpoint
CREATE TABLE "oauth2_refresh_tokens" (
	"id" text PRIMARY KEY NOT NULL,
	"token" text NOT NULL,
	"client_id" text NOT NULL,
	"session_id" text,
	"user_id" text NOT NULL,
	"reference_id" text,
	"authorization_code_id" text,
	"resources" text[],
	"requested_user_info_claims" text[],
	"expires_at" timestamp NOT NULL,
	"created_at" timestamp NOT NULL,
	"revoked" timestamp,
	"rotated_at" timestamp,
	"rotation_replay_response" text,
	"rotation_replay_expires_at" timestamp,
	"auth_time" timestamp,
	"confirmation" jsonb,
	"scopes" text[] NOT NULL,
	CONSTRAINT "oauth2_refresh_tokens_token_unique" UNIQUE("token")
);
--> statement-breakpoint
CREATE TABLE "oauth2_resources" (
	"id" text PRIMARY KEY NOT NULL,
	"identifier" text NOT NULL,
	"name" text NOT NULL,
	"access_token_ttl" integer,
	"refresh_token_ttl" integer,
	"signing_algorithm" text,
	"signing_key_id" text,
	"allowed_scopes" text[],
	"custom_claims" jsonb,
	"dpop_bound_access_tokens_required" boolean DEFAULT false,
	"disabled" boolean DEFAULT false,
	"created_at" timestamp,
	"updated_at" timestamp,
	"policy_version" integer DEFAULT 1,
	"metadata" jsonb,
	CONSTRAINT "oauth2_resources_identifier_unique" UNIQUE("identifier")
);
--> statement-breakpoint
ALTER TABLE "mcp_grants" DROP CONSTRAINT "mcp_grants_client_id_oauth_applications_client_id_fk";
--> statement-breakpoint
ALTER TABLE "oauth2_access_tokens" ADD CONSTRAINT "oauth2_access_tokens_client_id_oauth2_clients_client_id_fk" FOREIGN KEY ("client_id") REFERENCES "public"."oauth2_clients"("client_id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "oauth2_access_tokens" ADD CONSTRAINT "oauth2_access_tokens_session_id_auth_sessions_id_fk" FOREIGN KEY ("session_id") REFERENCES "public"."auth_sessions"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "oauth2_access_tokens" ADD CONSTRAINT "oauth2_access_tokens_user_id_users_id_fk" FOREIGN KEY ("user_id") REFERENCES "public"."users"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "oauth2_access_tokens" ADD CONSTRAINT "oauth2_access_tokens_refresh_id_oauth2_refresh_tokens_id_fk" FOREIGN KEY ("refresh_id") REFERENCES "public"."oauth2_refresh_tokens"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "oauth2_client_resources" ADD CONSTRAINT "oauth2_client_resources_client_id_oauth2_clients_client_id_fk" FOREIGN KEY ("client_id") REFERENCES "public"."oauth2_clients"("client_id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "oauth2_client_resources" ADD CONSTRAINT "oauth2_client_resources_resource_id_oauth2_resources_identifier_fk" FOREIGN KEY ("resource_id") REFERENCES "public"."oauth2_resources"("identifier") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "oauth2_clients" ADD CONSTRAINT "oauth2_clients_user_id_users_id_fk" FOREIGN KEY ("user_id") REFERENCES "public"."users"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "oauth2_consents" ADD CONSTRAINT "oauth2_consents_client_id_oauth2_clients_client_id_fk" FOREIGN KEY ("client_id") REFERENCES "public"."oauth2_clients"("client_id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "oauth2_consents" ADD CONSTRAINT "oauth2_consents_user_id_users_id_fk" FOREIGN KEY ("user_id") REFERENCES "public"."users"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "oauth2_refresh_tokens" ADD CONSTRAINT "oauth2_refresh_tokens_client_id_oauth2_clients_client_id_fk" FOREIGN KEY ("client_id") REFERENCES "public"."oauth2_clients"("client_id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "oauth2_refresh_tokens" ADD CONSTRAINT "oauth2_refresh_tokens_session_id_auth_sessions_id_fk" FOREIGN KEY ("session_id") REFERENCES "public"."auth_sessions"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "oauth2_refresh_tokens" ADD CONSTRAINT "oauth2_refresh_tokens_user_id_users_id_fk" FOREIGN KEY ("user_id") REFERENCES "public"."users"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
CREATE INDEX "oauth2_access_tokens_client_id_idx" ON "oauth2_access_tokens" USING btree ("client_id");--> statement-breakpoint
CREATE INDEX "oauth2_access_tokens_session_id_idx" ON "oauth2_access_tokens" USING btree ("session_id");--> statement-breakpoint
CREATE INDEX "oauth2_access_tokens_user_id_idx" ON "oauth2_access_tokens" USING btree ("user_id");--> statement-breakpoint
CREATE INDEX "oauth2_access_tokens_authorization_code_id_idx" ON "oauth2_access_tokens" USING btree ("authorization_code_id");--> statement-breakpoint
CREATE INDEX "oauth2_access_tokens_refresh_id_idx" ON "oauth2_access_tokens" USING btree ("refresh_id");--> statement-breakpoint
CREATE UNIQUE INDEX "oauth2_client_resources_client_resource_uidx" ON "oauth2_client_resources" USING btree ("client_id","resource_id");--> statement-breakpoint
CREATE INDEX "oauth2_client_resources_resource_id_idx" ON "oauth2_client_resources" USING btree ("resource_id");--> statement-breakpoint
CREATE INDEX "oauth2_clients_user_id_idx" ON "oauth2_clients" USING btree ("user_id");--> statement-breakpoint
CREATE INDEX "oauth2_consents_client_id_idx" ON "oauth2_consents" USING btree ("client_id");--> statement-breakpoint
CREATE INDEX "oauth2_consents_user_id_idx" ON "oauth2_consents" USING btree ("user_id");--> statement-breakpoint
CREATE INDEX "oauth2_refresh_tokens_client_id_idx" ON "oauth2_refresh_tokens" USING btree ("client_id");--> statement-breakpoint
CREATE INDEX "oauth2_refresh_tokens_session_id_idx" ON "oauth2_refresh_tokens" USING btree ("session_id");--> statement-breakpoint
CREATE INDEX "oauth2_refresh_tokens_user_id_idx" ON "oauth2_refresh_tokens" USING btree ("user_id");--> statement-breakpoint
CREATE INDEX "oauth2_refresh_tokens_authorization_code_id_idx" ON "oauth2_refresh_tokens" USING btree ("authorization_code_id");--> statement-breakpoint
-- better-auth 1.7: the 1.6 `mcp` plugin's clients move to oauth-provider's
-- table so mcp_grants can point at it (the legacy tables stay untouched for a
-- rollback). Secrets stay NULL here: oauth-provider stores them encrypted
-- with the auth secret, which only the server holds; the boot-time import
-- (lib/auth/legacy-mcp-oauth.ts) fills them and copies consents and tokens
-- (hashed). Public clients registered with token_endpoint_auth_method=none
-- (the 1.6 plugin stored type 'public' and an empty secret).
INSERT INTO "oauth2_clients" ("id", "client_id", "client_secret", "disabled", "skip_consent", "user_id", "created_at", "updated_at", "name", "icon", "redirect_uris", "token_endpoint_auth_method", "grant_types", "response_types", "require_pkce")
SELECT "id", "client_id", NULL, "disabled", false, "user_id", "created_at", "updated_at", "name", "icon",
  string_to_array("redirect_urls", ','),
  CASE WHEN "type" = 'public' THEN 'none' ELSE 'client_secret_basic' END,
  ARRAY['authorization_code', 'refresh_token'], ARRAY['code'], false
FROM "oauth_applications"
ON CONFLICT ("client_id") DO NOTHING;--> statement-breakpoint
-- Consents and live tokens, ONCE (a boot-time re-import would resurrect a
-- token revoked or rotated on 1.7). Tokens are stored as oauth-provider
-- stores them: SHA-256 of the token, base64url without padding
-- (lib/auth/oauth-token-hash.ts; the real-DB test pins the equality). Each
-- 1.6 row held an access + refresh pair: the refresh row keeps the legacy
-- id, the access row points at it.
INSERT INTO "oauth2_consents" ("id", "client_id", "user_id", "scopes", "created_at", "updated_at")
SELECT c."id", c."client_id", c."user_id", string_to_array(c."scopes", ' '), c."created_at", c."updated_at"
FROM "oauth_consents" c
JOIN "oauth2_clients" n ON n."client_id" = c."client_id"
WHERE c."consent_given"
ON CONFLICT ("id") DO NOTHING;--> statement-breakpoint
INSERT INTO "oauth2_refresh_tokens" ("id", "token", "client_id", "user_id", "expires_at", "created_at", "scopes")
SELECT t."id", rtrim(translate(encode(sha256(convert_to(t."refresh_token", 'UTF8')), 'base64'), '+/', '-_'), '='),
  t."client_id", t."user_id", t."refresh_token_expires_at", t."created_at", string_to_array(t."scopes", ' ')
FROM "oauth_access_tokens" t
JOIN "oauth2_clients" n ON n."client_id" = t."client_id"
WHERE t."user_id" IS NOT NULL AND t."refresh_token_expires_at" > now()
ON CONFLICT DO NOTHING;--> statement-breakpoint
INSERT INTO "oauth2_access_tokens" ("id", "token", "client_id", "user_id", "refresh_id", "expires_at", "created_at", "scopes")
SELECT t."id", rtrim(translate(encode(sha256(convert_to(t."access_token", 'UTF8')), 'base64'), '+/', '-_'), '='),
  t."client_id", t."user_id", t."id", t."access_token_expires_at", t."created_at", string_to_array(t."scopes", ' ')
FROM "oauth_access_tokens" t
JOIN "oauth2_refresh_tokens" r ON r."id" = t."id"
WHERE t."access_token_expires_at" > now()
ON CONFLICT DO NOTHING;--> statement-breakpoint
ALTER TABLE "mcp_grants" ADD CONSTRAINT "mcp_grants_client_id_oauth2_clients_client_id_fk" FOREIGN KEY ("client_id") REFERENCES "public"."oauth2_clients"("client_id") ON DELETE cascade ON UPDATE no action;