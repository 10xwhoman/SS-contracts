import assert from "node:assert/strict";
import test from "node:test";
import { SorobanDataBuilder } from "@stellar/stellar-sdk";
import { estimateTransactionResources } from "../dist/index.js";

test("returns simulated fees and host resource estimates", async () => {
  const transactionData = new SorobanDataBuilder().setResources(1234, 2048, 512);
  const server = {
    simulateTransaction: async () => ({
      _parsed: true,
      id: "1",
      latestLedger: 42,
      events: [],
      transactionData,
      minResourceFee: "987",
    }),
  };
  const estimate = await estimateTransactionResources(server);
  assert.equal(estimate.minResourceFee, "987");
  assert.equal(estimate.latestLedger, 42);
  assert.equal(estimate.instructions, 1234);
  assert.equal(estimate.diskReadBytes, 2048);
  assert.equal(estimate.writeBytes, 512);
  assert.equal(estimate.readOnlyLedgerKeys, 0);
  assert.equal(estimate.readWriteLedgerKeys, 0);
});

test("reports simulation errors to the caller", async () => {
  const server = {
    simulateTransaction: async () => ({
      _parsed: true,
      id: "1",
      latestLedger: 42,
      events: [],
      error: "contract trap",
    }),
  };
  await assert.rejects(
    estimateTransactionResources(server),
    /Soroban simulation failed: contract trap/,
  );
});
