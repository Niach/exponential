CREATE TABLE "app_record_events" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"seq" bigint GENERATED ALWAYS AS IDENTITY (sequence name "app_record_events_seq_seq" INCREMENT BY 1 MINVALUE 1 MAXVALUE 9223372036854775807 START WITH 1 CACHE 1),
	"record_id" uuid NOT NULL,
	"app_id" uuid NOT NULL,
	"team_id" uuid NOT NULL,
	"kind" varchar(32) NOT NULL,
	"op" varchar(16) NOT NULL,
	"actor" text,
	"author" text,
	"via" varchar(16) NOT NULL,
	"patch" jsonb,
	"before" jsonb,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL
);
--> statement-breakpoint
ALTER TABLE "app_record_events" ADD CONSTRAINT "app_record_events_record_id_app_records_id_fk" FOREIGN KEY ("record_id") REFERENCES "public"."app_records"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "app_record_events" ADD CONSTRAINT "app_record_events_app_id_apps_id_fk" FOREIGN KEY ("app_id") REFERENCES "public"."apps"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "app_record_events" ADD CONSTRAINT "app_record_events_team_id_teams_id_fk" FOREIGN KEY ("team_id") REFERENCES "public"."teams"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "app_record_events" ADD CONSTRAINT "app_record_events_actor_users_id_fk" FOREIGN KEY ("actor") REFERENCES "public"."users"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "app_record_events" ADD CONSTRAINT "app_record_events_author_users_id_fk" FOREIGN KEY ("author") REFERENCES "public"."users"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
CREATE INDEX "idx_app_record_events_record" ON "app_record_events" USING btree ("record_id","created_at");--> statement-breakpoint
CREATE INDEX "idx_app_record_events_created" ON "app_record_events" USING btree ("created_at");