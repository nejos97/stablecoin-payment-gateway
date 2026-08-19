-- Per-endpoint event subscriptions. Existing endpoints keep the historical
-- behavior: they only receive the PAID (deposit confirmed) event.
ALTER TABLE "webhook_endpoints"
    ADD COLUMN "events" TEXT[] NOT NULL DEFAULT ARRAY['PAID'];

-- Deliveries become event-scoped. PAID deliveries stay keyed on
-- (depositId, webhookEndpointId); PENDING/EXPIRED deliveries have no deposit
-- and are keyed on (depositAddressId, eventType, webhookEndpointId).
ALTER TABLE "webhook_deliveries"
    ADD COLUMN "eventType" TEXT NOT NULL DEFAULT 'PAID';
ALTER TABLE "webhook_deliveries"
    ADD COLUMN "depositAddressId" TEXT;

UPDATE "webhook_deliveries" w
SET "depositAddressId" = d."depositAddressId"
FROM "deposits" d
WHERE d."id" = w."depositId";

ALTER TABLE "webhook_deliveries"
    ALTER COLUMN "depositAddressId" SET NOT NULL;
ALTER TABLE "webhook_deliveries"
    ALTER COLUMN "depositId" DROP NOT NULL;

ALTER TABLE "webhook_deliveries" ADD CONSTRAINT "webhook_deliveries_depositAddressId_fkey"
    FOREIGN KEY ("depositAddressId") REFERENCES "deposit_addresses"("id") ON DELETE RESTRICT ON UPDATE CASCADE;

CREATE INDEX "webhook_deliveries_address_event_endpoint_idx"
    ON "webhook_deliveries"("depositAddressId", "eventType", "webhookEndpointId");
