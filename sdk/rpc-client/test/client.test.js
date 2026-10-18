import assert from "node:assert/strict";
import test from "node:test";
import { InvoiceEscrowRpcClient } from "../dist/index.js";

test("constructs a client with testnet as the default passphrase", () => {
  const client = new InvoiceEscrowRpcClient({
    contractId: "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAHK3M",
    rpcUrl: "http://localhost:8000/rpc",
  });
  assert.equal(client.networkPassphrase, "Test SDF Network ; September 2015");
  assert.equal(client.contractId, "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAHK3M");
});

test("allows an explicit network passphrase", () => {
  const client = new InvoiceEscrowRpcClient({
    contractId: "contract",
    rpcUrl: "https://rpc.example.test",
    networkPassphrase: "Standalone Network ; February 2017",
  });
  assert.equal(client.networkPassphrase, "Standalone Network ; February 2017");
});
