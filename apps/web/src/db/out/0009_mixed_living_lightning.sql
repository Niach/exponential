CREATE TABLE "github_installation_links" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"workspace_id" uuid NOT NULL,
	"github_installation_id" uuid NOT NULL,
	"created_by_user_id" text,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "github_installation_links_workspace_id_github_installation_id_unique" UNIQUE("workspace_id","github_installation_id")
);
--> statement-breakpoint
ALTER TABLE "repositories" ADD COLUMN "inaccessible_at" timestamp with time zone;--> statement-breakpoint
ALTER TABLE "github_installation_links" ADD CONSTRAINT "github_installation_links_workspace_id_workspaces_id_fk" FOREIGN KEY ("workspace_id") REFERENCES "public"."workspaces"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "github_installation_links" ADD CONSTRAINT "github_installation_links_github_installation_id_github_installations_id_fk" FOREIGN KEY ("github_installation_id") REFERENCES "public"."github_installations"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "github_installation_links" ADD CONSTRAINT "github_installation_links_created_by_user_id_users_id_fk" FOREIGN KEY ("created_by_user_id") REFERENCES "public"."users"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
CREATE INDEX "idx_github_installation_links_installation" ON "github_installation_links" USING btree ("github_installation_id");--> statement-breakpoint
-- Backfill (hand-written): seed workspace↔installation links from actual usage —
-- every workspace already holding repos under an installation gets a link to it.
-- Deliberately NOT seeded from the legacy github_installations.user_id
-- attribution: linking a user's installation into every workspace they own
-- would grant other members browse access to that installation's repos (the
-- exact cross-exposure the links model removes). Unlinked installations are
-- one OAuth claim away from re-linking.
INSERT INTO "github_installation_links" ("workspace_id", "github_installation_id")
SELECT DISTINCT r."workspace_id", gi."id"
FROM "repositories" r
JOIN "github_installations" gi ON gi."installation_id" = r."installation_id"
WHERE r."installation_id" IS NOT NULL
ON CONFLICT DO NOTHING;