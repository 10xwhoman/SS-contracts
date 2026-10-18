# StellarSettle Contract Error Code Catalog

This document catalogues all contract error codes, root cause failure conditions, and resolution steps across `invoice-escrow`, `invoice-token`, and `payment-distributor`.

## Error Codes & Resolution Matrix

| Code | Variant Name | Root Cause Description | Resolution Steps |
| :--- | :--- | :--- | :--- |
| 1 | `AlreadyInit` | Contract `initialize()` invoked more than once. | Verify setup script; do not call initialize twice. |
| 2 | `NotInit` | Function called before contract initialization. | Run deployment/initialization script first (`deploy.sh`). |
| 3 | `Unauthorized` | Caller address does not match required admin/seller auth. | Ensure correct wallet signature (`require_auth()`). |
| 4 | `Paused` | Contract is in emergency paused mode (`paused == true`). | Admin must call `set_paused(false)` before operations resume. |
| 5 | `InvalidAmount` | Amount parameter <= 0 or exceeds purchase/remaining balance. | Verify positive payment/funding amount within bounds. |
| 6 | `InvalidFeeBps` | Basis points fee exceeds 10,000 (100%). | Pass fee <= 10,000 bps (e.g. 300 = 3%). |
| 7 | `EscrowExists` | Invoice ID symbol already exists in storage. | Use unique invoice ID symbol for new escrow creation. |
| 8 | `EscrowNotFound` | Invoice ID symbol does not exist in contract storage. | Confirm invoice ID string; verify escrow was created. |
| 9 | `EscrowFunded` | Cannot cancel or re-fund an already funded escrow. | Check escrow status via `get_escrow_status()`. |
| 10 | `EscrowCancelled` | Cannot fund or settle an escrow that was cancelled. | No further operations permitted on cancelled escrows. |
| 11 | `AlreadySettled` | Invoice payment face value already fully paid. | Escrow settlement complete; no additional payments allowed. |
| 12 | `RefundNotAllowed` | Attempted refund before due date or on unfunded escrow. | Wait until `ledger_timestamp >= due_date` to execute refund. |
| 13 | `Overflow` | Checked math arithmetic overflow/underflow. | Verify transaction amounts fit within standard i128 range. |
| 14 | `InvalidDueDate` | Due date timestamp is in the past or zero. | Provide Unix timestamp strictly greater than current ledger time. |
| 15 | `InvalidPayer` | Payer address does not match authorized debtor. | Execute payment using debtor account. |
| 54 | `InvalidInstallmentSchedule` | Installment schedule empty/too long, non-positive amount, non-increasing or out-of-range `due_ts`, sum ≠ `face_value`, wrong escrow status, or `paid_amt > 0`. | Configure `1..=64` installments summing to `face_value` with strictly increasing future `due_ts <= due_dt` before any repayment starts. |
