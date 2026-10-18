# StellarSettle TypeScript contract error SDK

This package exports contract-specific error codes, TypeScript types, and a JSON Schema for JSON-RPC error responses containing decoded Soroban contract error data.

```ts
import {
  CONTRACT_ERRORS,
  contractErrorsForCode,
  isKnownContractError,
} from "@stellarstate/ss-contracts-sdk";
import type { ContractError } from "@stellarstate/ss-contracts-sdk";

const pausedCode = CONTRACT_ERRORS["invoice-escrow"].Paused;
const names = contractErrorsForCode("invoice-escrow", 55);
const known = isKnownContractError("payment-distributor", 11);
```

The generated `ContractError` type keeps the contract, name, and numeric code together. Error codes are scoped to each contract. The generator validates the enum entries directly from the Rust sources; `invoice-escrow`'s `MaxInvestorsReached` code is `57` so it does not collide with `FeeTooHigh` (`55`).

The schema is available at `@stellarstate/ss-contracts-sdk/schemas/contract-errors.json`. Its `error.data` payload uses `{ "contract": "...", "name": "...", "code": 123 }`.

The exports are generated from the `#[contracterror]` enums in the three contract crates. After changing an error enum, run `npm run generate` from this directory and include both generated files in the change.

## Development

```sh
npm install
npm test
```
