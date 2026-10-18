import test from "node:test";
import assert from "node:assert/strict";
import { rpc } from "@stellar/stellar-sdk";

const endpoint = process.env.STELLAR_RPC_URL;

test("connects to a standalone Soroban RPC instance", { skip: !endpoint }, async () => {
  const server = new rpc.Server(endpoint, { allowHttp: endpoint.startsWith("http://") });
  const [health, network, latestLedger] = await Promise.all([
    server.getHealth(),
    server.getNetwork(),
    server.getLatestLedger(),
  ]);
  assert.equal(health.status, "healthy");
  assert.equal(network.passphrase, "Standalone Network ; February 2017");
  assert.ok(latestLedger.sequence > 0);
});
