CREATE TABLE "conversion_events" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"user_id" text,
	"anonymous_id" varchar(64),
	"name" varchar(64) NOT NULL,
	"properties" jsonb,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL
);
--> statement-breakpoint
ALTER TABLE "users" ADD COLUMN "signup_ref" text;--> statement-breakpoint
ALTER TABLE "users" ADD COLUMN "signup_utm_source" text;--> statement-breakpoint
ALTER TABLE "users" ADD COLUMN "signup_utm_medium" text;--> statement-breakpoint
ALTER TABLE "users" ADD COLUMN "signup_utm_campaign" text;--> statement-breakpoint
ALTER TABLE "users" ADD COLUMN "signup_referrer" text;--> statement-breakpoint
ALTER TABLE "users" ADD COLUMN "signup_landing_path" text;--> statement-breakpoint
ALTER TABLE "users" ADD COLUMN "signup_anonymous_id" text;--> statement-breakpoint
ALTER TABLE "conversion_events" ADD CONSTRAINT "conversion_events_user_id_users_id_fk" FOREIGN KEY ("user_id") REFERENCES "public"."users"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
CREATE INDEX "idx_conversion_events_name_created" ON "conversion_events" USING btree ("name","created_at");--> statement-breakpoint
CREATE INDEX "idx_conversion_events_user" ON "conversion_events" USING btree ("user_id");--> statement-breakpoint
CREATE INDEX "idx_conversion_events_anon" ON "conversion_events" USING btree ("anonymous_id");--> statement-breakpoint
CREATE UNIQUE INDEX "uniq_conversion_events_once_per_user" ON "conversion_events" USING btree ("user_id","name") WHERE name in ('signup', 'first_issue_created');--> statement-breakpoint
CREATE UNIQUE INDEX "uniq_conversion_events_once_per_sub" ON "conversion_events" USING btree ("name",(properties->>'creemSubscriptionId')) WHERE name in ('subscription_first_active', 'subscription_canceled');--> statement-breakpoint
CREATE UNIQUE INDEX "uniq_conversion_events_landing_daily" ON "conversion_events" USING btree ("name","anonymous_id") WHERE name = 'landing';--> statement-breakpoint
CREATE INDEX "idx_users_signup_anonymous_id" ON "users" USING btree ("signup_anonymous_id");