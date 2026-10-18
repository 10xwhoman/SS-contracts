//! Soroban CPU and memory resource consumption benchmarks for escrow operations (#442).
//!
//! These tests measure the resource cost of core escrow operations so we can:
//! - Track regressions when changing contract logic
//! - Ensure operations stay within Soroban's budget limits
//! - Guide batching limits and pagination thresholds
//!
//! Each benchmark runs the operation inside a measured closure and reports
//! the CPU instructions consumed and memory bytes allocated.

use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, BytesN, Env, Symbol};

use super::{Config, EscrowStatus, InvoiceCategory};
use crate::{storage, InvoiceEscrow};

/// Build a minimal initialized escrow environment and return (env, admin, token).
fn setup_bench_env() -> (Env, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let token = Address::generate(&env);

    // Initialize the escrow contract.
    env.as_contract(&admin, || {
        let config = Config {
            admin: admin.clone(),
            fee_bps: 300,
            payment_distributor: None,
            paused: false,
            whitelist_enabled: false,
            min_investment: 0,
            grace_period_seconds: 0,
            dispute_timeout_secs: 604_800,
            accreditation_callback: None,
            penalty_interest_bps: 0,
        };
        storage::set_config(&env, &config);
    });

    (env, admin, token)
}

/// Helper to create a commitment hash from a string.
fn bench_commitment(env: &Env, data: &str) -> BytesN<32> {
    let mut array = [0u8; 32];
    let bytes = data.as_bytes();
    let len = bytes.len().min(32);
    array[..len].copy_from_slice(&bytes[..len]);
    BytesN::from_array(env, &array)
}

// ── Benchmarks ────────────────────────────────────────────────────────────────

#[test]
fn bench_initialize_contract() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);

    let result = env.budget(|budget| {
        let contract_addr = Address::generate(&env);
        env.register_contract(&contract_addr, InvoiceEscrow);
        env.as_contract(&admin, || {
            InvoiceEscrow::initialize(env.clone(), admin.clone(), 300).unwrap();
        });
        let cpu = budget.cpu_instructions();
        let mem = budget.mem_bytes();
        (cpu, mem)
    });

    println!(
        "[bench] initialize_contract: cpu={}, mem={}",
        result.0, result.1
    );
    // Sanity: initialization should complete without exceeding budget.
    assert!(result.0 > 0);
}

#[test]
fn bench_set_platform_fee() {
    let (env, admin, _token) = setup_bench_env();

    let result = env.budget(|budget| {
        env.as_contract(&admin, || {
            InvoiceEscrow::set_settlement_fee(env.clone(), admin.clone(), 500).unwrap();
        });
        let cpu = budget.cpu_instructions();
        let mem = budget.mem_bytes();
        (cpu, mem)
    });

    println!(
        "[bench] set_platform_fee: cpu={}, mem={}",
        result.0, result.1
    );
    assert!(result.0 > 0);
}

#[test]
fn bench_set_grace_period() {
    let (env, admin, _token) = setup_bench_env();

    let result = env.budget(|budget| {
        env.as_contract(&admin, || {
            InvoiceEscrow::set_grace_period(env.clone(), admin.clone(), 86_400).unwrap();
        });
        let cpu = budget.cpu_instructions();
        let mem = budget.mem_bytes();
        (cpu, mem)
    });

    println!(
        "[bench] set_grace_period: cpu={}, mem={}",
        result.0, result.1
    );
    assert!(result.0 > 0);
}

#[test]
fn bench_set_max_investors() {
    let (env, admin, _token) = setup_bench_env();

    let result = env.budget(|budget| {
        env.as_contract(&admin, || {
            InvoiceEscrow::set_max_investors(env.clone(), 100).unwrap();
        });
        let cpu = budget.cpu_instructions();
        let mem = budget.mem_bytes();
        (cpu, mem)
    });

    println!(
        "[bench] set_max_investors: cpu={}, mem={}",
        result.0, result.1
    );
    assert!(result.0 > 0);
}

#[test]
fn bench_propose_param_change() {
    let (env, admin, _token) = setup_bench_env();

    let result = env.budget(|budget| {
        env.as_contract(&admin, || {
            InvoiceEscrow::propose_param_change(
                env.clone(),
                admin.clone(),
                crate::types::ParamType::FeeBps,
                500,
                None,
            )
            .unwrap();
        });
        let cpu = budget.cpu_instructions();
        let mem = budget.mem_bytes();
        (cpu, mem)
    });

    println!(
        "[bench] propose_param_change: cpu={}, mem={}",
        result.0, result.1
    );
    assert!(result.0 > 0);
}

#[test]
fn bench_get_config() {
    let (env, admin, _token) = setup_bench_env();

    let result = env.budget(|budget| {
        env.as_contract(&admin, || {
            let _config = storage::get_config(&env).unwrap();
        });
        let cpu = budget.cpu_instructions();
        let mem = budget.mem_bytes();
        (cpu, mem)
    });

    println!("[bench] get_config: cpu={}, mem={}", result.0, result.1);
    assert!(result.0 > 0);
}

#[test]
fn bench_set_category_fee() {
    let (env, admin, _token) = setup_bench_env();

    let result = env.budget(|budget| {
        env.as_contract(&admin, || {
            InvoiceEscrow::set_category_fee(
                env.clone(),
                admin.clone(),
                InvoiceCategory::Factoring,
                450,
            )
            .unwrap();
        });
        let cpu = budget.cpu_instructions();
        let mem = budget.mem_bytes();
        (cpu, mem)
    });

    println!(
        "[bench] set_category_fee: cpu={}, mem={}",
        result.0, result.1
    );
    assert!(result.0 > 0);
}

#[test]
fn bench_toggle_whitelist() {
    let (env, admin, _token) = setup_bench_env();

    let result = env.budget(|budget| {
        env.as_contract(&admin, || {
            InvoiceEscrow::set_whitelist_enabled(env.clone(), admin.clone(), true).unwrap();
        });
        let cpu = budget.cpu_instructions();
        let mem = budget.mem_bytes();
        (cpu, mem)
    });

    println!(
        "[bench] toggle_whitelist: cpu={}, mem={}",
        result.0, result.1
    );
    assert!(result.0 > 0);
}
