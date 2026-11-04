CREATE TABLE "app_share_links" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"app_id" uuid NOT NULL,
	"team_id" uuid NOT NULL,
	"name" varchar(24) NOT NULL,
	"token" varchar(64) NOT NULL,
	"enabled_by" text,
	"expires_at" timestamp with time zone,
	"revoked_at" timestamp with time zone,
	"rotated_at" timestamp with time zone,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "app_share_links_app_name_unique" UNIQUE("app_id","name"),
	CONSTRAINT "app_share_links_token_unique" UNIQUE("token")
);
--> statement-breakpoint
CREATE TABLE "app_share_submissions" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"link_id" uuid NOT NULL,
	"app_id" uuid NOT NULL,
	"team_id" uuid NOT NULL,
	"record_id" uuid,
	"ip_hash" varchar(64) NOT NULL,
	"outcome" varchar(16) NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL
);
--> statement-breakpoint
ALTER TABLE "app_record_events" ALTER COLUMN "via" SET DATA TYPE varchar(48);--> statement-breakpoint
ALTER TABLE "app_records" ADD COLUMN "share_name" varchar(24);--> statement-breakpoint
ALTER TABLE "app_share_links" ADD CONSTRAINT "app_share_links_app_id_apps_id_fk" FOREIGN KEY ("app_id") REFERENCES "public"."apps"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "app_share_links" ADD CONSTRAINT "app_share_links_team_id_teams_id_fk" FOREIGN KEY ("team_id") REFERENCES "public"."teams"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "app_share_links" ADD CONSTRAINT "app_share_links_enabled_by_users_id_fk" FOREIGN KEY ("enabled_by") REFERENCES "public"."users"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "app_share_submissions" ADD CONSTRAINT "app_share_submissions_link_id_app_share_links_id_fk" FOREIGN KEY ("link_id") REFERENCES "public"."app_share_links"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "app_share_submissions" ADD CONSTRAINT "app_share_submissions_app_id_apps_id_fk" FOREIGN KEY ("app_id") REFERENCES "public"."apps"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "app_share_submissions" ADD CONSTRAINT "app_share_submissions_team_id_teams_id_fk" FOREIGN KEY ("team_id") REFERENCES "public"."teams"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
CREATE INDEX "app_share_submissions_link_created_idx" ON "app_share_submissions" USING btree ("link_id","created_at");--> statement-breakpoint
CREATE INDEX "app_share_submissions_link_ip_idx" ON "app_share_submissions" USING btree ("link_id","ip_hash","created_at");