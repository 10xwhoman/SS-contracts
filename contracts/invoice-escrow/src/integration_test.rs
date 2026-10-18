//! Comprehensive integration tests for the InvoiceEscrow contract.
//!
//! These tests spin up real InvoiceToken and Stellar-asset-contract instances
//! alongside InvoiceEscrow so that cross-contract calls (mint, set_transfer_locked,
//! token transfers) all execute as they would on-chain.

#![allow(deprecated)]

use super::*;
use invoice_token::{InvoiceToken, InvoiceTokenClient};
use referral::{Referral, ReferralClient};
use soroban_sdk::token::{Client as TokenClient, StellarAssetClient as AssetClient};
use soroban_sdk::{
    testutils::{Address as _, Events as _, Ledger as _},
    Address, BytesN, Env, String as SorobanString, Symbol, TryFromVal, TryIntoVal, Val, Vec,
};

// ──────────────────────────────────────────────────────────────────────────────
// Shared helpers
// ──────────────────────────────────────────────────────────────────────────────

/// Build a deterministic 32-byte commitment from an ASCII string (pads with 0s).
fn test_commitment(env: &Env, data: &str) -> BytesN<32> {
    let mut array = [0u8; 32];
    let bytes = data.as_bytes();
    let len = bytes.len().min(32);
    array[..len].copy_from_slice(&bytes[..len]);
    BytesN::from_array(env, &array)
}

/// All addresses and clients needed by most tests.
struct Ctx<'a> {
    env: Env,
    admin: Address,
    seller: Address,
    buyer: Address,
    payer: Address,
    escrow_id: Address,
    escrow: InvoiceEscrowClient<'a>,
    inv_token_id: Address,
    inv_token: InvoiceTokenClient<'a>,
    payment_token: TokenClient<'a>,
    payment_asset: AssetClient<'a>,
    invoice_id: Symbol,
}

/// Stand up all contracts, initialize them, and optionally mint tokens to buyer/payer.
fn setup<'a>(
    env: &'a Env,
    fee_bps: u32,
    inv_id_str: &str,
    buyer_balance: i128,
    payer_balance: i128,
) -> Ctx<'a> {
    let admin = Address::generate(env);
    let seller = Address::generate(env);
    let buyer = Address::generate(env);
    let payer = Address::generate(env);

    let escrow_id = env.register(InvoiceEscrow, ());
    let escrow = InvoiceEscrowClient::new(env, &escrow_id);

    let inv_token_id = env.register(InvoiceToken, ());
    let inv_token = InvoiceTokenClient::new(env, &inv_token_id);

    let token_admin = Address::generate(env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin);
    let payment_token = TokenClient::new(env, &token_contract.address());
    let payment_asset = AssetClient::new(env, &token_contract.address());

    let invoice_id = Symbol::new(env, inv_id_str);

    inv_token.initialize(
        &escrow_id,
        &SorobanString::from_str(env, "Test Invoice Token"),
        &SorobanString::from_str(env, "TIT"),
        &7,
        &invoice_id,
        &escrow_id,
    );
    escrow.initialize(&admin, &fee_bps);

    if buyer_balance > 0 {
        payment_asset.mint(&buyer, &buyer_balance);
    }
    if payer_balance > 0 {
        payment_asset.mint(&payer, &payer_balance);
    }

    Ctx {
        env: env.clone(),
        admin,
        seller,
        buyer,
        payer,
        escrow_id,
        escrow,
        inv_token_id,
        inv_token,
        payment_token,
        payment_asset,
        invoice_id,
    }
}

/// Create and fully fund an escrow using face_value == purchase_price == `amount`.
fn create_and_fund(ctx: &Ctx<'_>, amount: i128, due_date: u64) {
    ctx.escrow.create_escrow_legacy(
        &ctx.invoice_id,
        &ctx.seller,
        &ctx.payer,
        &amount,
        &amount,
        &due_date,
        &ctx.payment_token.address,
        &ctx.inv_token_id,
        &test_commitment(&ctx.env, "commitment"),
        &None,
        &None,
    );
    ctx.escrow.fund_escrow_legacy(&ctx.invoice_id, &ctx.buyer, &amount);
}

// ──────────────────────────────────────────────────────────────────────────────
// 1. Happy-path lifecycle (retained from original, extended assertions)
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn test_integration_escrow_lifecycle_happy_path() {
    let env = Env::default();
    env.mock_all_auths();
    let ctx = setup(&env, 300, "INV001", 1_000, 1_000);

    let due_date = 100_000u64;
    create_and_fund(&ctx, 1_000, due_date);

    // After funding: buyer paid, escrow holds tokens, inv-token locked.
    assert_eq!(ctx.payment_token.balance(&ctx.buyer), 0);
    assert_eq!(ctx.payment_token.balance(&ctx.escrow_id), 1_000);
    assert_eq!(ctx.inv_token.balance(&ctx.buyer), 1_000);
    assert!(ctx.inv_token.transfer_locked());
    assert_eq!(
        ctx.escrow.get_escrow_status(&ctx.invoice_id),
        EscrowStatus::Funded
    );

    // Settle.
    ctx.escrow
        .record_payment(&ctx.invoice_id, &ctx.payer, &1_000);

    // 3% fee: 30 to admin, 970 to buyer, 1000 to seller, escrow empty.
    assert_eq!(ctx.payment_token.balance(&ctx.admin), 30);
    assert_eq!(ctx.payment_token.balance(&ctx.buyer), 970);
    assert_eq!(ctx.payment_token.balance(&ctx.seller), 1_000);
    assert_eq!(ctx.payment_token.balance(&ctx.escrow_id), 0);
    assert_eq!(ctx.payment_token.balance(&ctx.payer), 0);

    assert_eq!(
        ctx.escrow.get_escrow_status(&ctx.invoice_id),
        EscrowStatus::Settled
    );
    // Token must unlock after full settlement.
    assert!(!ctx.inv_token.transfer_locked());

    // Buyer can now freely transfer invoice tokens.
    let recipient = Address::generate(&env);
    ctx.inv_token.transfer(&ctx.buyer, &recipient, &1_000);
    assert_eq!(ctx.inv_token.balance(&ctx.buyer), 0);
    assert_eq!(ctx.inv_token.balance(&recipient), 1_000);
}

// ──────────────────────────────────────────────────────────────────────────────
// 2. Refund lifecycle (retained + extended)
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn test_integration_multi_contract_partial_funding_and_repayment_lifecycle() {
    let env = Env::default();
    env.mock_all_auths();
    let ctx = setup(&env, 250, "INVMULTI", 400, 1_200);
    let face_value = 1_200i128;
    let purchase_price = 1_000i128;
    let due_date = 100_000u64;

    ctx.escrow.create_escrow(
        &ctx.invoice_id,
        &ctx.seller,
        &ctx.payer,
        &face_value,
        &purchase_price,
        &due_date,
        &ctx.payment_token.address,
        &ctx.inv_token_id,
        &test_commitment(&ctx.env, "multi-contract"),
        &None,
        &None,
    );

    // The same Soroban environment hosts escrow, SEP-41 invoice token, and
    // Stellar payment asset contracts through a partial then final funding.
    ctx.escrow.fund_escrow(&ctx.invoice_id, &ctx.buyer, &400);
    assert_eq!(
        ctx.escrow.get_escrow_status(&ctx.invoice_id),
        EscrowStatus::Created
    );
    assert_eq!(ctx.payment_token.balance(&ctx.escrow_id), 400);
    assert_eq!(ctx.inv_token.balance(&ctx.buyer), 400);

    let second_investor = Address::generate(&env);
    ctx.payment_asset.mint(&second_investor, &600);
    ctx.escrow
        .fund_escrow(&ctx.invoice_id, &second_investor, &600);
    assert_eq!(
        ctx.escrow.get_escrow_status(&ctx.invoice_id),
        EscrowStatus::Funded
    );
    assert_eq!(ctx.payment_token.balance(&ctx.escrow_id), 1_000);
    assert!(ctx.inv_token.transfer_locked());

    ctx.escrow.record_payment(&ctx.invoice_id, &ctx.payer, &500);
    assert_eq!(
        ctx.escrow.get_escrow_status(&ctx.invoice_id),
        EscrowStatus::Funded
    );
    ctx.escrow.record_payment(&ctx.invoice_id, &ctx.payer, &700);

    assert_eq!(
        ctx.escrow.get_escrow_status(&ctx.invoice_id),
        EscrowStatus::Settled
    );
    assert_eq!(ctx.payment_token.balance(&ctx.payer), 0);
    assert_eq!(ctx.payment_token.balance(&ctx.escrow_id), 0);
    assert_eq!(ctx.payment_token.balance(&ctx.seller), face_value);
    assert_eq!(ctx.payment_token.balance(&ctx.admin), 30);
    assert!(!ctx.inv_token.transfer_locked());
}

#[test]
fn test_integration_refund_lifecycle() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(5_000);
    let ctx = setup(&env, 300, "INVREF", 1_000, 0);

    let due_date = 10_000u64;
    create_and_fund(&ctx, 1_000, due_date);

    // Refund before due date must fail.
    assert!(ctx.escrow.try_refund_escrow(&ctx.invoice_id).is_err());

    // Advance past due date.
    env.ledger().set_timestamp(due_date + 1);
    ctx.escrow.refund_escrow(&ctx.invoice_id);

    // Buyer gets full purchase price back.
    assert_eq!(ctx.payment_token.balance(&ctx.buyer), 1_000);
    assert_eq!(ctx.payment_token.balance(&ctx.escrow_id), 0);
    assert_eq!(
        ctx.escrow.get_escrow_status(&ctx.invoice_id),
        EscrowStatus::Refunded
    );
    // Token unlocks after refund.
    assert!(!ctx.inv_token.transfer_locked());
}

// ──────────────────────────────────────────────────────────────────────────────
// 3. Invoice token locked during active escrow (retained)
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn test_integration_token_locked_during_active_escrow() {
    let env = Env::default();
    env.mock_all_auths();
    let ctx = setup(&env, 300, "INVLCK", 500, 500);

    create_and_fund(&ctx, 500, 20_000);

    assert!(ctx.inv_token.transfer_locked());

    // Transfer attempt must fail while locked.
    let other = Address::generate(&env);
    assert!(ctx
        .inv_token
        .try_transfer(&ctx.buyer, &other, &100)
        .is_err());

    ctx.escrow.record_payment(&ctx.invoice_id, &ctx.payer, &500);
    assert!(!ctx.inv_token.transfer_locked());
    ctx.inv_token.transfer(&ctx.buyer, &other, &100);
    assert_eq!(ctx.inv_token.balance(&ctx.buyer), 400);
    assert_eq!(ctx.inv_token.balance(&other), 100);
}

// ──────────────────────────────────────────────────────────────────────────────
// 4. Partial payment then full settlement
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn test_integration_partial_payments_accumulate_to_settlement() {
    let env = Env::default();
    env.mock_all_auths();
    // face_value = 1000, purchase_price = 1000, fee = 5%
    let ctx = setup(&env, 500, "INVPART", 1_000, 1_000);
    create_and_fund(&ctx, 1_000, 99_999);

    // First partial payment: 400
    ctx.escrow.record_payment(&ctx.invoice_id, &ctx.payer, &400);
    assert_eq!(
        ctx.escrow.get_escrow_status(&ctx.invoice_id),
        EscrowStatus::Funded // not yet settled
    );
    // 5% of 400 = 20 fee, 380 to buyer, 400 to seller
    assert_eq!(ctx.payment_token.balance(&ctx.admin), 20);
    assert_eq!(ctx.payment_token.balance(&ctx.buyer), 380);
    assert_eq!(ctx.payment_token.balance(&ctx.seller), 400);
    // Token remains locked during partial settlement.
    assert!(ctx.inv_token.transfer_locked());

    // Second payment: remaining 600
    ctx.escrow.record_payment(&ctx.invoice_id, &ctx.payer, &600);
    assert_eq!(
        ctx.escrow.get_escrow_status(&ctx.invoice_id),
        EscrowStatus::Settled
    );
    // Total fees: 5% of 1000 = 50
    assert_eq!(ctx.payment_token.balance(&ctx.admin), 50);
    assert_eq!(ctx.payment_token.balance(&ctx.buyer), 950);
    assert_eq!(ctx.payment_token.balance(&ctx.seller), 1_000);
    assert_eq!(ctx.payment_token.balance(&ctx.escrow_id), 0);
    assert_eq!(ctx.payment_token.balance(&ctx.payer), 0);
    assert!(!ctx.inv_token.transfer_locked());
}

// ──────────────────────────────────────────────────────────────────────────────
// 5. Partial payment then refund after due date
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn test_integration_partial_payment_then_refund() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);
    let ctx = setup(&env, 300, "INVPREF", 1_000, 1_000);

    create_and_fund(&ctx, 1_000, 5_000);

    // Partial payment: 400 (3% fee = 12, net 388 to buyer, 400 to seller)
    ctx.escrow.record_payment(&ctx.invoice_id, &ctx.payer, &400);
    assert_eq!(ctx.payment_token.balance(&ctx.seller), 400);
    assert_eq!(ctx.payment_token.balance(&ctx.buyer), 388);
    assert_eq!(ctx.payment_token.balance(&ctx.admin), 12);
    // Escrow still holds the original 1000 funding minus what was released.
    // After record_payment: escrow received 400 from payer, paid out 400+400-400 = 400 net.
    // Net: escrow had 1000, received 400, paid 400(seller)+388(buyer)+12(admin) = 800 out => 600 left.
    assert_eq!(ctx.payment_token.balance(&ctx.escrow_id), 600);

    // Advance past due date and refund remaining 600.
    env.ledger().set_timestamp(5_001);
    ctx.escrow.refund_escrow(&ctx.invoice_id);

    assert_eq!(
        ctx.escrow.get_escrow_status(&ctx.invoice_id),
        EscrowStatus::Refunded
    );
    // Buyer gets back 600 (the unfunded remainder).
    assert_eq!(ctx.payment_token.balance(&ctx.buyer), 388 + 600);
    assert_eq!(ctx.payment_token.balance(&ctx.escrow_id), 0);
    assert!(!ctx.inv_token.transfer_locked());
}

// ──────────────────────────────────────────────────────────────────────────────
// 6. Cancel unfunded escrow
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn test_integration_cancel_escrow_happy_path() {
    let env = Env::default();
    env.mock_all_auths();
    let ctx = setup(&env, 300, "INVCAN", 0, 0);

    ctx.escrow.create_escrow_legacy(
        &ctx.invoice_id,
        &ctx.seller,
        &ctx.payer,
        &1_000,
        &1_000,
        &99_999,
        &ctx.payment_token.address,
        &ctx.inv_token_id,
        &test_commitment(&env, "cancel_test"),
        &None,
        &None,
    );
    assert_eq!(
        ctx.escrow.get_escrow_status(&ctx.invoice_id),
        EscrowStatus::Created
    );

    ctx.escrow.cancel_escrow(&ctx.invoice_id, &ctx.seller);
    assert_eq!(
        ctx.escrow.get_escrow_status(&ctx.invoice_id),
        EscrowStatus::Cancelled
    );
}

#[test]
fn test_integration_cancel_funded_escrow_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let ctx = setup(&env, 300, "INVCNF", 1_000, 0);
    create_and_fund(&ctx, 1_000, 99_999);

    // Funded escrow cannot be cancelled.
    let result = ctx.escrow.try_cancel_escrow(&ctx.invoice_id, &ctx.seller);
    assert_eq!(result, Err(Ok(errors::Error::EscrowFunded)));
}

#[test]
fn test_integration_cancel_non_seller_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let ctx = setup(&env, 300, "INVCNR", 0, 0);

    ctx.escrow.create_escrow_legacy(
        &ctx.invoice_id,
        &ctx.seller,
        &ctx.payer,
        &1_000,
        &1_000,
        &99_999,
        &ctx.payment_token.address,
        &ctx.inv_token_id,
        &test_commitment(&env, "cancel_non_seller"),
        &None,
        &None,
    );

    let intruder = Address::generate(&env);
    let result = ctx.escrow.try_cancel_escrow(&ctx.invoice_id, &intruder);
    assert_eq!(result, Err(Ok(errors::Error::Unauthorized)));
}

#[test]
fn test_integration_fund_cancelled_escrow_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let ctx = setup(&env, 300, "INVCFC", 1_000, 0);

    ctx.escrow.create_escrow_legacy(
        &ctx.invoice_id,
        &ctx.seller,
        &ctx.payer,
        &1_000,
        &1_000,
        &99_999,
        &ctx.payment_token.address,
        &ctx.inv_token_id,
        &test_commitment(&env, "fund_cancelled"),
        &None,
        &None,
    );
    ctx.escrow.cancel_escrow(&ctx.invoice_id, &ctx.seller);

    let result = ctx
        .escrow
        .try_fund_escrow_legacy(&ctx.invoice_id, &ctx.buyer, &1_000);
    assert_eq!(result, Err(Ok(errors::Error::EscrowCancelled)));
}

// ──────────────────────────────────────────────────────────────────────────────
// 7. Emergency pause blocks lifecycle operations
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn test_integration_pause_blocks_fund_and_payment() {
    let env = Env::default();
    env.mock_all_auths();
    let ctx = setup(&env, 300, "INVPSE", 1_000, 1_000);

    ctx.escrow.create_escrow_legacy(
        &ctx.invoice_id,
        &ctx.seller,
        &ctx.payer,
        &1_000,
        &1_000,
        &99_999,
        &ctx.payment_token.address,
        &ctx.inv_token_id,
        &test_commitment(&env, "pause_test"),
        &None,
        &None,
    );

    ctx.escrow.set_paused(&true);
    assert!(ctx.escrow.paused());

    // fund_escrow must fail while paused.
    let r = ctx
        .escrow
        .try_fund_escrow_legacy(&ctx.invoice_id, &ctx.buyer, &1_000);
    assert_eq!(r, Err(Ok(errors::Error::Paused)));

    // Unpause and fund.
    ctx.escrow.set_paused(&false);
    ctx.escrow.fund_escrow_legacy(&ctx.invoice_id, &ctx.buyer, &1_000);

    // Re-pause and try to record payment.
    ctx.escrow.set_paused(&true);
    let r2 = ctx
        .escrow
        .try_record_payment(&ctx.invoice_id, &ctx.payer, &1_000);
    assert_eq!(r2, Err(Ok(errors::Error::Paused)));

    // Unpause and settle.
    ctx.escrow.set_paused(&false);
    ctx.escrow
        .record_payment(&ctx.invoice_id, &ctx.payer, &1_000);
    assert_eq!(
        ctx.escrow.get_escrow_status(&ctx.invoice_id),
        EscrowStatus::Settled
    );
}

#[test]
fn test_integration_pause_blocks_refund() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);
    let ctx = setup(&env, 300, "INVPSR", 1_000, 0);
    create_and_fund(&ctx, 1_000, 5_000);

    env.ledger().set_timestamp(5_001);
    ctx.escrow.set_paused(&true);

    let r = ctx.escrow.try_refund_escrow(&ctx.invoice_id);
    assert_eq!(r, Err(Ok(errors::Error::Paused)));

    ctx.escrow.set_paused(&false);
    ctx.escrow.refund_escrow(&ctx.invoice_id);
    assert_eq!(
        ctx.escrow.get_escrow_status(&ctx.invoice_id),
        EscrowStatus::Refunded
    );
}

// ──────────────────────────────────────────────────────────────────────────────
// 8. Zero-fee escrow — admin receives nothing, investor receives full amount
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn test_integration_zero_fee_full_investor_return() {
    let env = Env::default();
    env.mock_all_auths();
    let ctx = setup(&env, 0, "INVZF", 1_000, 1_000);
    create_and_fund(&ctx, 1_000, 99_999);

    ctx.escrow
        .record_payment(&ctx.invoice_id, &ctx.payer, &1_000);
    assert_eq!(ctx.payment_token.balance(&ctx.admin), 0);
    assert_eq!(ctx.payment_token.balance(&ctx.buyer), 1_000);
    assert_eq!(ctx.payment_token.balance(&ctx.seller), 1_000);
}

// ──────────────────────────────────────────────────────────────────────────────
// 9. Max fee (100%) — investor receives nothing
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn test_integration_max_fee_investor_receives_nothing() {
    let env = Env::default();
    env.mock_all_auths();
    let ctx = setup(&env, 10_000, "INVMF", 1_000, 1_000);
    create_and_fund(&ctx, 1_000, 99_999);

    ctx.escrow
        .record_payment(&ctx.invoice_id, &ctx.payer, &1_000);
    // 100% fee → investor gets 0, admin gets 1000, seller gets 1000 (released principal)
    assert_eq!(ctx.payment_token.balance(&ctx.admin), 1_000);
    assert_eq!(ctx.payment_token.balance(&ctx.buyer), 0);
    assert_eq!(ctx.payment_token.balance(&ctx.seller), 1_000);
    assert_eq!(ctx.payment_token.balance(&ctx.escrow_id), 0);
}

// ──────────────────────────────────────────────────────────────────────────────
// 10. Wrong payer is rejected
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn test_integration_wrong_payer_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let ctx = setup(&env, 300, "INVWP", 1_000, 0);
    create_and_fund(&ctx, 1_000, 99_999);

    let impostor = Address::generate(&env);
    ctx.payment_asset.mint(&impostor, &1_000);

    let result = ctx
        .escrow
        .try_record_payment(&ctx.invoice_id, &impostor, &1_000);
    assert_eq!(result, Err(Ok(errors::Error::InvalidPayer)));
}

// ──────────────────────────────────────────────────────────────────────────────
// 11. Overpayment rejected
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn test_integration_overpayment_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let ctx = setup(&env, 300, "INVOVR", 1_000, 2_000);
    create_and_fund(&ctx, 1_000, 99_999);

    // face_value == 1000, paying 1001 must fail.
    let result = ctx
        .escrow
        .try_record_payment(&ctx.invoice_id, &ctx.payer, &1_001);
    assert_eq!(result, Err(Ok(errors::Error::InvalidAmount)));
}

// ──────────────────────────────────────────────────────────────────────────────
// 12. Over-funding rejected
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn test_integration_over_funding_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let ctx = setup(&env, 300, "INVOVF", 2_000, 0);

    ctx.escrow.create_escrow_legacy(
        &ctx.invoice_id,
        &ctx.seller,
        &ctx.payer,
        &1_000,
        &1_000,
        &99_999,
        &ctx.payment_token.address,
        &ctx.inv_token_id,
        &test_commitment(&env, "over_fund"),
        &None,
        &None,
    );

    // Purchase price is 1000; funding 1001 must fail.
    let result = ctx
        .escrow
        .try_fund_escrow_legacy(&ctx.invoice_id, &ctx.buyer, &1_001);
    assert_eq!(result, Err(Ok(errors::Error::InvalidAmount)));
}

/// Two competing funders racing to fill the same escrow cannot overfund it.
/// The first transaction in ledger order fills the target; the losing funding
/// transaction is rejected without transferring tokens or minting invoice tokens.
#[test]
fn test_integration_competing_funders_cannot_overfund_invoice() {
    let env = Env::default();
    env.mock_all_auths();
    let ctx = setup(&env, 300, "INVRACE", 1_000, 0);
    let competitor = Address::generate(&env);
    ctx.payment_asset.mint(&competitor, &1_000);

    ctx.escrow.create_escrow_legacy(
        &ctx.invoice_id,
        &ctx.seller,
        &ctx.payer,
        &1_000,
        &1_000,
        &99_999,
        &ctx.payment_token.address,
        &ctx.inv_token_id,
        &test_commitment(&env, "front_run_race"),
        &None,
        &None,
    );

    ctx.escrow.fund_escrow_legacy(&ctx.invoice_id, &ctx.buyer, &1_000);
    let losing_funder = ctx.escrow.try_fund_escrow_legacy(&ctx.invoice_id, &competitor, &1_000);

    assert!(losing_funder.is_err());
    assert_eq!(ctx.payment_token.balance(&ctx.escrow_id), 1_000);
    assert_eq!(ctx.payment_token.balance(&competitor), 1_000);
    assert_eq!(ctx.inv_token.balance(&ctx.buyer), 1_000);
    assert_eq!(ctx.inv_token.balance(&competitor), 0);
    assert_eq!(ctx.escrow.get_escrow_status(&ctx.invoice_id), EscrowStatus::Funded);
}

// ──────────────────────────────────────────────────────────────────────────────
// 13. Platform fee update mid-lifecycle affects next payment
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn test_integration_fee_update_affects_subsequent_payment() {
    let env = Env::default();
    env.mock_all_auths();
    // Start with 3% fee, face_value = 1000, split into two payments.
    let ctx = setup(&env, 300, "INVFUP", 1_000, 1_000);
    create_and_fund(&ctx, 1_000, 99_999);

    // First payment: 500 at 3% fee → 15 to admin, 485 to buyer, 500 to seller.
    ctx.escrow.record_payment(&ctx.invoice_id, &ctx.payer, &500);
    assert_eq!(ctx.payment_token.balance(&ctx.admin), 15);
    assert_eq!(ctx.payment_token.balance(&ctx.buyer), 485);
    assert_eq!(ctx.payment_token.balance(&ctx.seller), 500);

    // Admin raises fee to 10%.
    ctx.escrow.update_platform_fee_bps(&1_000);
    assert_eq!(ctx.escrow.get_config().fee_bps, 1_000);

    // Second payment: 500 at 10% → 50 to admin, 450 to buyer, 500 to seller.
    ctx.escrow.record_payment(&ctx.invoice_id, &ctx.payer, &500);
    assert_eq!(ctx.payment_token.balance(&ctx.admin), 15 + 50);
    assert_eq!(ctx.payment_token.balance(&ctx.buyer), 485 + 450);
    assert_eq!(ctx.payment_token.balance(&ctx.seller), 1_000);
    assert_eq!(
        ctx.escrow.get_escrow_status(&ctx.invoice_id),
        EscrowStatus::Settled
    );
}

// ──────────────────────────────────────────────────────────────────────────────
// 14. Duplicate invoice ID is rejected
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn test_integration_duplicate_invoice_id_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let ctx = setup(&env, 300, "INVDUP", 0, 0);

    let commitment = test_commitment(&env, "dup");
    ctx.escrow.create_escrow_legacy(
        &ctx.invoice_id,
        &ctx.seller,
        &ctx.payer,
        &1_000,
        &1_000,
        &99_999,
        &ctx.payment_token.address,
        &ctx.inv_token_id,
        &commitment,
        &None,
        &None,
    );

    let result = ctx.escrow.try_create_escrow_legacy(
        &ctx.invoice_id,
        &ctx.seller,
        &ctx.payer,
        &2_000,
        &2_000,
        &99_999,
        &ctx.payment_token.address,
        &ctx.inv_token_id,
        &commitment,
        &None,
    );
    assert_eq!(result, Err(Ok(errors::Error::EscrowExists)));
}

// ──────────────────────────────────────────────────────────────────────────────
// 15. Due date in the past / zero is rejected
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn test_integration_past_due_date_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(50_000);
    let ctx = setup(&env, 300, "INVPDD", 0, 0);

    // due_date = 49_999 < current timestamp (50_000) → must fail.
    let result = ctx.escrow.try_create_escrow_legacy(
        &ctx.invoice_id,
        &ctx.seller,
        &ctx.payer,
        &1_000,
        &1_000,
        &49_999,
        &ctx.payment_token.address,
        &ctx.inv_token_id,
        &test_commitment(&env, "past_due"),
        &None,
    );
    assert_eq!(result, Err(Ok(errors::Error::InvalidDueDate)));
}

#[test]
fn test_integration_zero_due_date_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let ctx = setup(&env, 300, "INVZDD", 0, 0);

    let result = ctx.escrow.try_create_escrow_legacy(
        &ctx.invoice_id,
        &ctx.seller,
        &ctx.payer,
        &1_000,
        &1_000,
        &0,
        &ctx.payment_token.address,
        &ctx.inv_token_id,
        &test_commitment(&env, "zero_due"),
        &None,
    );
    assert_eq!(result, Err(Ok(errors::Error::InvalidDueDate)));
}

// ──────────────────────────────────────────────────────────────────────────────
// 16. create_escrow on uninitialised contract fails
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn test_integration_create_escrow_not_initialized() {
    let env = Env::default();
    env.mock_all_auths();

    // Register escrow but do NOT initialize it.
    let escrow_id = env.register(InvoiceEscrow, ());
    let escrow = InvoiceEscrowClient::new(&env, &escrow_id);

    let seller = Address::generate(&env);
    let payer = Address::generate(&env);
    let token = Address::generate(&env);
    let inv_token = Address::generate(&env);

    let result = escrow.try_create_escrow_legacy(
        &Symbol::new(&env, "INV"),
        &seller,
        &payer,
        &1_000,
        &1_000,
        &99_999,
        &token,
        &inv_token,
        &test_commitment(&env, "no_init"),
        &None,
    );
    assert_eq!(result, Err(Ok(errors::Error::NotInit)));
}

// ──────────────────────────────────────────────────────────────────────────────
// 17. State persistence: get_escrow returns correct stored data
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn test_integration_state_persistence_after_create() {
    let env = Env::default();
    env.mock_all_auths();
    let ctx = setup(&env, 300, "INVPST", 0, 0);

    let commitment = test_commitment(&env, "persistence");
    ctx.escrow.create_escrow_legacy(
        &ctx.invoice_id,
        &ctx.seller,
        &ctx.payer,
        &2_000,
        &1_800,
        &88_888,
        &ctx.payment_token.address,
        &ctx.inv_token_id,
        &commitment,
        &None,
        &None,
    );

    let data = ctx.escrow.get_escrow(&ctx.invoice_id);
    assert_eq!(data.inv_id, ctx.invoice_id);
    assert_eq!(data.seller, ctx.seller);
    assert_eq!(data.debtor, ctx.payer);
    assert_eq!(data.face_value, 2_000);
    assert_eq!(data.purchase_price, 1_800);
    assert_eq!(data.funded_amt, 0);
    assert_eq!(data.due_dt, 88_888);
    assert_eq!(data.paid_amt, 0);
    assert_eq!(data.status, EscrowStatus::Created);
    assert_eq!(data.commitment, commitment);
    assert!(data.funder.is_none());
}

#[test]
fn test_integration_state_persistence_after_fund() {
    let env = Env::default();
    env.mock_all_auths();
    let ctx = setup(&env, 300, "INVPSF", 1_000, 0);

    create_and_fund(&ctx, 1_000, 99_999);

    let data = ctx.escrow.get_escrow(&ctx.invoice_id);
    assert_eq!(data.status, EscrowStatus::Funded);
    assert_eq!(data.funded_amt, 1_000);
    assert_eq!(data.funder, Some(ctx.buyer.clone()));
}

#[test]
fn test_integration_state_persistence_after_settlement() {
    let env = Env::default();
    env.mock_all_auths();
    let ctx = setup(&env, 300, "INVPSS", 1_000, 1_000);
    create_and_fund(&ctx, 1_000, 99_999);

    ctx.escrow
        .record_payment(&ctx.invoice_id, &ctx.payer, &1_000);

    let data = ctx.escrow.get_escrow(&ctx.invoice_id);
    assert_eq!(data.status, EscrowStatus::Settled);
    assert_eq!(data.paid_amt, 1_000);
}

#[test]
fn test_integration_state_persistence_after_refund() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(0);
    let ctx = setup(&env, 300, "INVPSR", 1_000, 0);
    create_and_fund(&ctx, 1_000, 5_000);

    env.ledger().set_timestamp(5_001);
    ctx.escrow.refund_escrow(&ctx.invoice_id);

    let data = ctx.escrow.get_escrow(&ctx.invoice_id);
    assert_eq!(data.status, EscrowStatus::Refunded);
}

// ──────────────────────────────────────────────────────────────────────────────
// 18. get_escrow on unknown invoice returns EscrowNotFound
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn test_integration_get_escrow_not_found() {
    let env = Env::default();
    env.mock_all_auths();
    let ctx = setup(&env, 300, "INVGNF", 0, 0);

    let result = ctx.escrow.try_get_escrow(&Symbol::new(&env, "MISSING"));
    assert_eq!(result, Err(Ok(errors::Error::EscrowNotFound)));
}

#[test]
fn test_integration_get_escrow_status_not_found() {
    let env = Env::default();
    env.mock_all_auths();
    let ctx = setup(&env, 300, "INVSGNF", 0, 0);

    let result = ctx
        .escrow
        .try_get_escrow_status(&Symbol::new(&env, "MISSING"));
    assert_eq!(result, Err(Ok(errors::Error::EscrowNotFound)));
}

// ──────────────────────────────────────────────────────────────────────────────
// 19. Commitment is immutable — stored value never changes after create
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn test_integration_commitment_immutable_after_lifecycle() {
    let env = Env::default();
    env.mock_all_auths();
    let ctx = setup(&env, 300, "INVCMT", 1_000, 1_000);

    let original = test_commitment(&env, "original_pdf_hash");
    ctx.escrow.create_escrow_legacy(
        &ctx.invoice_id,
        &ctx.seller,
        &ctx.payer,
        &1_000,
        &1_000,
        &99_999,
        &ctx.payment_token.address,
        &ctx.inv_token_id,
        &original,
        &None,
        &None,
    );

    // Fund.
    ctx.escrow.fund_escrow_legacy(&ctx.invoice_id, &ctx.buyer, &1_000);
    assert_eq!(ctx.escrow.get_escrow(&ctx.invoice_id).commitment, original);

    // Settle.
    ctx.escrow
        .record_payment(&ctx.invoice_id, &ctx.payer, &1_000);
    assert_eq!(ctx.escrow.get_escrow(&ctx.invoice_id).commitment, original);
}

// ──────────────────────────────────────────────────────────────────────────────
// 20. Two independent escrows coexist without interference
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn test_integration_two_independent_escrows() {
    let env = Env::default();
    env.mock_all_auths();

    // Escrow A
    let ctx_a = setup(&env, 300, "INVA01", 1_000, 1_000);
    create_and_fund(&ctx_a, 1_000, 99_999);

    // Escrow B — reuse the same escrow contract, different invoice
    let inv_b_id = Symbol::new(&env, "INVB01");

    let inv_token_b_id = env.register(InvoiceToken, ());
    let inv_token_b = InvoiceTokenClient::new(&env, &inv_token_b_id);
    inv_token_b.initialize(
        &ctx_a.admin,
        &SorobanString::from_str(&env, "Invoice B"),
        &SorobanString::from_str(&env, "INVB"),
        &7,
        &inv_b_id,
        &ctx_a.escrow_id,
    );

    let buyer_b = Address::generate(&env);
    let payer_b = Address::generate(&env);
    ctx_a.payment_asset.mint(&buyer_b, &500);
    ctx_a.payment_asset.mint(&payer_b, &500);

    ctx_a.escrow.create_escrow_legacy(
        &inv_b_id,
        &ctx_a.seller,
        &payer_b,
        &500,
        &500,
        &99_999,
        &ctx_a.payment_token.address,
        &inv_token_b_id,
        &test_commitment(&env, "inv_b"),
        &None,
        &None,
    );
    ctx_a.escrow.fund_escrow_legacy(&inv_b_id, &buyer_b, &500);

    // Settle A.
    ctx_a
        .escrow
        .record_payment(&ctx_a.invoice_id, &ctx_a.payer, &1_000);
    assert_eq!(
        ctx_a.escrow.get_escrow_status(&ctx_a.invoice_id),
        EscrowStatus::Settled
    );
    // B must remain Funded.
    assert_eq!(
        ctx_a.escrow.get_escrow_status(&inv_b_id),
        EscrowStatus::Funded
    );

    // Settle B.
    ctx_a.escrow.record_payment(&inv_b_id, &payer_b, &500);
    assert_eq!(
        ctx_a.escrow.get_escrow_status(&inv_b_id),
        EscrowStatus::Settled
    );

    // Both tokens unlocked independently.
    assert!(!ctx_a.inv_token.transfer_locked());
    assert!(!inv_token_b.transfer_locked());
}

fn parse_event(env: &Env, event: &soroban_sdk::xdr::ContractEvent) -> (Address, Vec<Val>, Val) {
    let contract_addr = match &event.contract_id {
        Some(hash) => Address::try_from_val(
            env,
            &soroban_sdk::xdr::ScVal::Address(soroban_sdk::xdr::ScAddress::Contract(hash.clone())),
        )
        .unwrap(),
        None => Address::generate(env),
    };
    let soroban_sdk::xdr::ContractEventBody::V0(v0) = &event.body;
    let topics = Vec::<Val>::try_from_val(
        env,
        &soroban_sdk::xdr::ScVal::Vec(Some(v0.topics.clone().into())),
    )
    .unwrap();
    let data = Val::try_from_val(env, &v0.data).unwrap();
    (contract_addr, topics, data)
}

#[test]
fn test_integration_escrow_created_event_emitted() {
    let env = Env::default();
    env.mock_all_auths();
    let ctx = setup(&env, 300, "INVECE", 0, 0);

    let commitment = test_commitment(&env, "event_emitted");
    ctx.escrow.create_escrow_legacy(
        &ctx.invoice_id,
        &ctx.seller,
        &ctx.payer,
        &1_000,
        &900,
        &55_555,
        &ctx.payment_token.address,
        &ctx.inv_token_id,
        &commitment,
        &None,
        &None,
    );

    let evts = env.events().all();
    let evt = evts
        .events()
        .iter()
        .rev()
        .find(|e| {
            let (_addr, topics, _data) = parse_event(&env, e);
            let topic_sym = Symbol::try_from_val(&env, &topics.get(0).unwrap()).unwrap();
            topic_sym == Symbol::new(&env, "escrow_created")
        })
        .expect("expected escrow_created event");
    let (_addr, topics, data) = parse_event(&env, evt);
    let topic_sym = Symbol::try_from_val(&env, &topics.get(0).unwrap()).unwrap();
    assert_eq!(topic_sym, Symbol::new(&env, "escrow_created"));

    let (inv_id, seller, debtor, fv, pp, dd, _tok, _inv_tok, cmt, _milestone): (
        Symbol,
        Address,
        Address,
        i128,
        i128,
        u64,
        Address,
        Address,
        BytesN<32>,
        Option<i128>,
    ) = data.try_into_val(&env).unwrap();
    assert_eq!(inv_id, ctx.invoice_id);
    assert_eq!(seller, ctx.seller);
    assert_eq!(debtor, ctx.payer);
    assert_eq!(fv, 1_000);
    assert_eq!(pp, 900);
    assert_eq!(dd, 55_555);
    assert_eq!(cmt, commitment);
}

#[test]
fn test_integration_escrow_cancelled_event_emitted() {
    let env = Env::default();
    env.mock_all_auths();
    let ctx = setup(&env, 300, "INVCNE", 0, 0);

    ctx.escrow.create_escrow_legacy(
        &ctx.invoice_id,
        &ctx.seller,
        &ctx.payer,
        &1_000,
        &1_000,
        &99_999,
        &ctx.payment_token.address,
        &ctx.inv_token_id,
        &test_commitment(&env, "cancel_event"),
        &None,
        &None,
    );
    ctx.escrow.cancel_escrow(&ctx.invoice_id, &ctx.seller);

    let evts = env.events().all();
    let evt = evts
        .events()
        .iter()
        .rev()
        .find(|e| {
            let (_addr, topics, _data) = parse_event(&env, e);
            let topic_sym = Symbol::try_from_val(&env, &topics.get(0).unwrap()).unwrap();
            topic_sym == Symbol::new(&env, "escrow_cancelled")
        })
        .expect("expected escrow_cancelled event");
    let (_addr, topics, data) = parse_event(&env, evt);
    let topic_sym = Symbol::try_from_val(&env, &topics.get(0).unwrap()).unwrap();
    assert_eq!(topic_sym, Symbol::new(&env, "escrow_cancelled"));

    let (inv_id, seller): (Symbol, Address) = data.try_into_val(&env).unwrap();
    assert_eq!(inv_id, ctx.invoice_id);
    assert_eq!(seller, ctx.seller);
}

#[test]
fn test_integration_payment_settled_event_emitted() {
    let env = Env::default();
    env.mock_all_auths();
    let ctx = setup(&env, 300, "INVPSE", 1_000, 1_000);
    create_and_fund(&ctx, 1_000, 99_999);
    ctx.escrow
        .record_payment(&ctx.invoice_id, &ctx.payer, &1_000);

    let evts = env.events().all();
    let evt = evts
        .events()
        .iter()
        .rev()
        .find(|e| {
            let (_addr, topics, _data) = parse_event(&env, e);
            let topic_sym = Symbol::try_from_val(&env, &topics.get(0).unwrap()).unwrap();
            topic_sym == Symbol::new(&env, "payment_settled")
        })
        .expect("expected payment_settled event");
    let (_addr, topics, data) = parse_event(&env, evt);
    let topic_sym = Symbol::try_from_val(&env, &topics.get(0).unwrap()).unwrap();
    assert_eq!(topic_sym, Symbol::new(&env, "payment_settled"));

    let (inv_id, amount, fee, investor): (Symbol, i128, i128, i128) =
        data.try_into_val(&env).unwrap();
    assert_eq!(inv_id, ctx.invoice_id);
    assert_eq!(amount, 1_000);
    assert_eq!(fee, 30); // 3% of 1000
    assert_eq!(investor, 970);
}

#[test]
fn test_integration_escrow_refunded_event_emitted() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(0);
    let ctx = setup(&env, 300, "INVREF", 1_000, 0);
    create_and_fund(&ctx, 1_000, 5_000);
    env.ledger().set_timestamp(5_001);
    ctx.escrow.refund_escrow(&ctx.invoice_id);

    let evts = env.events().all();
    let evt = evts
        .events()
        .iter()
        .rev()
        .find(|e| {
            let (_addr, topics, _data) = parse_event(&env, e);
            let topic_sym = Symbol::try_from_val(&env, &topics.get(0).unwrap()).unwrap();
            topic_sym == Symbol::new(&env, "escrow_refunded")
        })
        .expect("expected escrow_refunded event");
    let (_addr, topics, data) = parse_event(&env, evt);
    let topic_sym = Symbol::try_from_val(&env, &topics.get(0).unwrap()).unwrap();
    assert_eq!(topic_sym, Symbol::new(&env, "escrow_refunded"));

    let (inv_id, amount): (Symbol, i128) = data.try_into_val(&env).unwrap();
    assert_eq!(inv_id, ctx.invoice_id);
    assert_eq!(amount, 1_000);
}

// ──────────────────────────────────────────────────────────────────────────────
// 22. Face value vs purchase price discount — seller gets face, buyer gets net
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn test_integration_discounted_purchase_price() {
    let env = Env::default();
    env.mock_all_auths();

    // face_value = 900 == purchase_price = 900 (discount reflected externally).
    // Use 0% fee so all math is clean.  Buyer funds 900; payer pays 900.
    // Escrow in: 900 (buyer) + 900 (payer) = 1800.
    // Escrow out: 900 (investor, 0% fee) + 900 (seller release) = 1800.
    let ctx = setup(&env, 0, "INVDSC", 900, 900);

    ctx.escrow.create_escrow_legacy(
        &ctx.invoice_id,
        &ctx.seller,
        &ctx.payer,
        &900, // face_value
        &900, // purchase_price
        &99_999,
        &ctx.payment_token.address,
        &ctx.inv_token_id,
        &test_commitment(&env, "discount"),
        &None,
        &None,
    );
    ctx.escrow.fund_escrow_legacy(&ctx.invoice_id, &ctx.buyer, &900);

    assert_eq!(ctx.payment_token.balance(&ctx.buyer), 0);
    assert_eq!(ctx.payment_token.balance(&ctx.escrow_id), 900);
    assert_eq!(
        ctx.escrow.get_escrow_status(&ctx.invoice_id),
        EscrowStatus::Funded
    );

    // Payer settles the invoice.
    ctx.escrow.record_payment(&ctx.invoice_id, &ctx.payer, &900);

    // 0% fee: investor gets 900, admin gets 0, seller gets 900.
    assert_eq!(ctx.payment_token.balance(&ctx.admin), 0);
    assert_eq!(ctx.payment_token.balance(&ctx.buyer), 900);
    assert_eq!(ctx.payment_token.balance(&ctx.seller), 900);
    assert_eq!(ctx.payment_token.balance(&ctx.escrow_id), 0);
}

// ──────────────────────────────────────────────────────────────────────────────
// 23. Refund before due_date strictly blocked; at due_dt it is allowed
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn test_integration_refund_at_exact_due_date_blocked() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(0);
    let ctx = setup(&env, 300, "INVBND", 1_000, 0);
    create_and_fund(&ctx, 1_000, 5_000);

    // One second before due_date: must fail.
    env.ledger().set_timestamp(4_999);
    let result = ctx.escrow.try_refund_escrow(&ctx.invoice_id);
    assert_eq!(result, Err(Ok(errors::Error::RefundNotAllowed)));

    // At exactly due_date (ledger_ts == due_dt): contract allows refund
    // because it checks `ledger_ts < due_dt` — false when equal.
    env.ledger().set_timestamp(5_000);
    ctx.escrow.refund_escrow(&ctx.invoice_id);
    assert_eq!(
        ctx.escrow.get_escrow_status(&ctx.invoice_id),
        EscrowStatus::Refunded
    );
}

// ──────────────────────────────────────────────────────────────────────────────
// 24. Double refund rejected
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn test_integration_double_refund_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(0);
    let ctx = setup(&env, 300, "INVDR", 1_000, 0);
    create_and_fund(&ctx, 1_000, 5_000);

    env.ledger().set_timestamp(5_001);
    ctx.escrow.refund_escrow(&ctx.invoice_id);

    let result = ctx.escrow.try_refund_escrow(&ctx.invoice_id);
    assert_eq!(result, Err(Ok(errors::Error::RefundNotAllowed)));
}

// ──────────────────────────────────────────────────────────────────────────────
// 25. get_config returns current admin and fee
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn test_integration_get_config_returns_correct_values() {
    let env = Env::default();
    env.mock_all_auths();
    let ctx = setup(&env, 750, "INVCFG", 0, 0);

    let cfg = ctx.escrow.get_config();
    assert_eq!(cfg.admin, ctx.admin);
    assert_eq!(cfg.fee_bps, 750);
    assert!(!cfg.paused);
    assert!(cfg.payment_distributor.is_none());
}

// -----------------------------------------------------------------------------
// 26. Cancellation after partial payment is rejected without changing state
// -----------------------------------------------------------------------------

#[test]
fn test_integration_cancel_after_partial_payment_preserves_state() {
    let env = Env::default();
    env.mock_all_auths();
    let ctx = setup(&env, 300, "INVCPP", 1_000, 1_000);
    create_and_fund(&ctx, 1_000, 99_999);

    ctx.escrow.record_payment(&ctx.invoice_id, &ctx.payer, &400);

    let result = ctx.escrow.try_cancel_escrow(&ctx.invoice_id, &ctx.seller);
    assert_eq!(result, Err(Ok(errors::Error::EscrowFunded)));

    let data = ctx.escrow.get_escrow(&ctx.invoice_id);
    assert_eq!(data.status, EscrowStatus::Funded);
    assert_eq!(data.funded_amt, 1_000);
    assert_eq!(data.paid_amt, 400);
    assert_eq!(ctx.payment_token.balance(&ctx.payer), 600);
    assert_eq!(ctx.payment_token.balance(&ctx.escrow_id), 600);
    assert_eq!(ctx.payment_token.balance(&ctx.seller), 400);
    assert_eq!(ctx.payment_token.balance(&ctx.admin), 12);
    assert_eq!(ctx.payment_token.balance(&ctx.buyer), 388);
    assert!(ctx.inv_token.transfer_locked());
}

// ──────────────────────────────────────────────────────────────────────────────
// Issue #336: Pause blocks settlement and refund
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn test_integration_pause_blocks_settlement() {
    let env = Env::default();
    env.mock_all_auths();
    let ctx = setup(&env, 300, "INVPAUSES", 1_000, 1_000);
    create_and_fund(&ctx, 1_000, 99_999);

    ctx.escrow.set_paused(&true);

    let result = ctx
        .escrow
        .try_record_payment(&ctx.invoice_id, &ctx.payer, &1_000);
    assert_eq!(result, Err(Ok(errors::Error::Paused)));

    assert_eq!(
        ctx.escrow.get_escrow_status(&ctx.invoice_id),
        EscrowStatus::Funded
    );
    assert_eq!(ctx.payment_token.balance(&ctx.escrow_id), 1_000);
}

#[test]
fn test_integration_pause_blocks_refund_after_deadline() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(5_000);
    let ctx = setup(&env, 300, "INVPAUSER", 1_000, 0);
    create_and_fund(&ctx, 1_000, 99_999);

    env.ledger().set_timestamp(100_000);

    ctx.escrow.set_paused(&true);

    let result = ctx.escrow.try_refund_escrow(&ctx.invoice_id);
    assert_eq!(result, Err(Ok(errors::Error::Paused)));

    assert_eq!(
        ctx.escrow.get_escrow_status(&ctx.invoice_id),
        EscrowStatus::Funded
    );
    assert_eq!(ctx.payment_token.balance(&ctx.escrow_id), 1_000);
}

#[test]
fn test_integration_unpause_restores_behavior() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(5_000);
    let ctx = setup(&env, 300, "INVUNP", 1_000, 1_000);
    create_and_fund(&ctx, 1_000, 99_999);

    ctx.escrow.set_paused(&true);
    let result = ctx
        .escrow
        .try_record_payment(&ctx.invoice_id, &ctx.payer, &1_000);
    assert_eq!(result, Err(Ok(errors::Error::Paused)));

    ctx.escrow.set_paused(&false);
    ctx.escrow
        .record_payment(&ctx.invoice_id, &ctx.payer, &1_000);

    assert_eq!(
        ctx.escrow.get_escrow_status(&ctx.invoice_id),
        EscrowStatus::Settled
    );
}

/// Emergency pause must preserve the funded escrow and token lock, then allow
/// normal settlement and token recovery after the circuit breaker is released.
#[test]
fn test_integration_circuit_breaker_pause_and_recovery_drill() {
    let env = Env::default();
    env.mock_all_auths();
    let ctx = setup(&env, 300, "INVCIRCUIT", 1_000, 1_000);
    create_and_fund(&ctx, 1_000, 99_999);

    assert!(ctx.inv_token.transfer_locked());
    assert_eq!(ctx.payment_token.balance(&ctx.escrow_id), 1_000);

    // Exercise both pause switches as an emergency operator would. The token
    // contract is separately paused to ensure that unpausing only escrow does
    // not accidentally resume token movement.
    ctx.escrow.set_paused(&true);
    ctx.inv_token.set_paused(&true);
    assert!(ctx.escrow.paused());
    assert!(ctx.inv_token.paused());

    let blocked_payment = ctx
        .escrow
        .try_record_payment(&ctx.invoice_id, &ctx.payer, &1_000);
    assert_eq!(blocked_payment, Err(Ok(errors::Error::Paused)));
    assert_eq!(ctx.escrow.get_escrow_status(&ctx.invoice_id), EscrowStatus::Funded);
    assert_eq!(ctx.payment_token.balance(&ctx.escrow_id), 1_000);
    assert_eq!(ctx.payment_token.balance(&ctx.seller), 0);

    ctx.escrow.set_paused(&false);
    assert!(!ctx.escrow.paused());
    assert!(ctx.inv_token.paused());
    ctx.inv_token.set_paused(&false);
    assert!(!ctx.inv_token.paused());

    ctx.escrow.record_payment(&ctx.invoice_id, &ctx.payer, &1_000);
    assert_eq!(ctx.escrow.get_escrow_status(&ctx.invoice_id), EscrowStatus::Settled);
    assert_eq!(ctx.payment_token.balance(&ctx.escrow_id), 0);
    assert_eq!(ctx.payment_token.balance(&ctx.seller), 1_000);
    assert_eq!(ctx.payment_token.balance(&ctx.buyer), 970);
    assert_eq!(ctx.payment_token.balance(&ctx.admin), 30);
    assert!(!ctx.inv_token.transfer_locked());
}

/// A stale dispute is resolved through the timeout fallback across escrow and
/// invoice-token contracts. Even a seller-favouring resolution request after
/// timeout must return the funder's held collateral and unlock the invoice token.
#[test]
fn test_integration_dispute_timeout_refunds_buyer_and_recovers_token() {
    let env = Env::default();
    env.mock_all_auths();
    let ctx = setup(&env, 300, "INVDISPTIME", 1_000, 1_000);
    create_and_fund(&ctx, 1_000, 99_999);

    ctx.escrow.raise_dispute(
        &ctx.payer,
        &ctx.invoice_id,
        &soroban_sdk::Bytes::from_slice(&env, b"delivery not received"),
    );
    assert_eq!(ctx.escrow.get_escrow_status(&ctx.invoice_id), EscrowStatus::Disputed);
    assert!(ctx.inv_token.transfer_locked());

    let blocked_payment = ctx
        .escrow
        .try_record_payment(&ctx.invoice_id, &ctx.payer, &1_000);
    assert_eq!(blocked_payment, Err(Ok(errors::Error::InvalidInvoiceStatus)));
    assert_eq!(ctx.payment_token.balance(&ctx.escrow_id), 1_000);

    let raised_at = env.ledger().timestamp();
    env.ledger().set_timestamp(raised_at + 604_801);
    ctx.escrow.resolve_dispute(
        &ctx.admin,
        &ctx.invoice_id,
        &Symbol::new(&env, "seller"),
    );

    assert_eq!(ctx.escrow.get_escrow_status(&ctx.invoice_id), EscrowStatus::Refunded);
    assert_eq!(ctx.payment_token.balance(&ctx.buyer), 1_000);
    assert_eq!(ctx.payment_token.balance(&ctx.seller), 0);
    assert_eq!(ctx.payment_token.balance(&ctx.escrow_id), 0);
    assert!(!ctx.inv_token.transfer_locked());
}

// ──────────────────────────────────────────────────────────────────────────────
// Issue #390: Comprehensive refund test suite
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn test_integration_refund_after_deadline() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(5_000);
    let ctx = setup(&env, 300, "INVREFD", 1_000, 0);
    create_and_fund(&ctx, 1_000, 99_999);

    env.ledger().set_timestamp(100_000);

    ctx.escrow.refund_escrow(&ctx.invoice_id);

    assert_eq!(
        ctx.escrow.get_escrow_status(&ctx.invoice_id),
        EscrowStatus::Refunded
    );
    assert_eq!(ctx.payment_token.balance(&ctx.buyer), 1_000);
    assert_eq!(ctx.payment_token.balance(&ctx.escrow_id), 0);
    assert!(!ctx.inv_token.transfer_locked());
}

#[test]
fn test_integration_refund_before_deadline_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(5_000);
    let ctx = setup(&env, 300, "INVREFB", 1_000, 0);
    create_and_fund(&ctx, 1_000, 99_999);

    let result = ctx.escrow.try_refund_escrow(&ctx.invoice_id);
    assert_eq!(result, Err(Ok(errors::Error::RefundNotAllowed)));
}

#[test]
fn test_integration_partial_payment_refund() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(5_000);
    let ctx = setup(&env, 300, "INVREFP", 1_000, 1_000);
    create_and_fund(&ctx, 1_000, 99_999);

    // Partial payment of 400 (3% fee = 12, investor gets 388, seller gets 400)
    ctx.escrow.record_payment(&ctx.invoice_id, &ctx.payer, &400);
    assert_eq!(ctx.payment_token.balance(&ctx.escrow_id), 600);

    env.ledger().set_timestamp(100_000);

    ctx.escrow.refund_escrow(&ctx.invoice_id);

    // Refund = purchase_price - paid_amt = 1000 - 400 = 600
    // Buyer already received 388 from partial payment (400 - 12 fee)
    assert_eq!(ctx.payment_token.balance(&ctx.buyer), 988);
    assert_eq!(ctx.payment_token.balance(&ctx.escrow_id), 0);
    assert_eq!(
        ctx.escrow.get_escrow_status(&ctx.invoice_id),
        EscrowStatus::Refunded
    );
}

#[test]
fn test_integration_duplicate_refund_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(5_000);
    let ctx = setup(&env, 300, "INVREFDD", 1_000, 0);
    create_and_fund(&ctx, 1_000, 99_999);

    env.ledger().set_timestamp(100_000);
    ctx.escrow.refund_escrow(&ctx.invoice_id);

    let result = ctx.escrow.try_refund_escrow(&ctx.invoice_id);
    assert_eq!(result, Err(Ok(errors::Error::RefundNotAllowed)));
}

#[test]
fn test_integration_refund_restores_capacity() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(5_000);
    let ctx = setup(&env, 300, "INVREFC", 1_000, 0);
    create_and_fund(&ctx, 1_000, 99_999);

    env.ledger().set_timestamp(100_000);
    ctx.escrow.refund_escrow(&ctx.invoice_id);

    // Buyer can fund again after refund
    let ctx2 = setup(&env, 300, "INVREFC2", 1_000, 0);
    ctx2.escrow.create_escrow_legacy(
        &ctx2.invoice_id,
        &ctx2.seller,
        &ctx2.payer,
        &1_000,
        &1_000,
        &200_000,
        &ctx2.payment_token.address,
        &ctx2.inv_token_id,
        &test_commitment(&ctx2.env, "commitment2"),
        &None,
        &None,
    );
    ctx2.escrow
        .fund_escrow_legacy(&ctx2.invoice_id, &ctx2.buyer, &1_000);
    assert_eq!(ctx2.payment_token.balance(&ctx2.buyer), 0);
    assert_eq!(ctx2.payment_token.balance(&ctx2.escrow_id), 1_000);
}

// ──────────────────────────────────────────────────────────────────────────────
// 27. Referral contract integration
// ──────────────────────────────────────────────────────────────────────────────

/// Register a referral contract, initialize it, bind the escrow as its only
/// allowed caller, and bind the referral contract on the escrow.
fn wire_referral<'a>(ctx: &'a Ctx<'a>, referral_fee_bps: u32) -> ReferralClient<'a> {
    let referral_id = ctx.env.register(Referral, ());
    let referral = ReferralClient::new(&ctx.env, &referral_id);
    referral.initialize(&ctx.admin, &referral_fee_bps, &ctx.payment_token.address);
    referral.set_escrow(&ctx.admin, &Some(ctx.escrow_id.clone()));
    ctx.escrow.set_referral_contract(&referral_id);
    referral
}

#[test]
fn test_integration_referral_fee_accrues_on_funding() {
    let env = Env::default();
    env.mock_all_auths();
    let ctx = setup(&env, 300, "INVREF1", 1_000, 0);

    let referral = wire_referral(&ctx, 500); // 5% referral fee
    let referrer = Address::generate(&env);
    referral.register_referral(&ctx.buyer, &referrer);

    create_and_fund(&ctx, 1_000, 99_999);

    // 5% of 1_000 = 50 accrued to the referrer.
    let stats = referral.get_referral_stats(&referrer);
    assert_eq!(stats.total_earned, 50);
    assert_eq!(stats.pending, 50);
    assert_eq!(stats.referee_count, 1);

    // Funding itself succeeded normally.
    assert_eq!(
        ctx.escrow.get_escrow_status(&ctx.invoice_id),
        EscrowStatus::Funded
    );
    assert_eq!(ctx.payment_token.balance(&ctx.escrow_id), 1_000);
}

#[test]
fn test_integration_referral_fees_accumulate_across_investments() {
    let env = Env::default();
    env.mock_all_auths();
    let ctx = setup(&env, 300, "INVREF2", 1_000, 0);

    let referral = wire_referral(&ctx, 500);
    let referrer = Address::generate(&env);
    referral.register_referral(&ctx.buyer, &referrer);

    ctx.escrow.create_escrow_legacy(
        &ctx.invoice_id,
        &ctx.seller,
        &ctx.payer,
        &1_000,
        &1_000,
        &99_999,
        &ctx.payment_token.address,
        &ctx.inv_token_id,
        &test_commitment(&env, "commitment_ref2"),
        &None,
        &None,
    );

    // Two separate investments by the same referee.
    ctx.escrow.fund_escrow_legacy(&ctx.invoice_id, &ctx.buyer, &400);
    ctx.escrow.fund_escrow_legacy(&ctx.invoice_id, &ctx.buyer, &600);

    // 5% of 400 = 20, 5% of 600 = 30 → 50 total.
    let stats = referral.get_referral_stats(&referrer);
    assert_eq!(stats.total_earned, 50);
    assert_eq!(stats.pending, 50);
    assert_eq!(stats.referee_count, 1);
    assert_eq!(
        ctx.escrow.get_escrow_status(&ctx.invoice_id),
        EscrowStatus::Funded
    );
}

#[test]
fn test_integration_referral_no_referrer_is_noop_for_funding() {
    let env = Env::default();
    env.mock_all_auths();
    let ctx = setup(&env, 300, "INVREF3", 1_000, 0);

    let referral = wire_referral(&ctx, 500);
    let referrer = Address::generate(&env);

    // Buyer has no registered referrer → funding still succeeds, nothing accrues.
    create_and_fund(&ctx, 1_000, 99_999);

    let stats = referral.get_referral_stats(&referrer);
    assert_eq!(stats.total_earned, 0);
    assert_eq!(stats.pending, 0);
    assert_eq!(stats.referee_count, 0);
    assert_eq!(
        ctx.escrow.get_escrow_status(&ctx.invoice_id),
        EscrowStatus::Funded
    );
}

#[test]
fn test_integration_referral_misconfigured_does_not_block_funding() {
    let env = Env::default();
    env.mock_all_auths();
    let ctx = setup(&env, 300, "INVREF4", 1_000, 0);

    // Point the escrow at an uninitialized referral contract: the accrual call
    // fails, is swallowed (best-effort), and funding must still succeed.
    let broken_referral_id = env.register(Referral, ());
    ctx.escrow.set_referral_contract(&broken_referral_id);

    create_and_fund(&ctx, 1_000, 99_999);

    assert_eq!(
        ctx.escrow.get_escrow_status(&ctx.invoice_id),
        EscrowStatus::Funded
    );
    assert_eq!(ctx.payment_token.balance(&ctx.escrow_id), 1_000);
}

#[test]
fn test_integration_referral_unbound_caller_rejected_but_funding_continues() {
    let env = Env::default();
    env.mock_all_auths();
    let ctx = setup(&env, 300, "INVREF5", 1_000, 0);

    let referral_id = env.register(Referral, ());
    let referral = ReferralClient::new(&env, &referral_id);
    referral.initialize(&ctx.admin, &500, &ctx.payment_token.address);

    // Bind a *different* escrow as the only allowed caller → accrual rejected,
    // but the investment itself is unaffected.
    let other_escrow = Address::generate(&env);
    referral.set_escrow(&ctx.admin, &Some(other_escrow));
    ctx.escrow.set_referral_contract(&referral_id);

    let referrer = Address::generate(&env);
    referral.register_referral(&ctx.buyer, &referrer);

    create_and_fund(&ctx, 1_000, 99_999);

    let stats = referral.get_referral_stats(&referrer);
    assert_eq!(stats.pending, 0); // rejected accrual accrued nothing
    assert_eq!(
        ctx.escrow.get_escrow_status(&ctx.invoice_id),
        EscrowStatus::Funded
    );
}

#[test]
fn test_integration_clear_referral_contract_stops_accrual() {
    let env = Env::default();
    env.mock_all_auths();
    let ctx = setup(&env, 300, "INVREF6", 1_000, 0);

    let referral = wire_referral(&ctx, 500);
    let referrer = Address::generate(&env);
    referral.register_referral(&ctx.buyer, &referrer);

    ctx.escrow.create_escrow_legacy(
        &ctx.invoice_id,
        &ctx.seller,
        &ctx.payer,
        &1_000,
        &1_000,
        &99_999,
        &ctx.payment_token.address,
        &ctx.inv_token_id,
        &test_commitment(&env, "commitment_ref6"),
        &None,
        &None,
    );

    ctx.escrow.fund_escrow_legacy(&ctx.invoice_id, &ctx.buyer, &400);
    assert_eq!(referral.get_referral_stats(&referrer).pending, 20);

    // Clear the binding: further investments no longer accrue.
    ctx.escrow.clear_referral_contract();
    assert!(ctx.escrow.get_config().referral_contract.is_none());

    ctx.escrow.fund_escrow_legacy(&ctx.invoice_id, &ctx.buyer, &600);
    assert_eq!(referral.get_referral_stats(&referrer).pending, 20);
    assert_eq!(
        ctx.escrow.get_escrow_status(&ctx.invoice_id),
        EscrowStatus::Funded
    );
}

#[test]
fn test_integration_set_referral_contract_admin_only() {
    let env = Env::default();
    env.mock_all_auths();
    let ctx = setup(&env, 300, "INVREF7", 0, 0);

    let referral_id = env.register(Referral, ());

    // Without admin authorization the binding fails.
    env.set_auths(&[]);
    let result = ctx.escrow.try_set_referral_contract(&referral_id);
    assert!(result.is_err());
    assert!(ctx.escrow.get_config().referral_contract.is_none());

    // With admin auth the binding succeeds.
    env.mock_all_auths();
    ctx.escrow.set_referral_contract(&referral_id);
    assert_eq!(ctx.escrow.get_config().referral_contract, Some(referral_id));
}
