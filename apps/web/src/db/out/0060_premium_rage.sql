CREATE TABLE "device_codes" (
	"id" text PRIMARY KEY NOT NULL,
	"device_code" text NOT NULL,
	"user_code" text NOT NULL,
	"user_id" text,
	"expires_at" timestamp NOT NULL,
	"status" text NOT NULL,
	"last_polled_at" timestamp,
	"polling_interval" integer,
	"client_id" text,
	"scope" text
);
--> statement-breakpoint
ALTER TABLE "device_codes" ADD CONSTRAINT "device_codes_user_id_users_id_fk" FOREIGN KEY ("user_id") REFERENCES "public"."users"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
CREATE INDEX "device_codes_device_code_idx" ON "device_codes" USING btree ("device_code");--> statement-breakpoint
CREATE INDEX "device_codes_user_code_idx" ON "device_codes" USING btree ("user_code");--> statement-breakpoint
CREATE INDEX "device_codes_user_id_idx" ON "device_codes" USING btree ("user_id");