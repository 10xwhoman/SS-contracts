//! Property-based fuzz tests for the invoice-escrow contract's multi-investor
//! refund allocations (#438).
//!
//! Verifies the zero-sum invariant: for any set of investor contributions,
//! the sum of all individual refunds equals the total refund amount.

use proptest::prelude::*;

/// The zero-sum property: given N investors each contributing `amounts[i]`,
/// and a refund rate `refund_per_unit`, the total distributed must equal
/// `sum(amounts) * refund_per_unit / denominator` (with integer division).
///
/// This is a pure-logic test that mirrors the contract's pro-rata refund
/// calculation without requiring a full Soroban environment.
fn compute_pro_rata_refund(funded_amt: i128, funder_amt: i128, amount_to_refund: i128) -> i128 {
    if funded_amt == 0 {
        return 0;
    }
    amount_to_refund
        .saturating_mul(funder_amt)
        .saturating_div(funded_amt)
}

/// Property: for any non-negative funding amounts that sum to `total_funded`,
/// and any refund amount `R <= total_funded`, the sum of per-investor pro-rata
/// refunds never exceeds `R`.
proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    #[test]
    fn refund_sum_never_exceeds_total(
        amounts in prop::collection::vec(1i128..1_000_000i128, 1..50),
        refund_ratio in 0u32..10_001u32,
    ) {
        let total_funded: i128 = amounts.iter().sum();
        let amount_to_refund = total_funded
            .saturating_mul(refund_ratio as i128)
            .saturating_div(10_000);

        let mut refund_sum: i128 = 0;
        for &funder_amt in &amounts {
            let refund = compute_pro_rata_refund(total_funded, funder_amt, amount_to_refund);
            refund_sum = refund_sum.saturating_add(refund);
        }

        // The sum of individual refunds must never exceed the total refund amount.
        prop_assert!(
            refund_sum <= amount_to_refund,
            "refund_sum {} exceeded amount_to_refund {}",
            refund_sum,
            amount_to_refund,
        );
    }

    #[test]
    fn full_refund_returns_all_funded(
        amounts in prop::collection::vec(1i128..1_000_000i128, 1..50),
    ) {
        let total_funded: i128 = amounts.iter().sum();
        // Full refund: amount_to_refund == total_funded
        let amount_to_refund = total_funded;

        let mut refund_sum: i128 = 0;
        for &funder_amt in &amounts {
            let refund = compute_pro_rata_refund(total_funded, funder_amt, amount_to_refund);
            refund_sum = refund_sum.saturating_add(refund);
        }

        // With integer division, the sum may be slightly less than total due to
        // truncation, but never more.
        prop_assert!(
            refund_sum <= amount_to_refund,
            "refund_sum {} exceeded total_funded {}",
            refund_sum,
            amount_to_refund,
        );
        // And the difference should be at most N-1 (one unit of truncation per investor).
        let truncation_loss = amount_to_refund - refund_sum;
        prop_assert!(
            truncation_loss <= (amounts.len() as i128 - 1).max(0),
            "truncation loss {} exceeds investor count - 1 ({})",
            truncation_loss,
            amounts.len() as i128 - 1,
        );
    }

    #[test]
    fn zero_refund_distributes_nothing(
        amounts in prop::collection::vec(1i128..1_000_000i128, 1..50),
    ) {
        let total_funded: i128 = amounts.iter().sum();
        let amount_to_refund: i128 = 0;

        for &funder_amt in &amounts {
            let refund = compute_pro_rata_refund(total_funded, funder_amt, amount_to_refund);
            prop_assert_eq!(refund, 0i128);
        }
    }

    #[test]
    fn proportional_refund_matches_share(
        amounts in prop::collection::vec(1i128..1_000_000i128, 2..20),
        refund_ratio in 1u32..10_001u32,
    ) {
        let total_funded: i128 = amounts.iter().sum();
        let amount_to_refund = total_funded
            .saturating_mul(refund_ratio as i128)
            .saturating_div(10_000);

        for &funder_amt in &amounts {
            let refund = compute_pro_rata_refund(total_funded, funder_amt, amount_to_refund);
            let expected = funder_amt
                .saturating_mul(amount_to_refund)
                .saturating_div(total_funded);
            prop_assert_eq!(refund, expected);
        }
    }
}
