# Escrow State Machine Invariants & Transition Matrix

This document specifies the primary invoice escrow lifecycle. Optional dispute and installment flows are described where they change the primary states. Authorization is enforced by Soroban authorization checks in the contract methods; callers listed below are the business actors, not substitutes for those checks.

## State transition matrix

| Current state | Action | Next state | Caller / conditions | Effects |
| --- | --- | --- | --- | --- |
| No record | `create_escrow` | `Created` | Seller; positive face value and purchase price; valid future due date and configured token addresses | Stores terms and invoice metadata; no payment movement |
| `Created` | `fund_escrow` (partial) | `Created` | Eligible buyer; amount positive and no more than remaining target | Payment tokens move into escrow; invoice tokens minted; investor/funder position updated |
| `Created` | `fund_escrow` (target reached) | `Funded` | Same funding checks; cumulative funding reaches purchase target | Same funding effects; invoice token transfers remain locked during active escrow |
| `Created` | `cancel_escrow` | `Cancelled` | Seller; no funds accepted | Marks terminal status; no refund is due |
| `Funded` | `record_payment` (partial) | `Funded` | Authorized debtor; payment within unpaid face value | Debtor payment collected and allocated according to configured distribution path; paid amount increases |
| `Funded` | `record_payment` (final) | `Settled` | Authorized debtor; cumulative payments reach face value | Final payout and configured token cleanup occur; invoice token transfer lock is released |
| `Funded` | `refund_escrow` | `Refunded` | Caller permitted by method; escrow is refund-eligible after due date/grace period | Remaining funded amount is returned according to the contract's refund allocation; configured token cleanup occurs and lock is released |
| `Funded` | open dispute | `Disputed` | Authorized party and dispute conditions satisfied | Records dispute metadata; settlement/refund is governed by dispute resolution |
| `Disputed` | resolve dispute | `Settled` or `Refunded` | Authorized resolver; resolution outcome and timeout rules satisfied | Applies corresponding payment/token effects and reaches terminal status |

`Settled`, `Refunded`, and `Cancelled` are terminal for the base lifecycle. Installment schedules may create intermediate payment milestones, but do not alter the funding and terminal-state rules above.

## Invariants

1. **Funding bound:** cumulative accepted funding never exceeds the purchase target; each accepted amount is positive.
2. **Payment bound:** cumulative recorded repayment never exceeds face value. A partial payment leaves the escrow active; reaching face value settles it.
3. **Balance accounting:** escrow-held payment tokens represent funds awaiting allocation/refund. After an allocation or refund, the contract balance decreases by the transferred amount. Previously allocated payouts and fees must not be counted as escrow-held funds.
4. **Fee arithmetic:** basis-point fees use `fee_bps / 10_000` and must remain within the configured maximum. Integer multiplication and additions used for financial amounts must be checked before storage or transfer.
5. **Token lifecycle:** invoice tokens are locked while their escrow is active. When the terminal settlement/refund path performs token cleanup, the lock is released.
6. **Authorization:** seller-only cancellation, debtor-only repayment, and admin-only configuration changes must verify the relevant address authorization in the contract invocation.
7. **Terminal status:** no funding, repayment, cancellation, or refund may reopen a terminal escrow.

The architecture-level call flows are in [ARCHITECTURE.md](ARCHITECTURE.md#contract-interaction-sequences).
