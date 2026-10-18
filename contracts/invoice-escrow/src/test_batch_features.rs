//! Comprehensive tests for Issues #479, #480, and #481.
#![cfg(test)]

use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::{Address, BytesN, Env, Symbol};

use crate::insurance_pool::{InsuranceConfig, InsurancePool};
use crate::oracle_adapter::{OracleError, OraclePriceAdapter, PriceData};
use crate::earnest_deposit::{EarnestDeposit, EarnestDepositConfig, EarnestError, EarnestStatus};

// ==============================================================================
// 1. Issue #479: Insurance Pool Tests
// ==============================================================================

#[test]
fn test_insurance_pool_deduction_and_claim() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let pool_address = Address::generate(&env);
    let recipient = Address::generate(&env);
    let invoice_id = BytesN::from_array(&env, &[1u8; 32]);

    // 1. Initialize config with 100 bps (1.00%)
    let config = InsuranceConfig {
        admin: admin.clone(),
        deduction_bps: 100,
        pool_address: pool_address.clone(),
        is_active: true,
    };
    InsurancePool::init_config(&env, &config);

    assert_eq!(InsurancePool::get_balance(&env), 0);

    // 2. Invoice with 100,000 stroops face value
    // Deduction = 100,000 * 100 / 10,000 = 1,000 stroops
    let deducted = InsurancePool::record_invoice_deduction(&env, &invoice_id, 100_000);
    assert_eq!(deducted, 1_000);
    assert_eq!(InsurancePool::get_balance(&env), 1_000);

    // 3. Second invoice with 50,000 face value -> 500 stroops
    let invoice_id_2 = BytesN::from_array(&env, &[2u8; 32]);
    let deducted_2 = InsurancePool::record_invoice_deduction(&env, &invoice_id_2, 50_000);
    assert_eq!(deducted_2, 500);
    assert_eq!(InsurancePool::get_balance(&env), 1_500);

    // 4. Admin claims bad-debt coverage of 800 stroops for invoice 1
    let claimed = InsurancePool::claim_coverage(&env, &admin, &invoice_id, &recipient, 800);
    assert_eq!(claimed, 800);
    assert_eq!(InsurancePool::get_balance(&env), 700);
}

#[test]
#[should_panic(expected = "Insurance deduction exceeds maximum allowed rate")]
fn test_insurance_pool_max_bps_enforcement() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let pool_address = Address::generate(&env);

    // 1500 bps (15%) exceeds 1000 bps maximum
    let config = InsuranceConfig {
        admin,
        deduction_bps: 1_500,
        pool_address,
        is_active: true,
    };
    InsurancePool::init_config(&env, &config);
}

#[test]
#[should_panic(expected = "Claim amount exceeds available insurance pool reserves")]
fn test_insurance_pool_claim_exceeds_reserves() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let pool_address = Address::generate(&env);
    let recipient = Address::generate(&env);
    let invoice_id = BytesN::from_array(&env, &[1u8; 32]);

    let config = InsuranceConfig {
        admin: admin.clone(),
        deduction_bps: 50,
        pool_address,
        is_active: true,
    };
    InsurancePool::init_config(&env, &config);
    InsurancePool::record_invoice_deduction(&env, &invoice_id, 10_000); // 50 stroops

    // Attempting to claim 100 stroops when reserves are 50
    InsurancePool::claim_coverage(&env, &admin, &invoice_id, &recipient, 100);
}

// ==============================================================================
// 2. Issue #480: Multi-Currency Oracle Adapter Tests
// ==============================================================================

#[test]
fn test_oracle_adapter_currency_conversion() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000_000);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    OraclePriceAdapter::init_admin(&env, &admin);

    let eur = Symbol::new(&env, "EUR");

    // 1. Configure EUR oracle with 3600s staleness threshold
    OraclePriceAdapter::set_currency_oracle(&env, &admin, eur.clone(), oracle.clone(), 3_600).unwrap();

    // 2. Set price: 1 EUR = 1.08 USD (price = 108_000_000, 8 decimals)
    OraclePriceAdapter::set_price(&env, &admin, eur.clone(), 108_000_000, 8, 1_000_000).unwrap();

    let price_data = OraclePriceAdapter::get_price(&env, &eur).unwrap();
    assert_eq!(price_data.price, 108_000_000);
    assert_eq!(price_data.decimals, 8);

    // 3. Convert 100,000,000 foreign units (100 EUR) to base USD units
    // Expected: 100 * 1.08 = 108 USD (108_000_000 stroops)
    let base_amt = OraclePriceAdapter::convert_to_base(&env, &eur, 100_000_000).unwrap();
    assert_eq!(base_amt, 108_000_000);

    // 4. Inverse conversion: 108 USD to EUR units
    let foreign_amt = OraclePriceAdapter::convert_from_base(&env, &eur, 108_000_000).unwrap();
    assert_eq!(foreign_amt, 100_000_000);
}

#[test]
fn test_oracle_adapter_stale_price_rejection() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000_000);

    let admin = Address::generate(&env);
    let gbp = Symbol::new(&env, "GBP");
    let oracle = Address::generate(&env);

    OraclePriceAdapter::init_admin(&env, &admin);
    OraclePriceAdapter::set_currency_oracle(&env, &admin, gbp.clone(), oracle, 1_800).unwrap(); // 30 mins
    OraclePriceAdapter::set_price(&env, &admin, gbp.clone(), 125_000_000, 8, 1_000_000).unwrap();

    // Advance time by 2,000 seconds (exceeds 1,800s max staleness)
    env.ledger().set_timestamp(1_002_000);

    let result = OraclePriceAdapter::get_price(&env, &gbp);
    assert_eq!(result, Err(OracleError::PriceStale));

    let conv_result = OraclePriceAdapter::convert_to_base(&env, &gbp, 50_000_000);
    assert_eq!(conv_result, Err(OracleError::PriceStale));
}

// ==============================================================================
// 3. Issue #481: Debtor Commitment Earnest Money Deposit Tests
// ==============================================================================

#[test]
fn test_earnest_deposit_lifecycle_return_on_settlement() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(500_000);

    let admin = Address::generate(&env);
    let debtor = Address::generate(&env);
    let invoice_id = BytesN::from_array(&env, &[7u8; 32]);

    // 1. Configure earnest deposit: 5% (500 bps), min 1,000 stroops
    let config = EarnestDepositConfig {
        admin: admin.clone(),
        deposit_bps: 500,
        min_deposit: 1_000,
        is_required: true,
    };
    EarnestDeposit::init_config(&env, &config).unwrap();

    // Face value 50,000 stroops -> 5% = 2,500 stroops
    let required = EarnestDeposit::calculate_required_deposit(&env, 50_000);
    assert_eq!(required, 2_500);

    // 2. Debtor locks earnest deposit
    let record = EarnestDeposit::lock_deposit(&env, &invoice_id, &debtor, required).unwrap();
    assert_eq!(record.amount, 2_500);
    assert_eq!(record.status, EarnestStatus::Locked);
    assert_eq!(EarnestDeposit::get_total_locked(&env), 2_500);

    // 3. Invoice is settled -> Return deposit to debtor
    env.ledger().set_timestamp(510_000);
    let returned = EarnestDeposit::return_deposit(&env, &invoice_id).unwrap();
    assert_eq!(returned, 2_500);
    assert_eq!(EarnestDeposit::get_total_locked(&env), 0);

    let updated = EarnestDeposit::get_deposit(&env, &invoice_id).unwrap();
    assert_eq!(updated.status, EarnestStatus::Returned);
    assert_eq!(updated.resolved_timestamp, 510_000);
}

#[test]
fn test_earnest_deposit_lifecycle_forfeited_on_default() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(600_000);

    let admin = Address::generate(&env);
    let debtor = Address::generate(&env);
    let pool_beneficiary = Address::generate(&env);
    let invoice_id = BytesN::from_array(&env, &[9u8; 32]);

    let config = EarnestDepositConfig {
        admin,
        deposit_bps: 1_000, // 10%
        min_deposit: 500,
        is_required: true,
    };
    EarnestDeposit::init_config(&env, &config).unwrap();

    // Lock deposit of 3,000 stroops
    EarnestDeposit::lock_deposit(&env, &invoice_id, &debtor, 3_000).unwrap();
    assert_eq!(EarnestDeposit::get_total_locked(&env), 3_000);

    // Invoice defaults and is refunded -> Forfeit deposit to pool beneficiary
    let forfeited = EarnestDeposit::forfeit_deposit(&env, &invoice_id, &pool_beneficiary).unwrap();
    assert_eq!(forfeited, 3_000);
    assert_eq!(EarnestDeposit::get_total_locked(&env), 0);
    assert_eq!(EarnestDeposit::get_total_forfeited(&env), 3_000);

    let updated = EarnestDeposit::get_deposit(&env, &invoice_id).unwrap();
    assert_eq!(updated.status, EarnestStatus::Forfeited);

    // Attempting to return after forfeiture should fail
    let err = EarnestDeposit::return_deposit(&env, &invoice_id);
    assert_eq!(err, Err(EarnestError::InvalidStatus));
}
