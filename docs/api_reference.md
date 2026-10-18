# Invoice Escrow Contract API Reference

Complete technical specification of all public entrypoints, error codes, and parameters for the `invoice-escrow` smart contract.

---

## Functions

### `initialize`
Initializes the contract instance with an admin address.

```rust
pub fn initialize(env: Env, admin: Address);
```
- **Auth:** Requires no auth (can only be invoked once).
- **Errors:** `AlreadyInitialized` (code 1) if invoked multiple times.

### `create_escrow`
Creates a new invoice escrow record.

```rust
pub fn create_escrow(
    env: Env,
    invoice_id: Symbol,
    seller: Address,
    debtor: Address,
    face_value: i128,
    purchase_price: i128,
    due_date: u64,
    payment_token: Address,
    invoice_token: Address,
);
```
- **Auth:** Requires `seller` authorization.
- **Errors:** `EscrowExists` (code 2), `InvalidAmount` (code 3).

### `fund_escrow`
Funds an active escrow with payment tokens and mints invoice tokens to investor.

```rust
pub fn fund_escrow(env: Env, invoice_id: Symbol, investor: Address, amount: i128);
```
- **Auth:** Requires `investor` authorization.
- **Errors:** `EscrowNotFound` (code 4), `EscrowAlreadyFunded` (code 5).

### `set_installment_schedule`
Configures the partial installment settlement milestone schedule for an invoice.

```rust
pub fn set_installment_schedule(
    env: Env,
    invoice_id: Symbol,
    seller: Address,
    schedule: Vec<InstallmentInput>,
);
```
- **Auth:** Requires `seller` authorization (must be the escrow's seller).
- **Preconditions:** Escrow `Created`/`Funded` with `paid_amt == 0`; `1..=64` entries; amounts `> 0` and summing to `face_value`; strictly increasing `due_ts` in `(now, due_dt]`.
- **Errors:** `InvalidInstallmentSchedule` (code 54), `Unauthorized` (code 3), `EscrowNotFound` (code 6), `Paused` (code 15).

---

## References

- Contract implementation: [`contracts/invoice-escrow/src/lib.rs`](../contracts/invoice-escrow/src/lib.rs)
- State machine spec: [`docs/state-machine.md`](state-machine.md)
