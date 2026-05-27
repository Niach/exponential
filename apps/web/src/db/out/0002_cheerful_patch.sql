CREATE TABLE "creem_subscriptions" (
	"id" text PRIMARY KEY NOT NULL,
	"product_id" text NOT NULL,
	"reference_id" text NOT NULL,
	"creem_customer_id" text,
	"creem_subscription_id" text,
	"creem_order_id" text,
	"status" text NOT NULL,
	"period_start" timestamp,
	"period_end" timestamp,
	"cancel_at_period_end" boolean NOT NULL,
	"created_at" timestamp NOT NULL,
	"updated_at" timestamp NOT NULL
);
--> statement-breakpoint
ALTER TABLE "users" ADD COLUMN "creem_customer_id" text;--> statement-breakpoint
ALTER TABLE "users" ADD COLUMN "had_trial" boolean DEFAULT false NOT NULL;--> statement-breakpoint
ALTER TABLE "creem_subscriptions" ADD CONSTRAINT "creem_subscriptions_reference_id_users_id_fk" FOREIGN KEY ("reference_id") REFERENCES "public"."users"("id") ON DELETE cascade ON UPDATE no action;