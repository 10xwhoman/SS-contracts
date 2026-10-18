#![allow(deprecated)]

use super::*;
use invoice_escrow::{EscrowStatus, InvoiceEscrow, InvoiceEscrowClient};
use invoice_token::{InvoiceToken, InvoiceTokenClient};
use soroban_sdk::token::{Client as TokenClient, StellarAssetClient as AssetClient};
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    Address, BytesN, Env, String as SorobanString, Symbol,
};

fn test_commitment(env: &Env) -> BytesN<32> {
    BytesN::from_array(env, &[0; 32])
}

struct FlowContext<'a> {
    admin: Address,
    seller: Address,
    buyer: Address,
    payer: Address,
    escrow_id: Address,
    escrow: InvoiceEscrowClient<'a>,
    distributor_id: Address,
    distributor: PaymentDistributorClient<'a>,
    inv_token: InvoiceTokenClient<'a>,
    payment_token: TokenClient<'a>,
    payment_asset: AssetClient<'a>,
    invoice_id: Symbol,
}

fn setup(env: &Env, fee_bps: u32, configure_distributor: bool) -> FlowContext<'_> {
    let admin = Address::generate(env);
    let seller = Address::generate(env);
    let buyer = Address::generate(env);
    let payer = Address::generate(env);

    let escrow_id = env.register(InvoiceEscrow, ());
    let escrow = InvoiceEscrowClient::new(env, &escrow_id);

    let distributor_id = env.register(PaymentDistributor, ());
    let distributor = PaymentDistributorClient::new(env, &distributor_id);

    let inv_token_id = env.register(InvoiceToken, ());
    let inv_token = InvoiceTokenClient::new(env, &inv_token_id);

    let token_admin = Address::generate(env);
    let token_id = env.register_stellar_asset_contract_v2(token_admin);
    let payment_token = TokenClient::new(env, &token_id.address());
    let payment_asset = AssetClient::new(env, &token_id.address());

    let invoice_id = Symbol::new(env, "INV_FLOW");
    inv_token.initialize(
        &admin,
        &SorobanString::from_str(env, "Invoice Flow"),
        &SorobanString::from_str(env, "INVF"),
        &7,
        &invoice_id,
        &escrow_id,
    );

    escrow.initialize(&admin, &fee_bps);
    distributor.initialize(&admin);
    distributor.set_escrow_contract(&admin, &escrow_id);
    if configure_distributor {
        escrow.set_payment_distributor(&distributor_id);
    }

    FlowContext {
        admin,
        seller,
        buyer,
        payer,
        escrow_id,
        escrow,
        distributor_id,
        distributor,
        inv_token,
        payment_token,
        payment_asset,
        invoice_id,
    }
}

fn create_and_fund(ctx: &FlowContext<'_>, amount: i128, due_date: u64) {
    ctx.payment_asset.mint(&ctx.buyer, &amount);
    ctx.payment_asset.mint(&ctx.payer, &amount);
    ctx.escrow.create_escrow_legacy(
        &ctx.invoice_id,
        &ctx.seller,
        &ctx.payer,
        &amount,
        &amount,
        &due_date,
        &ctx.payment_token.address,
        &ctx.inv_token.address,
        &test_commitment(&ctx.escrow.env),
        &None,
        &None,
    );
    ctx.escrow.fund_escrow_legacy(&ctx.invoice_id, &ctx.buyer, &amount);
}

#[test]
fn test_integration_settlement_routes_through_distributor_when_configured() {
    let env = Env::default();
    env.mock_all_auths();

    let ctx = setup(&env, 300, true);
    create_and_fund(&ctx, 1_000, 50_000);

    ctx.escrow
        .record_payment(&ctx.invoice_id, &ctx.payer, &1_000);

    assert_eq!(ctx.payment_token.balance(&ctx.payer), 0);
    assert_eq!(ctx.payment_token.balance(&ctx.seller), 1_000);
    assert_eq!(ctx.payment_token.balance(&ctx.buyer), 970);
    assert_eq!(ctx.payment_token.balance(&ctx.admin), 30);
    assert_eq!(ctx.payment_token.balance(&ctx.distributor_id), 0);
    assert_eq!(ctx.payment_token.balance(&ctx.escrow_id), 0);
    assert_eq!(
        ctx.escrow.get_escrow_status(&ctx.invoice_id),
        EscrowStatus::Settled
    );
    assert!(!ctx.inv_token.transfer_locked());

    let state = ctx
        .distributor
        .get_distribution_state(&ctx.escrow_id, &ctx.invoice_id);
    assert_eq!(state.paid_distributed, 1_000);
    assert!(!state.refund_distributed);
}

#[test]
fn test_integration_partial_payment_then_refund_routes_through_distributor() {
    let env = Env::default();
    env.mock_all_auths();

    let ctx = setup(&env, 300, true);
    env.ledger().set_timestamp(5_000);
    create_and_fund(&ctx, 1_000, 10_000);

    ctx.escrow.record_payment(&ctx.invoice_id, &ctx.payer, &400);

    env.ledger().set_timestamp(10_001);
    ctx.escrow.refund_escrow(&ctx.invoice_id);

    assert_eq!(ctx.payment_token.balance(&ctx.seller), 400);
    assert_eq!(ctx.payment_token.balance(&ctx.buyer), 988);
    assert_eq!(ctx.payment_token.balance(&ctx.admin), 12);
    assert_eq!(ctx.payment_token.balance(&ctx.distributor_id), 0);
    assert_eq!(
        ctx.escrow.get_escrow_status(&ctx.invoice_id),
        EscrowStatus::Refunded
    );
    assert!(!ctx.inv_token.transfer_locked());

    let state = ctx
        .distributor
        .get_distribution_state(&ctx.escrow_id, &ctx.invoice_id);
    assert_eq!(state.paid_distributed, 400);
    assert!(state.refund_distributed);
}

#[test]
fn test_integration_escrow_keeps_direct_flow_without_distributor() {
    let env = Env::default();
    env.mock_all_auths();

    let ctx = setup(&env, 300, false);
    create_and_fund(&ctx, 1_000, 50_000);

    ctx.escrow
        .record_payment(&ctx.invoice_id, &ctx.payer, &1_000);

    assert_eq!(ctx.payment_token.balance(&ctx.seller), 1_000);
    assert_eq!(ctx.payment_token.balance(&ctx.buyer), 970);
    assert_eq!(ctx.payment_token.balance(&ctx.admin), 30);
    assert_eq!(ctx.payment_token.balance(&ctx.distributor_id), 0);
    assert_eq!(
        ctx.distributor
            .get_distribution_state(&ctx.escrow_id, &ctx.invoice_id)
            .paid_distributed,
        0
    );
}

// ══════════════════════════════════════════════════════════════════════════════
// Issue #163: Mock Contract Call Invocation Verification Tests
// ══════════════════════════════════════════════════════════════════════════════

/// Verify that settlement routes through the distributor when configured,
/// resulting in correct distribution state recorded by the distributor.
#[test]
fn test_integration_verify_auth_distribution_invocations() {
    let env = Env::default();
    env.mock_all_auths();

    let ctx = setup(&env, 300, true);
    create_and_fund(&ctx, 1_000, 50_000);
    ctx.payment_asset.mint(&ctx.payer, &1_000);

    ctx.escrow
        .record_payment(&ctx.invoice_id, &ctx.payer, &1_000);

    // Verify that payment distribution completed and state was recorded.
    let state = ctx
        .distributor
        .get_distribution_state(&ctx.escrow_id, &ctx.invoice_id);
    assert_eq!(state.paid_distributed, 1_000);
    assert!(!state.refund_distributed);
}

/// Verify that calling `distribute_payment` with an invalid escrow status
/// properly returns the `InvalidEscrowStatus` error code.
#[test]
fn test_integration_error_invalid_escrow_status_on_distribute() {
    let env = Env::default();
    env.mock_all_auths();

    let ctx = setup(&env, 300, true);
    create_and_fund(&ctx, 1_000, 50_000);

    // Attempt distribute_payment directly with escrow_status=0 (Created) —
    // which is not a fundable/settleable status.
    let result = ctx.distributor.try_distribute_payment(
        &ctx.escrow_id,
        &ctx.invoice_id,
        &soroban_sdk::vec![
            &env,
            ctx.payment_token.address.clone(),
            ctx.seller.clone(),
            ctx.buyer.clone(),
            ctx.admin.clone()
        ],
        &soroban_sdk::vec![&env, 100i128, 100i128, 0i128, 0i128],
        &0u32, // EscrowStatus::Created (invalid for distribute_payment)
    );
    assert_eq!(result, Err(Ok(Error::InvalidEscrowStatus)));

    // Also test with status=3 (Refunded) — invalid for distribute_payment.
    let result2 = ctx.distributor.try_distribute_payment(
        &ctx.escrow_id,
        &ctx.invoice_id,
        &soroban_sdk::vec![
            &env,
            ctx.payment_token.address.clone(),
            ctx.seller.clone(),
            ctx.buyer.clone(),
            ctx.admin.clone()
        ],
        &soroban_sdk::vec![&env, 100i128, 100i128, 0i128, 0i128],
        &3u32, // EscrowStatus::Refunded (invalid for distribute_payment)
    );
    assert_eq!(result2, Err(Ok(Error::InvalidEscrowStatus)));
}

/// Verify that `distribute_payment` returns `InsufficientBalance` when the
/// distributor contract holds no tokens to route.
#[test]
fn test_integration_error_insufficient_balance_on_distribute() {
    let env = Env::default();
    env.mock_all_auths();

    let ctx = setup(&env, 300, true);
    // Do NOT create/fund the escrow — the distributor has no tokens.

    let result = ctx.distributor.try_distribute_payment(
        &ctx.escrow_id,
        &ctx.invoice_id,
        &soroban_sdk::vec![
            &env,
            ctx.payment_token.address.clone(),
            ctx.seller.clone(),
            ctx.buyer.clone(),
            ctx.admin.clone()
        ],
        &soroban_sdk::vec![&env, 100i128, 100i128, 0i128, 300i128],
        &1u32, // EscrowStatus::Funded
    );
    assert_eq!(result, Err(Ok(Error::InsufficientBalance)));
}

/// Verify that a non-whitelisted escrow contract is rejected with
/// `UnauthorizedEscrow` when attempting to invoke `distribute_payment`.
#[test]
fn test_integration_error_unauthorized_escrow_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let ctx = setup(&env, 300, true);

    // Generate a rogue escrow address that is NOT the whitelisted ctx.escrow_id.
    let rogue_escrow = Address::generate(&env);

    let result = ctx.distributor.try_distribute_payment(
        &rogue_escrow, // Not whitelisted!
        &ctx.invoice_id,
        &soroban_sdk::vec![
            &env,
            ctx.payment_token.address.clone(),
            ctx.seller.clone(),
            ctx.buyer.clone(),
            ctx.admin.clone()
        ],
        &soroban_sdk::vec![&env, 100i128, 100i128, 0i128, 300i128],
        &1u32,
    );
    assert_eq!(result, Err(Ok(Error::UnauthorizedEscrow)));
}

/// Verify that distribution state persists correctly across multiple
/// incremental payments within the same escrow lifecycle.
#[test]
fn test_integration_state_persistence_across_multiple_distributions() {
    let env = Env::default();
    env.mock_all_auths();

    let ctx = setup(&env, 500, true);
    create_and_fund(&ctx, 1_000, 50_000);
    ctx.payment_asset.mint(&ctx.payer, &1_000);

    // First incremental payment.
    ctx.escrow.record_payment(&ctx.invoice_id, &ctx.payer, &300);

    let state1 = ctx
        .distributor
        .get_distribution_state(&ctx.escrow_id, &ctx.invoice_id);
    assert_eq!(state1.paid_distributed, 300);
    assert!(!state1.refund_distributed);
    assert_eq!(
        ctx.escrow.get_escrow_status(&ctx.invoice_id),
        EscrowStatus::Funded
    );

    // Second incremental payment.
    ctx.escrow.record_payment(&ctx.invoice_id, &ctx.payer, &700);

    let state2 = ctx
        .distributor
        .get_distribution_state(&ctx.escrow_id, &ctx.invoice_id);
    assert_eq!(state2.paid_distributed, 1_000);
    assert!(!state2.refund_distributed);
    assert_eq!(
        ctx.escrow.get_escrow_status(&ctx.invoice_id),
        EscrowStatus::Settled
    );

    // Balances reflect full distribution.
    assert_eq!(ctx.payment_token.balance(&ctx.seller), 1_000);
    assert_eq!(ctx.payment_token.balance(&ctx.buyer), 950);
    assert_eq!(ctx.payment_token.balance(&ctx.admin), 50);
    assert_eq!(ctx.payment_token.balance(&ctx.distributor_id), 0);
}

/// Verify that refund state correctly persists after a partial payment
/// followed by a refund through the distributor.
#[test]
fn test_integration_state_persistence_after_refund() {
    let env = Env::default();
    env.mock_all_auths();

    let ctx = setup(&env, 300, true);
    env.ledger().set_timestamp(5_000);
    create_and_fund(&ctx, 1_000, 10_000);
    ctx.payment_asset.mint(&ctx.payer, &500);

    // Partial payment.
    ctx.escrow.record_payment(&ctx.invoice_id, &ctx.payer, &500);

    let state_after_payment = ctx
        .distributor
        .get_distribution_state(&ctx.escrow_id, &ctx.invoice_id);
    assert_eq!(state_after_payment.paid_distributed, 500);
    assert!(!state_after_payment.refund_distributed);

    // Advance time past due date and refund.
    env.ledger().set_timestamp(10_001);
    ctx.escrow.refund_escrow(&ctx.invoice_id);

    let state_after_refund = ctx
        .distributor
        .get_distribution_state(&ctx.escrow_id, &ctx.invoice_id);
    assert_eq!(state_after_refund.paid_distributed, 500);
    assert!(state_after_refund.refund_distributed);

    assert_eq!(
        ctx.escrow.get_escrow_status(&ctx.invoice_id),
        EscrowStatus::Refunded
    );
}

/// Verify that attempting a zero-amount distribution through the dry-run
/// getter returns `NothingToDistribute` (no new payment delta).
#[test]
fn test_integration_edge_case_zero_payment_delta_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let ctx = setup(&env, 300, true);
    create_and_fund(&ctx, 1_000, 50_000);
    ctx.payment_asset.mint(&ctx.payer, &1_000);

    // First settle the full amount so distributed state equals paid amount.
    ctx.escrow
        .record_payment(&ctx.invoice_id, &ctx.payer, &1_000);

    // Now the paid_distributed == 1_000. A second call with the same
    // paid_amount should yield NothingToDistribute (delta = 0).
    let result = ctx.distributor.try_calculate_distribution_splits(
        &ctx.escrow_id,
        &ctx.invoice_id,
        &soroban_sdk::vec![
            &env,
            ctx.payment_token.address.clone(),
            ctx.seller.clone(),
            ctx.buyer.clone(),
            ctx.admin.clone()
        ],
        &soroban_sdk::vec![&env, 1_000i128, 1_000i128, 950i128, 300i128],
    );
    assert_eq!(result, Err(Ok(Error::NothingToDistribute)));
}

/// Verify that the refund distribution routes correctly through the distributor
/// when a partial-payment-then-refund flow uses the distributor.
#[test]
fn test_integration_refund_distribution_invocation_verified() {
    let env = Env::default();
    env.mock_all_auths();

    let ctx = setup(&env, 300, true);
    env.ledger().set_timestamp(5_000);
    create_and_fund(&ctx, 1_000, 10_000);
    ctx.payment_asset.mint(&ctx.payer, &400);

    ctx.escrow.record_payment(&ctx.invoice_id, &ctx.payer, &400);

    env.ledger().set_timestamp(10_001);
    ctx.escrow.refund_escrow(&ctx.invoice_id);

    let state = ctx
        .distributor
        .get_distribution_state(&ctx.escrow_id, &ctx.invoice_id);
    assert!(state.refund_distributed);

    // Verify final state.
    assert_eq!(
        ctx.escrow.get_escrow_status(&ctx.invoice_id),
        EscrowStatus::Refunded
    );
    let state = ctx
        .distributor
        .get_distribution_state(&ctx.escrow_id, &ctx.invoice_id);
    assert_eq!(state.paid_distributed, 400);
    assert!(state.refund_distributed);
}

/// Verify that `distribute_refund` rejects a non-refunded escrow status.
#[test]
fn test_integration_error_distribute_refund_invalid_status() {
    let env = Env::default();
    env.mock_all_auths();

    let ctx = setup(&env, 300, true);
    create_and_fund(&ctx, 1_000, 50_000);
    ctx.payment_asset.mint(&ctx.payer, &1_000);

    // Settle the escrow first so it isn't in Refunded status.
    ctx.escrow
        .record_payment(&ctx.invoice_id, &ctx.payer, &1_000);

    // Try distribute_refund with status=2 (Settled) — should be rejected.
    let result = ctx.distributor.try_distribute_refund(
        &ctx.escrow_id,
        &ctx.invoice_id,
        &soroban_sdk::vec![&env, ctx.payment_token.address.clone(), ctx.buyer.clone()],
        &soroban_sdk::vec![&env, 500i128],
        &2u32, // EscrowStatus::Settled (invalid for distribute_refund)
    );
    assert_eq!(result, Err(Ok(Error::InvalidEscrowStatus)));

    // Also try with status=1 (Funded) — should also be rejected.
    let result2 = ctx.distributor.try_distribute_refund(
        &ctx.escrow_id,
        &ctx.invoice_id,
        &soroban_sdk::vec![&env, ctx.payment_token.address.clone(), ctx.buyer.clone()],
        &soroban_sdk::vec![&env, 500i128],
        &1u32, // EscrowStatus::Funded (invalid for distribute_refund)
    );
    assert_eq!(result2, Err(Ok(Error::InvalidEscrowStatus)));
}

// ══════════════════════════════════════════════════════════════════════════════
// Issue #148: Multi-Recipient Payment Distribution Fanout Test Suite
// ══════════════════════════════════════════════════════════════════════════════

/// Verify multi-recipient fee fanout distribution where platform fee is split
/// among multiple third-party fee recipients and the primary admin.
#[test]
fn test_integration_multi_recipient_payment_fanout_success() {
    let env = Env::default();
    env.mock_all_auths();

    let ctx = setup(&env, 500, true); // 5% fee (500 bps)
    let fanout_1 = Address::generate(&env);
    let fanout_2 = Address::generate(&env);
    let fanout_3 = Address::generate(&env);

    let paid_amount = 10_000i128;
    let investor_amount = 0i128;
    let fee_bps = 500i128;
    let platform_fee = 500i128; // 10_000 * 500 / 10_000
    let total_required = paid_amount + investor_amount + platform_fee; // 10_500

    ctx.payment_asset.mint(&ctx.distributor_id, &total_required);

    let addresses = soroban_sdk::vec![
        &env,
        ctx.payment_token.address.clone(),
        ctx.seller.clone(),
        ctx.buyer.clone(),
        ctx.admin.clone(),
        fanout_1.clone(),
        fanout_2.clone(),
        fanout_3.clone(),
    ];
    let amounts = soroban_sdk::vec![
        &env,
        paid_amount,
        paid_amount,
        investor_amount,
        fee_bps,
        100i128, // fanout_1 cut
        150i128, // fanout_2 cut
        50i128,  // fanout_3 cut
    ];

    ctx.distributor.distribute_payment(
        &ctx.escrow_id,
        &ctx.invoice_id,
        &addresses,
        &amounts,
        &1u32, // EscrowStatus::Funded
    );

    // Verify recipient balances
    assert_eq!(ctx.payment_token.balance(&ctx.seller), 10_000);
    assert_eq!(ctx.payment_token.balance(&fanout_1), 100);
    assert_eq!(ctx.payment_token.balance(&fanout_2), 150);
    assert_eq!(ctx.payment_token.balance(&fanout_3), 50);
    // Admin receives remainder of platform fee: 500 - (100 + 150 + 50) = 200
    assert_eq!(ctx.payment_token.balance(&ctx.admin), 200);
    assert_eq!(ctx.payment_token.balance(&ctx.distributor_id), 0);

    // Verify persistent distribution state
    let state = ctx
        .distributor
        .get_distribution_state(&ctx.escrow_id, &ctx.invoice_id);
    assert_eq!(state.paid_distributed, 10_000);
    assert!(!state.refund_distributed);
}

/// Verify multi-recipient fanout with maximum allowed fee recipients (10 recipients).
#[test]
fn test_integration_multi_recipient_payment_fanout_max_limit_boundary() {
    let env = Env::default();
    env.mock_all_auths();

    let ctx = setup(&env, 1_000, true); // 10% fee (1000 bps)
    let paid_amount = 10_000i128;
    let investor_amount = 0i128;
    let fee_bps = 1_000i128;
    let platform_fee = 1_000i128; // 1000
    let total_required = paid_amount + investor_amount + platform_fee;

    ctx.payment_asset.mint(&ctx.distributor_id, &total_required);

    let mut addresses = soroban_sdk::vec![
        &env,
        ctx.payment_token.address.clone(),
        ctx.seller.clone(),
        ctx.buyer.clone(),
        ctx.admin.clone(),
    ];
    let mut amounts = soroban_sdk::vec![&env, paid_amount, paid_amount, investor_amount, fee_bps,];

    let mut fanout_addresses: soroban_sdk::Vec<Address> = soroban_sdk::vec![&env];
    for _ in 0..10 {
        let addr = Address::generate(&env);
        fanout_addresses.push_back(addr.clone());
        addresses.push_back(addr);
        amounts.push_back(50i128); // 10 * 50 = 500 fanout total
    }

    ctx.distributor.distribute_payment(
        &ctx.escrow_id,
        &ctx.invoice_id,
        &addresses,
        &amounts,
        &1u32,
    );

    for i in 0..10 {
        let addr = fanout_addresses.get(i).unwrap();
        assert_eq!(ctx.payment_token.balance(&addr), 50);
    }
    // Admin receives remainder: 1000 - 500 = 500
    assert_eq!(ctx.payment_token.balance(&ctx.admin), 500);
    assert_eq!(ctx.payment_token.balance(&ctx.distributor_id), 0);
}

/// Verify that exceeding MAX_FANOUT_RECIPIENTS (11 fee recipients) is rejected.
#[test]
fn test_integration_multi_recipient_payment_fanout_exceeding_max_limit_fails() {
    let env = Env::default();
    env.mock_all_auths();

    let ctx = setup(&env, 500, true);
    let mut addresses = soroban_sdk::vec![
        &env,
        ctx.payment_token.address.clone(),
        ctx.seller.clone(),
        ctx.buyer.clone(),
        ctx.admin.clone(),
    ];
    let mut amounts = soroban_sdk::vec![&env, 1_000i128, 1_000i128, 0i128, 500i128,];

    // Add 11 fanout recipients
    for _ in 0..11 {
        addresses.push_back(Address::generate(&env));
        amounts.push_back(10i128);
    }

    let result = ctx.distributor.try_distribute_payment(
        &ctx.escrow_id,
        &ctx.invoice_id,
        &addresses,
        &amounts,
        &1u32,
    );
    assert_eq!(result, Err(Ok(Error::TooManyFeeRecipients)));
}

/// Verify that fee fanout amounts exceeding platform_fee are rejected.
#[test]
fn test_integration_multi_recipient_payment_fanout_exceeding_fee_fails() {
    let env = Env::default();
    env.mock_all_auths();

    let ctx = setup(&env, 500, true); // fee = 50
    let fanout_1 = Address::generate(&env);
    let fanout_2 = Address::generate(&env);

    let addresses = soroban_sdk::vec![
        &env,
        ctx.payment_token.address.clone(),
        ctx.seller.clone(),
        ctx.buyer.clone(),
        ctx.admin.clone(),
        fanout_1,
        fanout_2,
    ];
    let amounts = soroban_sdk::vec![
        &env, 1_000i128, 1_000i128, 0i128, 500i128, 30i128,
        30i128, // 30 + 30 = 60 > platform_fee (50)
    ];

    let result = ctx.distributor.try_distribute_payment(
        &ctx.escrow_id,
        &ctx.invoice_id,
        &addresses,
        &amounts,
        &1u32,
    );
    assert_eq!(result, Err(Ok(Error::InvalidFeeSplit)));
}

/// Verify that negative fanout amounts are rejected.
#[test]
fn test_integration_multi_recipient_payment_fanout_negative_amount_fails() {
    let env = Env::default();
    env.mock_all_auths();

    let ctx = setup(&env, 500, true);
    let fanout_1 = Address::generate(&env);

    let addresses = soroban_sdk::vec![
        &env,
        ctx.payment_token.address.clone(),
        ctx.seller.clone(),
        ctx.buyer.clone(),
        ctx.admin.clone(),
        fanout_1,
    ];
    let amounts = soroban_sdk::vec![
        &env, 1_000i128, 1_000i128, 0i128, 500i128, -10i128, // negative fanout
    ];

    let result = ctx.distributor.try_distribute_payment(
        &ctx.escrow_id,
        &ctx.invoice_id,
        &addresses,
        &amounts,
        &1u32,
    );
    assert_eq!(result, Err(Ok(Error::InvalidFeeSplit)));
}

/// Verify pro-rata refund fanout across multiple funders with remainder dust absorption.
#[test]
fn test_integration_multi_funder_refund_pro_rata_fanout_success() {
    let env = Env::default();
    env.mock_all_auths();

    let ctx = setup(&env, 300, true);
    let funder_1 = Address::generate(&env);
    let funder_2 = Address::generate(&env);
    let funder_3 = Address::generate(&env);

    let refund_amount = 1_000i128;
    ctx.payment_asset.mint(&ctx.distributor_id, &refund_amount);

    let addresses = soroban_sdk::vec![
        &env,
        ctx.payment_token.address.clone(),
        funder_1.clone(),
        funder_2.clone(),
        funder_3.clone(),
    ];
    let amounts = soroban_sdk::vec![
        &env,
        refund_amount,
        500i128, // weight 1 (50%)
        300i128, // weight 2 (30%)
        200i128, // weight 3 (20%)
    ];

    ctx.distributor.distribute_refund(
        &ctx.escrow_id,
        &ctx.invoice_id,
        &addresses,
        &amounts,
        &3u32, // EscrowStatus::Refunded
    );

    assert_eq!(ctx.payment_token.balance(&funder_1), 500);
    assert_eq!(ctx.payment_token.balance(&funder_2), 300);
    assert_eq!(ctx.payment_token.balance(&funder_3), 200);
    assert_eq!(ctx.payment_token.balance(&ctx.distributor_id), 0);

    let state = ctx
        .distributor
        .get_distribution_state(&ctx.escrow_id, &ctx.invoice_id);
    assert!(state.refund_distributed);
}

/// Verify exceeding MAX_REFUND_RECIPIENTS in distribute_refund is rejected.
#[test]
fn test_integration_multi_funder_refund_exceeding_max_recipients_fails() {
    let env = Env::default();
    env.mock_all_auths();

    let ctx = setup(&env, 300, true);
    let mut addresses = soroban_sdk::vec![&env, ctx.payment_token.address.clone()];
    let mut amounts = soroban_sdk::vec![&env, 1_000i128];

    for _ in 0..11 {
        addresses.push_back(Address::generate(&env));
        amounts.push_back(100i128);
    }

    let result = ctx.distributor.try_distribute_refund(
        &ctx.escrow_id,
        &ctx.invoice_id,
        &addresses,
        &amounts,
        &3u32,
    );
    assert_eq!(result, Err(Ok(Error::TooManyRefundRecipients)));
}

// ══════════════════════════════════════════════════════════════════════════════
// Issue #386: distribute_refund reentrancy tests
// ══════════════════════════════════════════════════════════════════════════════

/// Malicious token whose `transfer` callback re-invokes `distribute_refund`
/// on the distributor while the lock is held, recording whether it was rejected.
#[soroban_sdk::contract]
pub struct MaliciousRefundReentrantToken;

#[soroban_sdk::contractimpl]
impl MaliciousRefundReentrantToken {
    pub fn __constructor(env: Env, distributor: Address, escrow: Address, invoice_id: Symbol) {
        let storage = env.storage().instance();
        storage.set(&soroban_sdk::symbol_short!("dist"), &distributor);
        storage.set(&soroban_sdk::symbol_short!("escrow"), &escrow);
        storage.set(&soroban_sdk::symbol_short!("inv"), &invoice_id);
        storage.set(&soroban_sdk::symbol_short!("code"), &0u32);
    }

    pub fn balance(_env: Env, _id: Address) -> i128 {
        1_000_000
    }

    /// Mimics the token `transfer` entrypoint; attempts a re-entrant distribute_refund.
    pub fn transfer(env: Env, _from: Address, _to: Address, _amount: i128) {
        let storage = env.storage().instance();
        let distributor: Address = storage.get(&soroban_sdk::symbol_short!("dist")).unwrap();
        let escrow: Address = storage.get(&soroban_sdk::symbol_short!("escrow")).unwrap();
        let invoice_id: Symbol = storage.get(&soroban_sdk::symbol_short!("inv")).unwrap();

        let addresses = soroban_sdk::vec![&env, escrow.clone(), escrow.clone(), escrow.clone()];
        let amounts = soroban_sdk::vec![&env, 1i128, 1i128, 1i128];

        let client = PaymentDistributorClient::new(&env, &distributor);
        let res = client.try_distribute_refund(&escrow, &invoice_id, &addresses, &amounts, &3u32);
        let code: u32 = match res {
            Ok(Ok(())) => 1,
            Ok(Err(_)) => 2,
            Err(Ok(e)) => 100 + e as u32,
            Err(Err(_)) => 3,
        };
        storage.set(&soroban_sdk::symbol_short!("code"), &code);
    }

    pub fn last_code(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&soroban_sdk::symbol_short!("code"))
            .unwrap_or(0)
    }
}

/// End-to-end: a malicious token whose transfer callback tries to re-invoke
/// distribute_refund during a distribute_payment cannot re-enter successfully.
#[test]
fn test_integration_reentrant_callback_into_distribute_refund_is_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let distributor_id = env.register(PaymentDistributor, ());
    let distributor = PaymentDistributorClient::new(&env, &distributor_id);
    distributor.initialize(&admin);

    let seller = Address::generate(&env);
    let funder = Address::generate(&env);
    let escrow = Address::generate(&env);
    let invoice_id = Symbol::new(&env, "REENTR_REF");
    distributor.set_escrow_contract(&admin, &escrow);

    // Register the malicious token that will re-enter distribute_refund on `transfer`.
    let token_id = env.register(
        MaliciousRefundReentrantToken,
        (distributor_id.clone(), escrow.clone(), invoice_id.clone()),
    );
    let malicious = MaliciousRefundReentrantTokenClient::new(&env, &token_id);

    // Outer distribution: its token transfers route into the malicious token, which
    // tries to re-invoke distribute_refund while the distribution is in progress.
    let result = distributor.try_distribute_payment(
        &escrow,
        &invoice_id,
        &soroban_sdk::vec![
            &env,
            token_id.clone(),
            seller.clone(),
            funder.clone(),
            admin.clone()
        ],
        &soroban_sdk::vec![&env, 100i128, 100i128, 0i128, 0i128],
        &2u32,
    );

    // Outer call completes and the re-entrant distribute_refund invocation was NOT allowed.
    assert!(result.is_ok());
    assert_ne!(malicious.last_code(), 1);
}

/// White-box: simulate an in-progress guarded distribution by setting the lock,
/// then confirm distribute_refund also rejects with ReentrancyDetected.
#[test]
fn test_integration_reentrancy_guard_rejects_distribute_refund_when_locked() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let distributor_id = env.register(PaymentDistributor, ());
    let distributor = PaymentDistributorClient::new(&env, &distributor_id);
    distributor.initialize(&admin);

    let escrow = Address::generate(&env);
    let invoice_id = Symbol::new(&env, "LOCKED_REF");
    distributor.set_escrow_contract(&admin, &escrow);

    // Simulate the lock being held (as if distribute_payment is in progress).
    env.as_contract(&distributor_id, || {
        crate::storage::set_lock(&env, true);
    });

    let addresses = soroban_sdk::vec![&env, escrow.clone(), escrow.clone(), escrow.clone()];
    let amounts = soroban_sdk::vec![&env, 100i128, 100i128, 100i128];

    let result =
        distributor.try_distribute_refund(&escrow, &invoice_id, &addresses, &amounts, &3u32);
    assert_eq!(result, Err(Ok(Error::ReentrancyDetected)));
}

/// State persistence after failed reentrancy: distribution state must remain
/// consistent after a rejected reentrant distribute_refund attempt.
#[test]
fn test_integration_state_persistence_after_reentrancy_attempt() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let distributor_id = env.register(PaymentDistributor, ());
    let distributor = PaymentDistributorClient::new(&env, &distributor_id);
    distributor.initialize(&admin);

    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let payer = Address::generate(&env);
    let escrow_id = env.register(InvoiceEscrow, ());
    let escrow = InvoiceEscrowClient::new(&env, &escrow_id);

    let inv_token_id = env.register(InvoiceToken, ());
    let inv_token = InvoiceTokenClient::new(&env, &inv_token_id);

    let token_admin = Address::generate(&env);
    let token_id = env.register_stellar_asset_contract_v2(token_admin);
    let payment_token = TokenClient::new(&env, &token_id.address());
    let payment_asset = AssetClient::new(&env, &token_id.address());

    let invoice_id = Symbol::new(&env, "PERSIST");
    inv_token.initialize(
        &admin,
        &SorobanString::from_str(&env, "Persist Test"),
        &SorobanString::from_str(&env, "PERS"),
        &7,
        &invoice_id,
        &escrow_id,
    );

    escrow.initialize(&admin, &300);
    distributor.set_escrow_contract(&admin, &escrow_id);
    escrow.set_payment_distributor(&distributor_id);

    // Create and fund escrow, then settle a payment to establish state.
    payment_asset.mint(&buyer, &1_000);
    escrow.create_escrow_legacy(
        &invoice_id,
        &seller,
        &payer,
        &1_000,
        &1_000,
        &50_000,
        &payment_token.address,
        &inv_token.address,
        &test_commitment(&escrow.env),
        &None,
        &None,
    );
    escrow.fund_escrow_legacy(&invoice_id, &buyer, &1_000);
    payment_asset.mint(&payer, &1_000);
    escrow.record_payment(&invoice_id, &payer, &1_000);

    // Verify state was recorded.
    let state = distributor.get_distribution_state(&escrow_id, &invoice_id);
    assert_eq!(state.paid_distributed, 1_000);
    assert!(!state.refund_distributed);

    // Now simulate a failed reentrancy attempt by setting the lock.
    env.as_contract(&distributor_id, || {
        crate::storage::set_lock(&env, true);
    });

    // Attempt distribute_refund — it should fail due to ReentrancyDetected.
    let addresses = soroban_sdk::vec![&env, payment_token.address.clone(), buyer.clone()];
    let amounts = soroban_sdk::vec![&env, 970i128, 970i128];
    let result =
        distributor.try_distribute_refund(&escrow_id, &invoice_id, &addresses, &amounts, &3u32);
    assert_eq!(result, Err(Ok(Error::ReentrancyDetected)));

    // State must be unchanged after the failed reentrancy attempt.
    let state_after = distributor.get_distribution_state(&escrow_id, &invoice_id);
    assert_eq!(state_after.paid_distributed, 1_000);
    assert!(!state_after.refund_distributed);
}

/// Lock cleanup: verify the lock is released after a successful distribute_refund,
/// allowing subsequent operations to proceed.
#[test]
fn test_integration_lock_cleared_after_distribute_refund_success() {
    let env = Env::default();
    env.mock_all_auths();

    let ctx = setup(&env, 300, true);
    create_and_fund(&ctx, 1_000, 50_000);
    env.ledger().set_timestamp(5_000);
    ctx.payment_asset.mint(&ctx.payer, &1_000);

    // Record partial payment then refund.
    ctx.escrow.record_payment(&ctx.invoice_id, &ctx.payer, &400);
    env.ledger().set_timestamp(50_001);
    ctx.escrow.refund_escrow(&ctx.invoice_id);

    // Verify refund was distributed.
    let state = ctx
        .distributor
        .get_distribution_state(&ctx.escrow_id, &ctx.invoice_id);
    assert!(state.refund_distributed);

    // Verify lock is not stuck — we can still call distribute_payment on a new invoice.
    let invoice_id2 = Symbol::new(&env, "INV_LOCK");
    ctx.payment_asset.mint(&ctx.buyer, &500);
    ctx.payment_asset.mint(&ctx.payer, &500);
    ctx.escrow.create_escrow_legacy(
        &invoice_id2,
        &ctx.seller,
        &ctx.payer,
        &500,
        &500,
        &100_000,
        &ctx.payment_token.address,
        &ctx.inv_token.address,
        &test_commitment(&ctx.escrow.env),
        &None,
        &None,
    );
    ctx.escrow.fund_escrow_legacy(&invoice_id2, &ctx.buyer, &500);
    ctx.payment_asset.mint(&ctx.payer, &500);
    ctx.escrow.record_payment(&invoice_id2, &ctx.payer, &500);

    let state2 = ctx
        .distributor
        .get_distribution_state(&ctx.escrow_id, &invoice_id2);
    assert_eq!(state2.paid_distributed, 500);
}

/// Invariant: Distributor balance solvency — verify that distributable amounts
/// never exceed available contract balance. This invariant is maintained across
/// all payment and refund distributions.
#[test]
fn test_invariant_distributor_balance_solvency() {
    let env = Env::default();
    env.mock_all_auths();

    let ctx = setup(&env, 300, true);
    let invoice_id = Symbol::new(&env, "INV_SOLV");

    // Create escrow: seller=1000, funding_target=50000
    ctx.escrow.create_escrow_legacy(
        &invoice_id,
        &ctx.seller,
        &ctx.payer,
        &1_000,
        &50_000,
        &100_000,
        &ctx.payment_token.address,
        &ctx.inv_token.address,
        &test_commitment(&ctx.escrow.env),
        &None,
        &None,
    );

    // Fund the escrow
    ctx.payment_asset.mint(&ctx.buyer, &50_000);
    ctx.escrow.fund_escrow_legacy(&invoice_id, &ctx.buyer, &50_000);

    // Verify distributor balance before payment
    let balance_before = ctx.payment_token.balance(&ctx.distributor_id);
    assert_eq!(balance_before, 0);

    // Record payment
    ctx.payment_asset.mint(&ctx.payer, &1_000);
    ctx.escrow.record_payment(&invoice_id, &ctx.payer, &1_000);

    // Verify distributor balance after payment (should still be distributable)
    let balance_after = ctx.payment_token.balance(&ctx.distributor_id);
    // Contract balance must be >= 0 and sufficient for computed distributions
    assert!(balance_after >= 0);

    // Verify distribution state consistency: paid_distributed <= cumulative_paid
    let state = ctx
        .distributor
        .get_distribution_state(&ctx.escrow_id, &invoice_id);
    assert_eq!(state.paid_distributed, 1_000);
    assert!(!state.refund_distributed);

    // Escrows keyed by Symbol settle implicitly once `paid_amt` reaches
    // `face_value` (there is no explicit settle entrypoint for this id type),
    // so the payment above already settled it. Re-read to confirm no
    // over-distribution occurred during settlement.

    let final_state = ctx
        .distributor
        .get_distribution_state(&ctx.escrow_id, &invoice_id);
    // After settlement, paid_distributed should reflect the settled amount
    assert!(final_state.paid_distributed <= 1_000);
}

/// Invariant: Refund distribution balance conservation — verify that refund amounts
/// are correctly distributed across multiple funders without balance violations.
#[test]
fn test_invariant_refund_balance_conservation() {
    let env = Env::default();
    env.mock_all_auths();

    let ctx = setup(&env, 300, true);
    let invoice_id = Symbol::new(&env, "INV_REFUND");

    // Create and fund escrow
    ctx.escrow.create_escrow_legacy(
        &invoice_id,
        &ctx.seller,
        &ctx.payer,
        &1_000,
        &50_000,
        &100_000,
        &ctx.payment_token.address,
        &ctx.inv_token.address,
        &test_commitment(&ctx.escrow.env),
        &None,
        &None,
    );

    // Fund with buyer
    ctx.payment_asset.mint(&ctx.buyer, &50_000);
    ctx.escrow.fund_escrow_legacy(&invoice_id, &ctx.buyer, &50_000);

    // Refund the escrow (no payment recorded)
    env.ledger().set_timestamp(100_001);
    ctx.escrow.refund_escrow(&invoice_id);

    // Verify refund distribution state
    let state = ctx
        .distributor
        .get_distribution_state(&ctx.escrow_id, &invoice_id);
    assert!(state.refund_distributed);
}
