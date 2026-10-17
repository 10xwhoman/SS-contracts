//! Invoice Escrow contract for StellarSettle.
//!
//! Handles escrow creation, funding by investors, payment settlement,
//! and refunds when invoices are not paid by due date.

#![no_std]
#![allow(clippy::too_many_arguments)]

mod errors;
mod events;
mod storage;
mod types;

use soroban_sdk::{contract, contractimpl, token, Address, Env, IntoVal, Symbol};

// EscrowStatus is re-exported publicly; Config and EscrowData are crate-private.
pub use types::EscrowStatus;
use types::{Config, EscrowData};

use errors::Error;

const MAX_BPS: u32 = 10_000;
const DISTRIBUTE_PAYMENT_FN: &str = "distribute_payment";
const DISTRIBUTE_REFUND_FN: &str = "distribute_refund";

#[contract]
pub struct InvoiceEscrow;

fn ensure_not_paused(config: &Config) -> Result<(), Error> {
    if config.paused {
        return Err(Error::Paused);
    }
    Ok(())
}

#[contractimpl]
impl InvoiceEscrow {
    /// Initialize the contract with admin and platform fee (basis points, e.g. 300 = 3%).
    pub fn initialize(env: Env, admin: Address, platform_fee_bps: u32) -> Result<(), Error> {
        admin.require_auth();
        if storage::get_config(&env).is_some() {
            return Err(Error::AlreadyInit);
        }
        if platform_fee_bps > MAX_BPS {
            return Err(Error::InvalidFeeBps);
        }
        let config = Config {
            admin: admin.clone(),
            fee_bps: platform_fee_bps,
            payment_distributor: None,
            referral_contract: None,
            paused: false,
            whitelist_enabled: false,
        };
        storage::set_config(&env, &config);
        Ok(())
    }

    /// Admin-only: enable/disable buyer whitelist enforcement on `fund_escrow`.
    pub fn set_whitelist_enabled(env: Env, admin: Address, enabled: bool) -> Result<(), Error> {
        admin.require_auth();
        let mut config = storage::get_config(&env).ok_or(Error::NotInit)?;
        if config.admin != admin {
            return Err(Error::Unauthorized);
        }
        config.whitelist_enabled = enabled;
        storage::set_config(&env, &config);
        Ok(())
    }

    /// Admin-only: add or remove a buyer from the whitelist.
    pub fn set_buyer_whitelisted(
        env: Env,
        admin: Address,
        buyer: Address,
        allowed: bool,
    ) -> Result<(), Error> {
        admin.require_auth();
        let config = storage::get_config(&env).ok_or(Error::NotInit)?;
        if config.admin != admin {
            return Err(Error::Unauthorized);
        }
        storage::set_whitelisted(&env, &buyer, allowed);
        Ok(())
    }

    /// View: is `buyer` whitelisted to fund escrows.
    pub fn is_buyer_whitelisted(env: Env, buyer: Address) -> bool {
        storage::is_whitelisted(&env, &buyer)
    }

    /// Create an escrow for an invoice. Caller (seller) must be authenticated.
    /// face_value: what the debtor owes (amount to be paid at settlement)
    /// purchase_price: what the investor pays (discount applied here)
    /// commitment: immutable on-chain anchor (SHA-256 hash of off-chain invoice data)
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
        commitment: soroban_sdk::BytesN<32>,
        funding_milestone: Option<i128>,
    ) -> Result<(), Error> {
        seller.require_auth();
        if face_value <= 0 || purchase_price <= 0 {
            return Err(Error::InvalidAmount);
        }
        if due_date == 0 {
            return Err(Error::InvalidDueDate);
        }
        let current_timestamp = env.ledger().timestamp();
        if due_date <= current_timestamp {
            return Err(Error::InvalidDueDate);
        }
        let config = storage::get_config(&env).ok_or(Error::NotInit)?;
        ensure_not_paused(&config)?;
        if storage::has_escrow(&env, invoice_id.clone()) {
            return Err(Error::EscrowExists);
        }
        // Ensure the payment token and invoice token use the same decimals to avoid
        // settlement/rounding mismatches during distribution and fee calculations.
        let inv_decimals: Option<u32> = env
            .try_invoke_contract::<u32, soroban_sdk::Error>(
                &invoice_token,
                &Symbol::new(&env, "decimals"),
                soroban_sdk::vec![&env],
            )
            .ok()
            .and_then(|r| r.ok());
        let pay_decimals: Option<u32> = env
            .try_invoke_contract::<u32, soroban_sdk::Error>(
                &payment_token,
                &Symbol::new(&env, "decimals"),
                soroban_sdk::vec![&env],
            )
            .ok()
            .and_then(|r| r.ok());
        if let (Some(inv_d), Some(pay_d)) = (inv_decimals, pay_decimals) {
            if inv_d != pay_d {
                return Err(Error::InvalidAssetDecimals);
            }
        }
        let data = EscrowData {
            inv_id: invoice_id.clone(),
            seller: seller.clone(),
            debtor: debtor.clone(),
            face_value,
            purchase_price,
            funded_amt: 0,
            funder: None,
            funders: soroban_sdk::Vec::new(&env),
            due_dt: due_date,
            token: payment_token.clone(),
            inv_token: invoice_token.clone(),
            paid_amt: 0,
            status: EscrowStatus::Created,
            funding_milestone,
            min_investment: 0,
            commitment: commitment.clone(),
        };
        storage::set_escrow(&env, invoice_id.clone(), &data);
        events::escrow_created(
            &env,
            invoice_id.clone(),
            &seller,
            &debtor,
            face_value,
            purchase_price,
            due_date,
            &payment_token,
            &invoice_token,
            &commitment,
            data.funding_milestone,
        );
        events::escrow_status_changed(&env, invoice_id, EscrowStatus::Created, current_timestamp);
        Ok(())
    }

    /// Cancel an escrow in Created state, refunding any partial funds to the funders.
    /// Only the seller may cancel, and only while status is Created.
    /// Cancel an unfunded escrow. Only the seller may cancel, and only while status is Created
    /// AND no investor has contributed any funds yet.
    ///
    /// Locked out after partial payment: `fund_escrow` accepts partial contributions and only
    /// flips `status` to `Funded` once the escrow is fully subscribed, so an escrow with
    /// `funded_amt > 0` can still read as `Created`. Cancelling in that window would strand the
    /// investor's already-transferred funds (cancellation has no refund path), so any nonzero
    /// `funded_amt` blocks cancellation regardless of status.
    ///
    /// Emits `escrow_refunded` (if partial funds existed) and `escrow_cancelled`.
    pub fn cancel_escrow(env: Env, invoice_id: Symbol, seller: Address) -> Result<(), Error> {
        seller.require_auth();
        let config = storage::get_config(&env).ok_or(Error::NotInit)?;
        ensure_not_paused(&config)?;
        let mut data =
            storage::get_escrow(&env, invoice_id.clone()).ok_or(Error::EscrowNotFound)?;
        if data.seller != seller {
            return Err(Error::Unauthorized);
        }
        if data.status == EscrowStatus::Cancelled {
            return Err(Error::EscrowCancelled);
        }
        if data.status == EscrowStatus::Funded {
            return Err(Error::EscrowFunded);
        }
        if data.status != EscrowStatus::Created {
            return Err(Error::CancelNotAllowed);
        }

        if data.funded_amt > 0 {
            let amount_to_refund = data.funded_amt;
            let token = token::Client::new(&env, &data.token);
            let contract = env.current_contract_address();
            let funder_opt = data.funder.clone();

            if let Some(distributor) = config.payment_distributor.as_ref() {
                token.transfer(&contract, distributor, &amount_to_refund);
                env.invoke_contract::<()>(
                    distributor,
                    &Symbol::new(&env, DISTRIBUTE_REFUND_FN),
                    soroban_sdk::vec![
                        &env,
                        contract.to_val(),
                        invoice_id.clone().into_val(&env),
                        soroban_sdk::vec![
                            &env,
                            <Address as IntoVal<Env, soroban_sdk::Val>>::into_val(
                                &data.token,
                                &env
                            ),
                            <Option<Address> as IntoVal<Env, soroban_sdk::Val>>::into_val(
                                &funder_opt,
                                &env,
                            )
                        ]
                        .into_val(&env),
                        soroban_sdk::vec![&env, amount_to_refund].into_val(&env),
                        (EscrowStatus::Cancelled as u32).into_val(&env)
                    ],
                );
            } else if let Some(funder) = &funder_opt {
                let funder_amt = storage::get_funder_amount(&env, invoice_id.clone(), funder);
                if funder_amt > 0 {
                    token.transfer(&contract, funder, &funder_amt);
                }
            }

            env.invoke_contract::<()>(
                &data.inv_token,
                &Symbol::new(&env, "set_transfer_locked"),
                soroban_sdk::vec![&env, contract.to_val(), false.into_val(&env)],
            );
            events::escrow_refunded(&env, invoice_id.clone(), amount_to_refund);
        }
        data.status = EscrowStatus::Cancelled;
        storage::set_escrow(&env, invoice_id.clone(), &data);
        events::escrow_cancelled(&env, invoice_id.clone(), &seller);
        events::escrow_status_changed(
            &env,
            invoice_id,
            EscrowStatus::Cancelled,
            env.ledger().timestamp(),
        );
        Ok(())
    }

    /// Fund the escrow (investor buys part or all of the invoice at purchase_price).
    /// Transfers `amount` from buyer to this contract. Multiple investors can fund until fully subscribed.
    pub fn fund_escrow(
        env: Env,
        invoice_id: Symbol,
        buyer: Address,
        amount: i128,
    ) -> Result<(), Error> {
        buyer.require_auth();
        Self::fund_escrow_core(&env, invoice_id, &buyer, amount)
    }

    /// Fund the escrow on behalf of `buyer` using a signed off-chain approval that a relayer
    /// submits on their behalf. `buyer` authorizes exactly this `(invoice_id, amount, nonce, expiry)`
    /// tuple, and `nonce` must be strictly greater than the last nonce consumed by `buyer` so
    /// the same signed approval cannot be replayed.
    ///
    /// Issue #183: Includes an `expiry` timestamp. If the ledger timestamp exceeds `expiry`
    /// the signature is rejected, limiting the window for replay attacks.
    pub fn fund_escrow_signed(
        env: Env,
        invoice_id: Symbol,
        buyer: Address,
        amount: i128,
        nonce: u64,
        expiry: u64,
    ) -> Result<(), Error> {
        buyer.require_auth_for_args((invoice_id.clone(), amount, nonce, expiry).into_val(&env));

        let current_ts = env.ledger().timestamp();
        if current_ts > expiry {
            return Err(Error::SignatureExpired);
        }

        let last_nonce = storage::get_nonce(&env, &buyer);
        if nonce <= last_nonce {
            return Err(Error::NonceAlreadyUsed);
        }

        Self::fund_escrow_core(&env, invoice_id.clone(), &buyer, amount)?;

        storage::set_nonce(&env, &buyer, nonce);
        events::escrow_funded_signed(&env, invoice_id, &buyer, amount, nonce);
        Ok(())
    }

    /// Shared funding logic used by both the directly-authorized and signed-approval entry points.
    fn fund_escrow_core(
        env: &Env,
        invoice_id: Symbol,
        buyer: &Address,
        amount: i128,
    ) -> Result<(), Error> {
        // Fail fast: validate amount before hitting storage.
        if amount <= 0 {
            return Err(Error::InvalidAmount);
        }
        let config = storage::get_config(env).ok_or(Error::NotInit)?;
        ensure_not_paused(&config)?;
        if config.whitelist_enabled && !storage::is_whitelisted(env, buyer) {
            return Err(Error::NotWhitelisted);
        }

        let mut data = storage::get_escrow(env, invoice_id.clone()).ok_or(Error::EscrowNotFound)?;
        if data.status == EscrowStatus::Cancelled {
            return Err(Error::EscrowCancelled);
        }
        if data.status != EscrowStatus::Created {
            return Err(Error::EscrowFunded);
        }

        // Check that funding doesn't exceed purchase_price
        let new_funded = data.funded_amt.checked_add(amount).ok_or(Error::Overflow)?;
        if new_funded > data.purchase_price {
            return Err(Error::InvalidAmount);
        }

        // Enforce per-invoice minimum investment floor.
        // A minimum of 0 means no floor — any positive amount is accepted.
        if data.min_investment > 0 && amount < data.min_investment {
            return Err(Error::MinimumInvestmentViolation);
        }

        // Validate milestone constraints if a milestone is set
        if let Some(milestone) = data.funding_milestone {
            let remaining_to_fund = data
                .purchase_price
                .checked_sub(data.funded_amt)
                .ok_or(Error::Overflow)?;

            // Funder is always allowed to just fund exactly the remaining amount to complete the escrow.
            // If they are not completing the escrow, the amount must be at least the milestone and a multiple of it.
            if amount != remaining_to_fund && (amount < milestone || amount % milestone != 0) {
                return Err(Error::InvalidMilestoneAmount);
            }
        }

        let token = token::Client::new(env, &data.token);
        let contract = env.current_contract_address();
        token.transfer(buyer, &contract, &amount);

        // Mint invoice tokens to the buyer to represent their ownership share
        env.invoke_contract::<()>(
            &data.inv_token,
            &Symbol::new(env, "mint"),
            soroban_sdk::vec![env, buyer.to_val(), amount.into_val(env), contract.to_val()],
        );

        // Track this funder's contribution
        let current_funder_amt = storage::get_funder_amount(env, invoice_id.clone(), buyer);
        let new_funder_amt = current_funder_amt
            .checked_add(amount)
            .ok_or(Error::Overflow)?;
        storage::set_funder_amount(env, invoice_id.clone(), buyer, new_funder_amt);

        data.funded_amt = new_funded;

        let mut already_recorded = false;
        for funder in data.funders.iter() {
            if funder == buyer.clone() {
                already_recorded = true;
                break;
            }
        }
        if !already_recorded {
            data.funders.push_back(buyer.clone());
        }

        // MVP: Store the first funder for direct distribution
        if data.funder.is_none() {
            data.funder = Some(buyer.clone());
        }

        // If fully funded, transition to Funded status
        if data.funded_amt == data.purchase_price {
            data.status = EscrowStatus::Funded;
        }

        storage::set_escrow(env, invoice_id.clone(), &data);
        events::escrow_funded(
            env,
            invoice_id.clone(),
            buyer,
            amount,
            data.funded_amt,
            data.purchase_price,
        );
        if data.status == EscrowStatus::Funded {
            events::escrow_status_changed(
                env,
                invoice_id.clone(),
                EscrowStatus::Funded,
                env.ledger().timestamp(),
            );
        }

        // Best-effort referral fee accrual: if a referral contract is configured,
        // notify it of this investment so it can accrue rewards to the buyer's referrer.
        // Errors from the referral contract are swallowed so a referral misconfiguration
        // never blocks a legitimate investment.
        if let Some(referral_addr) = config.referral_contract.as_ref() {
            let _ = env.try_invoke_contract::<(), soroban_sdk::Error>(
                referral_addr,
                &Symbol::new(env, "accrue_referral_fee"),
                soroban_sdk::vec![
                    env,
                    contract.to_val(),
                    invoice_id.into_val(env),
                    buyer.to_val(),
                    amount.into_val(env),
                ],
            );
        }

        Ok(())
    }

    /// Record payment: distribute to investors and platform fee. Payer must auth.
    /// Payer must be the authorized debtor for this invoice.
    /// Payment is applied toward face_value; fees are calculated on the payment amount.
    /// MVP: Distributes pro-rata to all funders based on their contribution.
    pub fn record_payment(
        env: Env,
        invoice_id: Symbol,
        payer: Address,
        amount: i128,
    ) -> Result<(), Error> {
        payer.require_auth();
        if amount <= 0 {
            return Err(Error::InvalidAmount);
        }
        let config = storage::get_config(&env).ok_or(Error::NotInit)?;
        ensure_not_paused(&config)?;
        let mut data =
            storage::get_escrow(&env, invoice_id.clone()).ok_or(Error::EscrowNotFound)?;

        // Enforce payer role: payer must be the authorized debtor
        if payer != data.debtor {
            return Err(Error::InvalidPayer);
        }

        if data.status != EscrowStatus::Funded {
            return Err(Error::AlreadySettled);
        }

        // Remaining balance toward face_value
        let remaining = data
            .face_value
            .checked_sub(data.paid_amt)
            .ok_or(Error::Overflow)?;
        if amount > remaining {
            return Err(Error::InvalidAmount);
        }

        let fee_bps = i128::from(config.fee_bps);
        // Fee is calculated on the payment amount (not face_value)
        let platform_fee = amount
            .checked_mul(fee_bps)
            .ok_or(Error::Overflow)?
            .checked_div(i128::from(MAX_BPS))
            .ok_or(Error::Overflow)?;
        let investor_amount = amount.checked_sub(platform_fee).ok_or(Error::Overflow)?;

        let token = token::Client::new(&env, &data.token);
        let contract = env.current_contract_address();

        // 1. Pull payer's funds into escrow
        token.transfer(&payer, &contract, &amount);

        data.paid_amt = data.paid_amt.checked_add(amount).ok_or(Error::Overflow)?;

        // Settlement occurs when paid_amt reaches face_value
        if data.paid_amt == data.face_value {
            data.status = EscrowStatus::Settled;
        }

        storage::set_escrow(&env, invoice_id.clone(), &data);

        let funder_addr = data.funder.clone().unwrap_or_else(|| data.seller.clone());

        if let Some(distributor) = config.payment_distributor.as_ref() {
            // The distributor must pay seller_amount (== amount) plus investor_amount + platform_fee
            // (== amount), mirroring the direct path below which releases the payer's `amount` to the
            // seller in addition to paying the investor/admin out of escrow's held funding.
            let total_to_distributor = amount.checked_add(amount).ok_or(Error::Overflow)?;
            token.transfer(&contract, distributor, &total_to_distributor);
            env.invoke_contract::<()>(
                distributor,
                &Symbol::new(&env, DISTRIBUTE_PAYMENT_FN),
                soroban_sdk::vec![
                    &env,
                    contract.to_val(),
                    invoice_id.clone().into_val(&env),
                    soroban_sdk::vec![
                        &env,
                        <Address as IntoVal<Env, soroban_sdk::Val>>::into_val(&data.token, &env),
                        <Address as IntoVal<Env, soroban_sdk::Val>>::into_val(&data.seller, &env),
                        <Address as IntoVal<Env, soroban_sdk::Val>>::into_val(&funder_addr, &env),
                        <Address as IntoVal<Env, soroban_sdk::Val>>::into_val(&config.admin, &env)
                    ]
                    .into_val(&env),
                    soroban_sdk::vec![
                        &env,
                        data.paid_amt,
                        amount,
                        investor_amount,
                        config.fee_bps as i128,
                    ]
                    .into_val(&env),
                    (data.status as u32).into_val(&env)
                ],
            );
        } else {
            // 2. Platform fee to admin
            token.transfer(&contract, &config.admin, &platform_fee);

            // 3. Pro-rata investor distribution
            if let Some(funder) = &data.funder {
                if data.funded_amt > 0 && investor_amount > 0 {
                    let funder_amt = storage::get_funder_amount(&env, invoice_id.clone(), funder);
                    let pro_rata_share = investor_amount
                        .checked_mul(funder_amt)
                        .ok_or(Error::Overflow)?
                        .checked_div(data.funded_amt)
                        .ok_or(Error::Overflow)?;
                    if pro_rata_share > 0 {
                        token.transfer(&contract, funder, &pro_rata_share);
                    }
                }
            }
            // Seller receives the full payment amount
            token.transfer(&contract, &data.seller, &amount);
        }

        if data.status == EscrowStatus::Settled {
            // Unlock invoice token transfers only when the invoice is completely settled.
            env.invoke_contract::<()>(
                &data.inv_token,
                &Symbol::new(&env, "set_transfer_locked"),
                soroban_sdk::vec![&env, contract.to_val(), false.into_val(&env)],
            );
        }

        events::payment_settled(
            &env,
            invoice_id.clone(),
            amount,
            platform_fee,
            investor_amount,
        );
        if data.status == EscrowStatus::Settled {
            events::escrow_status_changed(
                &env,
                invoice_id,
                EscrowStatus::Settled,
                env.ledger().timestamp(),
            );
        }
        Ok(())
    }

    /// Refund the investors if the invoice was not paid by due date. Anyone may call.
    /// Refunds are distributed pro-rata based on each investor's contribution.
    pub fn refund(env: Env, invoice_id: Symbol) -> Result<(), Error> {
        let config = storage::get_config(&env).ok_or(Error::NotInit)?;
        ensure_not_paused(&config)?;
        let mut data =
            storage::get_escrow(&env, invoice_id.clone()).ok_or(Error::EscrowNotFound)?;
        if data.status != EscrowStatus::Funded {
            return Err(Error::RefundNotAllowed);
        }
        let ledger_ts = env.ledger().timestamp();
        if ledger_ts < data.due_dt {
            return Err(Error::RefundNotAllowed);
        }

        // Refund the remaining collateral (purchase_price minus already released partial payments)
        let amount_to_refund = data
            .purchase_price
            .checked_sub(data.paid_amt)
            .ok_or(Error::Overflow)?;

        let token = token::Client::new(&env, &data.token);
        let contract = env.current_contract_address();

        // Extract funder address before status mutation so it is available in both paths.
        let funder_opt = data.funder.clone();

        data.status = EscrowStatus::Refunded;
        storage::set_escrow(&env, invoice_id.clone(), &data);

        if amount_to_refund > 0 {
            if let Some(distributor) = config.payment_distributor.as_ref() {
                token.transfer(&contract, distributor, &amount_to_refund);
                env.invoke_contract::<()>(
                    distributor,
                    &Symbol::new(&env, DISTRIBUTE_REFUND_FN),
                    soroban_sdk::vec![
                        &env,
                        contract.to_val(),
                        invoice_id.clone().into_val(&env),
                        soroban_sdk::vec![
                            &env,
                            <Address as IntoVal<Env, soroban_sdk::Val>>::into_val(
                                &data.token,
                                &env
                            ),
                            <Option<Address> as IntoVal<Env, soroban_sdk::Val>>::into_val(
                                &funder_opt,
                                &env,
                            )
                        ]
                        .into_val(&env),
                        soroban_sdk::vec![&env, amount_to_refund].into_val(&env),
                        (data.status as u32).into_val(&env)
                    ],
                );
            } else {
                // Pro-rata refund to funders
                if let Some(funder) = &funder_opt {
                    if data.funded_amt > 0 {
                        let funder_amt =
                            storage::get_funder_amount(&env, invoice_id.clone(), funder);
                        let pro_rata_refund = amount_to_refund
                            .checked_mul(funder_amt)
                            .ok_or(Error::Overflow)?
                            .checked_div(data.funded_amt)
                            .ok_or(Error::Overflow)?;
                        if pro_rata_refund > 0 {
                            token.transfer(&contract, funder, &pro_rata_refund);
                        }
                    }
                }
            }
        }

        // Unlock invoice token transfers now that the invoice is refunded
        env.invoke_contract::<()>(
            &data.inv_token,
            &Symbol::new(&env, "set_transfer_locked"),
            soroban_sdk::vec![&env, contract.to_val(), false.into_val(&env)],
        );

        events::escrow_refunded(&env, invoice_id.clone(), amount_to_refund);
        events::escrow_status_changed(
            &env,
            invoice_id,
            EscrowStatus::Refunded,
            env.ledger().timestamp(),
        );
        Ok(())
    }

    /// Update platform fee (basis points). Admin only.
    pub fn update_platform_fee_bps(env: Env, new_fee_bps: u32) -> Result<(), Error> {
        let mut config = storage::get_config(&env).ok_or(Error::NotInit)?;
        let admin = config.admin.clone();
        admin.require_auth();
        if new_fee_bps > MAX_BPS {
            return Err(Error::InvalidFeeBps);
        }
        let old_fee_bps = config.fee_bps;
        config.fee_bps = new_fee_bps;
        storage::set_config(&env, &config);
        events::platform_fee_updated(&env, old_fee_bps, new_fee_bps);
        Ok(())
    }

    /// Set the payment distributor used for settlement/refund fan-out. Admin only.
    pub fn set_payment_distributor(env: Env, payment_distributor: Address) -> Result<(), Error> {
        let mut config = storage::get_config(&env).ok_or(Error::NotInit)?;
        let admin = config.admin.clone();
        admin.require_auth();
        let old_distributor = config.payment_distributor.clone();
        config.payment_distributor = Some(payment_distributor.clone());
        storage::set_config(&env, &config);
        events::payment_distributor_updated(&env, old_distributor.is_some(), &payment_distributor);
        Ok(())
    }

    /// Set the referral contract called on each investment to accrue referral fees.
    /// Admin only. Pass the same address again to update, or a zero address is not
    /// accepted — use `clear_referral_contract` to remove the binding.
    pub fn set_referral_contract(env: Env, referral_contract: Address) -> Result<(), Error> {
        let mut config = storage::get_config(&env).ok_or(Error::NotInit)?;
        let admin = config.admin.clone();
        admin.require_auth();
        config.referral_contract = Some(referral_contract);
        storage::set_config(&env, &config);
        Ok(())
    }

    /// Remove the referral contract binding so investments no longer trigger
    /// referral fee accrual. Admin only.
    pub fn clear_referral_contract(env: Env) -> Result<(), Error> {
        let mut config = storage::get_config(&env).ok_or(Error::NotInit)?;
        let admin = config.admin.clone();
        admin.require_auth();
        config.referral_contract = None;
        storage::set_config(&env, &config);
        Ok(())
    }

    /// Admin-only: set the minimum investment floor for a specific invoice's escrow.
    /// Can only be called while the escrow is in Created state and has not received
    /// any funding yet, preventing changes after investors have already committed.
    /// A value of 0 means no floor (any positive amount is accepted).
    pub fn set_min_investment(
        env: Env,
        admin: Address,
        invoice_id: Symbol,
        min_investment: i128,
    ) -> Result<(), Error> {
        admin.require_auth();
        let config = storage::get_config(&env).ok_or(Error::NotInit)?;
        if config.admin != admin {
            return Err(Error::Unauthorized);
        }
        if min_investment < 0 {
            return Err(Error::InvalidAmount);
        }
        let mut data =
            storage::get_escrow(&env, invoice_id.clone()).ok_or(Error::EscrowNotFound)?;
        if data.status != EscrowStatus::Created {
            return Err(Error::EscrowFunded);
        }
        if data.funded_amt > 0 {
            return Err(Error::EscrowFunded);
        }
        data.min_investment = min_investment;
        storage::set_escrow(&env, invoice_id, &data);
        Ok(())
    }

    /// View: return the minimum investment floor for a given invoice.
    /// Returns 0 if no minimum has been set (meaning no floor).
    pub fn get_min_investment(env: Env, invoice_id: Symbol) -> Result<i128, Error> {
        let data = storage::get_escrow(&env, invoice_id).ok_or(Error::EscrowNotFound)?;
        Ok(data.min_investment)
    }

    /// Toggle the emergency pause flag. Admin only.
    pub fn set_paused(env: Env, paused: bool) -> Result<(), Error> {
        let mut config = storage::get_config(&env).ok_or(Error::NotInit)?;
        let admin = config.admin.clone();
        admin.require_auth();
        let old_paused = config.paused;
        config.paused = paused;
        storage::set_config(&env, &config);
        events::paused_updated(&env, old_paused, paused);
        Ok(())
    }

    /// View: return escrow data for an invoice, or Err(Error::EscrowNotFound) if not found.
    pub fn get_escrow(env: Env, invoice_id: Symbol) -> Result<EscrowData, Error> {
        storage::get_escrow(&env, invoice_id).ok_or(Error::EscrowNotFound)
    }

    /// View: return current config (admin and fee_bps).
    pub fn get_config(env: Env) -> Result<Config, Error> {
        storage::get_config(&env).ok_or(Error::NotInit)
    }

    /// View: return escrow status for an invoice.
    pub fn get_escrow_status(env: Env, invoice_id: Symbol) -> Result<EscrowStatus, Error> {
        let data = storage::get_escrow(&env, invoice_id).ok_or(Error::EscrowNotFound)?;
        Ok(data.status)
    }

    /// View: return the current pause state.
    pub fn paused(env: Env) -> Result<bool, Error> {
        let config = storage::get_config(&env).ok_or(Error::NotInit)?;
        Ok(config.paused)
    }

    /// Reclaim persistent storage for an escrow that has reached a terminal state
    /// (Settled, Refunded, or Cancelled). Callable only by the seller or the admin.
    /// The escrow and its per-funder contribution record are removed permanently;
    /// terminal-state escrows are never mutated again, so this is safe to prune.
    pub fn cleanup_escrow(env: Env, invoice_id: Symbol, caller: Address) -> Result<(), Error> {
        caller.require_auth();
        let config = storage::get_config(&env).ok_or(Error::NotInit)?;
        let data = storage::get_escrow(&env, invoice_id.clone()).ok_or(Error::EscrowNotFound)?;
        if caller != data.seller && caller != config.admin {
            return Err(Error::Unauthorized);
        }
        match data.status {
            EscrowStatus::Settled | EscrowStatus::Refunded | EscrowStatus::Cancelled => {}
            _ => return Err(Error::EscrowNotSettled),
        }
        storage::remove_escrow_state(&env, invoice_id.clone(), &data.funders);
        events::escrow_cleaned_up(&env, invoice_id);
        Ok(())
    }
}

#[cfg(test)]
mod integration_test;
#[cfg(test)]
mod test;
