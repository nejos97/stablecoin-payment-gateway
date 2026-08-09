CREATE TYPE "Network" AS ENUM ('TRON', 'SOLANA', 'ETHEREUM');
CREATE TYPE "Token" AS ENUM ('USDT');
CREATE TYPE "DepositAddressStatus" AS ENUM ('PENDING', 'PAID', 'EXPIRED');
CREATE TYPE "DepositStatus" AS ENUM ('DETECTED', 'CONFIRMED');
CREATE TYPE "WebhookDeliveryStatus" AS ENUM ('PENDING', 'DELIVERED', 'FAILED');

CREATE TABLE "deposit_addresses" (
    "id" TEXT NOT NULL,
    "network" "Network" NOT NULL,
    "token" "Token" NOT NULL,
    "address" TEXT NOT NULL,
    "derivationIndex" INTEGER NOT NULL,
    "expectedAmount" DECIMAL(18,6) NOT NULL,
    "status" "DepositAddressStatus" NOT NULL DEFAULT 'PENDING',
    "expiresAt" TIMESTAMP(3) NOT NULL,
    "metadata" JSONB NOT NULL DEFAULT '{}',
    "createdAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,
    "updatedAt" TIMESTAMP(3) NOT NULL,
    CONSTRAINT "deposit_addresses_pkey" PRIMARY KEY ("id")
);

CREATE TABLE "deposits" (
    "id" TEXT NOT NULL,
    "depositAddressId" TEXT NOT NULL,
    "txHash" TEXT NOT NULL,
    "amount" DECIMAL(18,6) NOT NULL,
    "amountRaw" TEXT NOT NULL,
    "confirmations" INTEGER NOT NULL DEFAULT 0,
    "status" "DepositStatus" NOT NULL DEFAULT 'DETECTED',
    "detectedAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,
    "confirmedAt" TIMESTAMP(3),
    CONSTRAINT "deposits_pkey" PRIMARY KEY ("id")
);

CREATE TABLE "webhook_deliveries" (
    "id" TEXT NOT NULL,
    "depositId" TEXT NOT NULL,
    "status" "WebhookDeliveryStatus" NOT NULL DEFAULT 'PENDING',
    "attempts" INTEGER NOT NULL DEFAULT 0,
    "lastResponse" INTEGER,
    "lastError" TEXT,
    "createdAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,
    "updatedAt" TIMESTAMP(3) NOT NULL,
    CONSTRAINT "webhook_deliveries_pkey" PRIMARY KEY ("id")
);

CREATE TABLE "chain_cursors" (
    "network" "Network" NOT NULL,
    "lastBlockOrSlot" TEXT NOT NULL,
    "updatedAt" TIMESTAMP(3) NOT NULL,
    CONSTRAINT "chain_cursors_pkey" PRIMARY KEY ("network")
);

CREATE UNIQUE INDEX "deposit_addresses_address_key" ON "deposit_addresses"("address");
CREATE INDEX "deposit_addresses_network_status_idx" ON "deposit_addresses"("network", "status");
CREATE INDEX "deposit_addresses_status_expiresAt_idx" ON "deposit_addresses"("status", "expiresAt");
CREATE UNIQUE INDEX "deposits_txHash_key" ON "deposits"("txHash");
CREATE INDEX "deposits_depositAddressId_idx" ON "deposits"("depositAddressId");
CREATE INDEX "webhook_deliveries_depositId_idx" ON "webhook_deliveries"("depositId");

ALTER TABLE "deposits" ADD CONSTRAINT "deposits_depositAddressId_fkey"
    FOREIGN KEY ("depositAddressId") REFERENCES "deposit_addresses"("id") ON DELETE RESTRICT ON UPDATE CASCADE;
ALTER TABLE "webhook_deliveries" ADD CONSTRAINT "webhook_deliveries_depositId_fkey"
    FOREIGN KEY ("depositId") REFERENCES "deposits"("id") ON DELETE RESTRICT ON UPDATE CASCADE;
