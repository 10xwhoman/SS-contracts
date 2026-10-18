# End-to-End Invoice Financing Lifecycle

This document visualizes and explains the full end-to-end lifecycle of an invoice asset on StellarSettle, from issuance to final settlement or refund.

---

## 🔄 End-to-End Sequence Diagram

```
Seller                Escrow Contract          Investor               Debtor
  │                          │                    │                      │
  │─── 1. create_escrow ────>│                    │                      │
  │    (Invoice Details)     │                    │                      │
  │                          │                    │                      │
  │                          │<── 2. fund_escrow ─│                      │
  │                          │    (Payment Tokens)│                      │
  │                          │                    │                      │
  │<── 3. Mint Tokens ───────┼───────────────────>│                      │
  │    (Invoice Shares)      │                    │                      │
  │                          │                    │                      │
  │                          │<───────────────────┼── 4. record_payment ─│
  │                          │                    │    (Invoice Payout)  │
  │                          │                    │                      │
  │<── 5. Payout Net ────────┼───────────────────>│                      │
  │    (Pro-rata split)      │                    │                      │
```

---

## Lifecycle Stages Breakdown

### Stage 1: Invoice Creation (`Created`)
- **Actor:** Seller / Business
- **Action:** Calls `create_escrow` with invoice metadata (face value, discount purchase price, due date, payment token).
- **State:** Escrow parameters validated and stored. Invoice tokens initialized.

### Stage 2: Investor Funding (`Funded`)
- **Actor:** Liquidity Provider / Investor
- **Action:** Calls `fund_escrow`. Transfers payment tokens into contract escrow vault.
- **State:** `invoice-token` contract mints pro-rata shares to investor. Escrow enters `Funded` state. Transfer locks activated.

### Stage 3: Installment Schedule (optional, pre-payment)
- **Actor:** Seller
- **Action:** While the escrow is still `Created`/`Funded` with `paid_amt == 0`, the seller may call `set_installment_schedule` with a sequence of future installments whose amounts sum exactly to `face_value`.
- **State:** Cumulative milestones are stored per invoice. Each subsequent `record_payment` marks every milestone whose cumulative target has been reached as settled (emitting `installment_settled`). Full settlement (including early-settlement discounts or emergency release) settles any remaining milestones.

### Stage 4: Settlement (`Settled`)
- **Actor:** Debtor (Invoice Payer)
- **Action:** Calls `record_payment` before or at due date (full payment or matching installments).
- **State:** `payment-distributor` executes fee deduction and pro-rata payout fan-out to seller and investors. Escrow enters `Settled` state. Token locks released. All installment milestones are marked settled.

### Alternative Stage 5: Refund / Default (`Refunded`)
- **Actor:** Admin / Seller (if past due date without debtor payment)
- **Action:** Calls `refund`.
- **State:** Escrowed payment tokens returned to investors; invoice tokens burned. Escrow enters `Refunded` state.

---

## References

- State machine specification: [`docs/state-machine.md`](state-machine.md)
- Distributor math: [`docs/distributor_guide.md`](distributor_guide.md)
- Error codes: [`docs/error_catalog.md`](error_catalog.md)
