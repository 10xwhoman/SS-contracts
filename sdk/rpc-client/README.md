# Soroban RPC client

`@stellarstate/ss-contracts-rpc` is a small TypeScript wrapper around the
official `@stellar/stellar-sdk` RPC client for calling the InvoiceEscrow
contract. Read-only methods use the contract spec through `queryContract`.
Writes take already encoded Soroban `ScVal` arguments so callers control the
exact method arguments and signer.

```ts
import { InvoiceEscrowRpcClient } from "@stellarstate/ss-contracts-rpc";

const client = new InvoiceEscrowRpcClient({
  contractId: process.env.ESCROW_CONTRACT_ID!,
  rpcUrl: "https://soroban-testnet.stellar.org",
});
const escrow = await client.getEscrow("INV001");
```

To submit, encode arguments with `nativeToScVal` from `@stellar/stellar-sdk`
and pass the transaction signer to `client.submit(method, args, keypair)`. For
custom transaction composition, `contractCall` returns a Soroban operation.

`estimateTransactionResources` simulates a transaction and returns its minimum
resource fee and ledger footprint. `simulateContractCall` builds the unsigned
invocation before estimating. A simulation failure is surfaced as an error;
the transaction is never signed or submitted by these helpers.

## Development

```sh
npm ci
npm test
STELLAR_RPC_URL=http://localhost:8000/rpc npm run test:integration
```

The integration check expects Stellar Quickstart's standalone network and
verifies RPC health, network passphrase, and latest-ledger access.
