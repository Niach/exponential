CREATE TABLE "sidebar_layout" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"user_id" text NOT NULL,
	"team_id" uuid NOT NULL,
	"section" varchar(16) NOT NULL,
	"kind" varchar(16) NOT NULL,
	"target_id" uuid,
	"parent_id" uuid,
	"name" varchar(80),
	"sort_order" double precision DEFAULT 0 NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "sidebar_layout_section_check" CHECK (section IN ('board', 'agent', 'app')),
	CONSTRAINT "sidebar_layout_kind_check" CHECK ((kind = 'folder' AND target_id IS NULL AND parent_id IS NULL) OR (kind = 'item' AND target_id IS NOT NULL))
);
--> statement-breakpoint
ALTER TABLE "sidebar_layout" ADD CONSTRAINT "sidebar_layout_user_id_users_id_fk" FOREIGN KEY ("user_id") REFERENCES "public"."users"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "sidebar_layout" ADD CONSTRAINT "sidebar_layout_team_id_teams_id_fk" FOREIGN KEY ("team_id") REFERENCES "public"."teams"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
CREATE UNIQUE INDEX "uniq_sidebar_layout_user_target" ON "sidebar_layout" USING btree ("user_id","section","target_id") WHERE target_id IS NOT NULL;--> statement-breakpoint
CREATE INDEX "idx_sidebar_layout_user" ON "sidebar_layout" USING btree ("user_id");--> statement-breakpoint
CREATE INDEX "idx_sidebar_layout_team" ON "sidebar_layout" USING btree ("team_id");