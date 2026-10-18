import test from "node:test";
import assert from "node:assert/strict";
import {
  CONTRACT_ERRORS,
  contractErrorsForCode,
  isKnownContractError,
} from "../dist/index.js";
import schema from "../schemas/contract-errors.schema.json" with { type: "json" };

test("exports every contract error enum from the Rust sources", () => {
  assert.equal(Object.keys(CONTRACT_ERRORS["invoice-escrow"]).length, 60);
  assert.equal(Object.keys(CONTRACT_ERRORS["invoice-token"]).length, 21);
  assert.equal(Object.keys(CONTRACT_ERRORS["payment-distributor"]).length, 28);
});

test("maps known codes per contract and keeps code values unique within each contract", () => {
  assert.equal(isKnownContractError("invoice-token", 13), true);
  assert.equal(isKnownContractError("invoice-token", 99), false);
  assert.equal(isKnownContractError("unknown-contract", 1), false);
  assert.deepEqual(contractErrorsForCode("invoice-escrow", 55), ["FeeTooHigh"]);
  assert.deepEqual(contractErrorsForCode("invoice-escrow", 57), ["MaxInvestorsReached"]);
});

test("JSON-RPC schema includes a contract-specific variant for every error", () => {
  const contractErrorSchema = schema.$defs.ContractError;
  assert.equal(contractErrorSchema.oneOf.length, 109);
});
