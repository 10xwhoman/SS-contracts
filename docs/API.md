# StellarSettle API Reference

This document summarizes the public functions exposed by the StellarSettle Soroban contracts.

## Payment Distributor Contract

The `payment-distributor` contract now implements the settlement/refund fan-out flow used by `invoice-escrow` when a distributor is configured.

### `initialize(admin: Address)`
- Sets the distributor admin.

### `distribute_payment(escrow_contract: Address, invoice_id: Symbol, addresses: Vec<Address>, amounts: Vec<i128>, escrow_status: u32)`
- Callable only by the authenticated escrow contract.
- `addresses` must be `[token, seller, funder, admin]`.
- `amounts` must be `[paid_amount, seller_amount, investor_amount, platform_fee]`.
- `escrow_status` must represent `Funded` or `Settled`.
- Uses `paid_amount` plus internal distribution state to prevent replay or double distribution.
- Transfers funds held by `payment-distributor` to the seller, funder, and admin.

### `distribute_refund(escrow_contract: Address, invoice_id: Symbol, addresses: Vec<Address>, amounts: Vec<i128>, escrow_status: u32)`
- Callable only by the authenticated escrow contract.
- `addresses` must be `[token, funder]`.
- `amounts` must be `[refund_amount]`.
- `escrow_status` must represent `Refunded`.
- Refund distribution is one-time per `(escrow_contract, invoice_id)`.

### `get_distribution_state(escrow_contract: Address, invoice_id: Symbol) -> DistributionState`
- Returns the tracked payout progress for an escrow invoice.
- `paid_distributed` is the cumulative `paid_amount` already fanned out.
- `refund_distributed` indicates whether the refund leg has already been processed.

### `get_admin() -> Address`
- Returns the distributor admin.

### Events
- `payment_distributed`
- `refund_distributed`
- `initialized`

## Invoice Token Contract

`invoice-token` is the invoice ownership token contract with transfer-lock and pause controls.

### Core Functions
- `initialize(admin, name, symbol, decimals, invoice_id, minter)`
- `transfer(from, to, amount)`
- `approve(from, spender, amount, expiration_ledger)`
- `transfer_from(spender, from, to, amount)`
- `burn(from, amount)`
- `burn_from(spender, from, amount)`
- `mint(to, amount, by)`
- `set_transfer_locked(caller, locked)`
- `set_minter(new_minter)`
- `set_paused(paused)`
- `paused() -> bool`
- `transfer_locked() -> bool`

### Pause Policy
- When paused, the contract rejects `transfer`, `approve`, `transfer_from`, `burn`, `burn_from`, and `mint`.
- Admin-only config operations such as `set_transfer_locked`, `set_minter`, and `set_paused` remain available.

### Events
- `transfer`
- `approve`
- `mint`
- `burn`
- `transfer_locked_updated`
- `minter_updated`
- `paused_updated`

## Invoice Escrow Contract

`invoice-escrow` remains the lifecycle orchestrator and now optionally routes settlement/refund payouts through `payment-distributor`.

### Core Functions
- `initialize(admin: Address, platform_fee_bps: u32)`
- `create_escrow(..., accepted_tokens)`
- `cancel_escrow(invoice_id, seller)`
- `fund_escrow(invoice_id, buyer, amount, funding_token)`
- `record_payment(invoice_id, payer, amount)`
- `refund(invoice_id)`
- `set_installment_schedule(invoice_id, seller, schedule)`
- `get_installment_schedule(invoice_id) -> Vec<InstallmentMilestone>`
- `get_next_installment(invoice_id) -> Option<InstallmentMilestone>`
- `update_platform_fee_bps(new_fee_bps)`
- `set_payment_distributor(payment_distributor)`
- `set_paused(paused)`
- `get_escrow(invoice_id) -> EscrowData`
- `get_config() -> Config`
- `get_escrow_status(invoice_id) -> EscrowStatus`
- `paused() -> bool`

### Distribution Behavior
- If `payment_distributor` is configured, `record_payment` transfers the current settlement funds into the distributor and invokes `distribute_payment`.
- If `payment_distributor` is configured, `refund` transfers the remaining collateral into the distributor and invokes `distribute_refund`.
- If no distributor is configured, escrow falls back to the legacy direct-transfer path.
### `initialize(admin: Address, platform_fee_bps: u32)`
### `create_escrow(invoice_id: Symbol, seller: Address, debtor: Address, face_value: i128, purchase_price: i128, due_date: u64, payment_token: Address, invoice_token: Address, commitment: BytesN<32>, funding_milestone: Option<i128>, category: Option<InvoiceCategory>, accepted_tokens: Vec<Address>)`
Creates an escrow for an invoice with the specified parameters.
- **invoice_id**: Unique identifier for the invoice.
- **seller**: Address of the invoice seller (creator of the escrow).
- **debtor**: Address of the party responsible for paying the invoice.
- **face_value**: Total amount owed by the debtor (must be > 0).
- **purchase_price**: Amount the investor will pay to fund the escrow (must be > 0).
- **due_date**: Unix timestamp when the invoice is due (must be > 0 and > current ledger timestamp).
- **payment_token**: Address of the token used for payments.
- **invoice_token**: Address of the invoice token contract.
- **commitment**: SHA-256 hash of off-chain invoice data (immutable anchor).
- **accepted_tokens**: Non-empty set of payment tokens that may fund the escrow; it must include `payment_token`.

The first funding transaction selects and locks the escrow's payment token. Later funders must use that same token. `create_escrow_legacy` and `fund_escrow_legacy` preserve the former single-token behavior.

**Constraints:**
- face_value and purchase_price must be positive (> 0)
- due_date must be non-zero and strictly greater than the current ledger timestamp
- Each invoice_id can only be used once
### `fund_escrow(invoice_id: Symbol, buyer: Address, amount: i128, funding_token: Address)`
Funds the escrow with an accepted token. Once a contribution succeeds, every later funding, settlement, and refund uses that locked token.

### Two-admin registered-invoice settlement
`propose_settlement(admin, invoice_id, repayment_amount)` records a proposal from an admin in the configured emergency admin set. A different admin calls `approve_settlement(admin, invoice_id)` to execute the pro-rata investor settlement. The proposing admin receives `SameAdminApproval` if they try to approve their own proposal; successful execution clears the proposal.

The legacy `settle_invoice(invoice_id, repayment_amount)` now creates a proposal on behalf of the configured contract admin and still requires a second admin's approval.
### `record_payment(invoice_id: Symbol, payer: Address, amount: i128)`
Records a full or partial payment for a funded invoice.
- **amount**: Must be $> 0$ and $\le$ (initial amount - total already paid).
- Partial payments distribute the platform fee to the admin, the remainder to the investor, and release a proportional amount of the investor's initial funding to the seller.
- The invoice status transitions to `Settled` only when the total paid matches the invoice amount.
- If an installment schedule is configured, every milestone whose cumulative target is reached by the new `paid_amt` is marked settled (emitting `installment_settled`). When the escrow reaches terminal `Settled`, any remaining milestones are settled as well (covers early-settlement discounts and emergency releases).

### `set_installment_schedule(invoice_id: Symbol, seller: Address, schedule: Vec<InstallmentInput>)`
Configures (or replaces) the partial installment settlement milestone schedule for an invoice (#450).
- **Auth:** Requires `seller` authorization (must match the escrow's seller).
- **Preconditions:** Escrow status is `Created` or `Funded`, `paid_amt == 0`, contract not paused.
- **schedule:** `1..=64` entries, each `{ due_ts: u64, amount: i128 }`.
  - Every `amount` must be `> 0`.
  - `due_ts` values must be strictly increasing, strictly in the future, and `<= escrow.due_dt`.
  - Installment amounts must sum **exactly** to `face_value`.
- Stores cumulative milestones under `StorageKey::InstallmentSchedule(invoice_id)` and emits `installment_schedule_set`.
- Error: `InvalidInstallmentSchedule` (54) on any validation failure.

### `get_installment_schedule(invoice_id) -> Vec<InstallmentMilestone>`
Returns the stored milestone schedule (empty vector when none configured / after `cleanup_escrow`). Errors with `EscrowNotFound` for unknown invoices.

### `get_next_installment(invoice_id) -> Option<InstallmentMilestone>`
Returns the first unsettled milestone, or `None` when no schedule exists or all milestones are settled.

### Pause Policy
- When paused, the contract rejects lifecycle-changing operations:
  `create_escrow`, `cancel_escrow`, `fund_escrow`, `record_payment`, and `refund`.
- View functions and admin-only configuration updates remain available.

### Events
- `escrow_created`
- `escrow_funded`
- `payment_settled`
- `escrow_refunded`
- `escrow_cancelled`
- `platform_fee_updated`
- `distributor_updated`
- `paused_updated`
- `installment_schedule_set`
- `installment_settled`
- `settlement_proposed`
- `settlement_approved`
