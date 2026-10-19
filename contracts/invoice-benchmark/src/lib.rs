#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, Address, BytesN, Env, Symbol,
};

#[derive(Clone)]
#[contracttype]
enum DataKey {
    Admin,
    AuthorisedSettler(Address),
    InvoiceSector(BytesN<32>),
    InvoiceOutcome(BytesN<32>),
    SectorBenchmark(Symbol),
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct InvoiceOutcome {
    pub invoice_id: BytesN<32>,
    pub sector: Symbol,
    pub yield_achieved: i128,
    pub days_to_settle: u32,
    pub is_default: bool,
    pub recorded_at: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[contracttype]
pub struct SectorStats {
    pub total_settled: u32,
    pub total_defaults: u32,
    pub sum_yield: i128,
    pub sum_days_to_settle: u64,
    pub avg_yield: i128,
    pub avg_days_to_settle: u32,
    pub default_rate: u32, // In basis points (0..10_000, 100 bps = 1.00%)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[contracttype]
pub struct SectorBenchmarkInfo {
    pub avg_yield: i128,
    pub avg_days_to_settle: u32,
    pub default_rate: u32,
    pub total_settled: u32,
    pub total_defaults: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct InvoiceBenchmarkComparison {
    pub invoice_id: BytesN<32>,
    pub sector: Symbol,
    pub invoice_yield: i128,
    pub avg_yield: i128,
    pub yield_diff: i128, // invoice_yield - avg_yield
    pub invoice_days_to_settle: u32,
    pub avg_days_to_settle: u32,
    pub days_diff: i32, // invoice_days_to_settle - avg_days_to_settle
    pub sector_default_rate: u32,
}

#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum Error {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    Unauthorized = 3,
    InvoiceNotFound = 4,
    InvoiceAlreadyRecorded = 5,
    InvalidSector = 6,
    ArithmeticError = 7,
}

#[contract]
pub struct InvoiceBenchmark;

#[contractimpl]
impl InvoiceBenchmark {
    /// Initialize the benchmark contract with an admin.
    pub fn initialize(env: Env, admin: Address) -> Result<(), Error> {
        if env.storage().instance().has(&DataKey::Admin) {
            return Err(Error::AlreadyInitialized);
        }
        admin.require_auth();
        env.storage().instance().set(&DataKey::Admin, &admin);
        Ok(())
    }

    /// Admin configures authorized settlement contracts.
    pub fn set_authorised_settler(
        env: Env,
        settler: Address,
        authorised: bool,
    ) -> Result<(), Error> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::NotInitialized)?;
        admin.require_auth();

        env.storage()
            .persistent()
            .set(&DataKey::AuthorisedSettler(settler.clone()), &authorised);

        env.events().publish(
            (Symbol::new(&env, "AuthorisedSettlerUpdated"), settler),
            authorised,
        );

        Ok(())
    }

    /// Check if an address is authorized to record settlement outcomes.
    pub fn is_authorised_settler(env: Env, caller: Address) -> bool {
        let admin: Option<Address> = env.storage().instance().get(&DataKey::Admin);
        if let Some(admin_addr) = admin {
            if admin_addr == caller {
                return true;
            }
        }
        env.storage()
            .persistent()
            .get(&DataKey::AuthorisedSettler(caller))
            .unwrap_or(false)
    }

    /// Associate an invoice with its industry sector prior to settlement.
    pub fn register_invoice(
        env: Env,
        invoice_id: BytesN<32>,
        sector: Symbol,
    ) -> Result<(), Error> {
        env.storage()
            .persistent()
            .set(&DataKey::InvoiceSector(invoice_id.clone()), &sector);

        env.events().publish(
            (Symbol::new(&env, "InvoiceRegistered"), invoice_id),
            sector,
        );

        Ok(())
    }

    /// Alias for register_invoice.
    pub fn set_invoice_sector(
        env: Env,
        invoice_id: BytesN<32>,
        sector: Symbol,
    ) -> Result<(), Error> {
        Self::register_invoice(env, invoice_id, sector)
    }

    /// Record settlement outcome for an invoice. Restricted to authorized settlement contract.
    /// Updates rolling sector averages and emits BenchmarkUpdated event.
    pub fn record_settlement_outcome(
        env: Env,
        caller: Address,
        invoice_id: BytesN<32>,
        yield_achieved: i128,
        days_to_settle: u32,
    ) -> Result<(), Error> {
        caller.require_auth();

        if !Self::is_authorised_settler(env.clone(), caller) {
            return Err(Error::Unauthorized);
        }

        // If not explicitly registered, default to "general"
        let sector: Symbol = env
            .storage()
            .persistent()
            .get(&DataKey::InvoiceSector(invoice_id.clone()))
            .unwrap_or_else(|| Symbol::new(&env, "general"));

        Self::apply_settlement(env, invoice_id, sector, yield_achieved, days_to_settle)
    }

    /// Record settlement outcome directly specifying sector.
    pub fn record_settlement_with_sector(
        env: Env,
        caller: Address,
        invoice_id: BytesN<32>,
        sector: Symbol,
        yield_achieved: i128,
        days_to_settle: u32,
    ) -> Result<(), Error> {
        caller.require_auth();

        if !Self::is_authorised_settler(env.clone(), caller) {
            return Err(Error::Unauthorized);
        }

        env.storage()
            .persistent()
            .set(&DataKey::InvoiceSector(invoice_id.clone()), &sector);

        Self::apply_settlement(env, invoice_id, sector, yield_achieved, days_to_settle)
    }

    /// Record an invoice default. Updates default rate for sector.
    pub fn record_default(
        env: Env,
        caller: Address,
        invoice_id: BytesN<32>,
    ) -> Result<(), Error> {
        caller.require_auth();

        if !Self::is_authorised_settler(env.clone(), caller) {
            return Err(Error::Unauthorized);
        }

        let sector: Symbol = env
            .storage()
            .persistent()
            .get(&DataKey::InvoiceSector(invoice_id.clone()))
            .unwrap_or_else(|| Symbol::new(&env, "general"));

        let mut stats: SectorStats = env
            .storage()
            .persistent()
            .get(&DataKey::SectorBenchmark(sector.clone()))
            .unwrap_or(SectorStats {
                total_settled: 0,
                total_defaults: 0,
                sum_yield: 0,
                sum_days_to_settle: 0,
                avg_yield: 0,
                avg_days_to_settle: 0,
                default_rate: 0,
            });

        stats.total_defaults = stats.total_defaults.saturating_add(1);
        let total_invoices = stats.total_settled.saturating_add(stats.total_defaults);
        if total_invoices > 0 {
            stats.default_rate = ((stats.total_defaults as u64)
                .saturating_mul(10_000)
                .checked_div(total_invoices as u64)
                .unwrap_or(0)) as u32;
        }

        env.storage()
            .persistent()
            .set(&DataKey::SectorBenchmark(sector.clone()), &stats);

        let outcome = InvoiceOutcome {
            invoice_id: invoice_id.clone(),
            sector: sector.clone(),
            yield_achieved: 0,
            days_to_settle: 0,
            is_default: true,
            recorded_at: env.ledger().timestamp(),
        };
        env.storage()
            .persistent()
            .set(&DataKey::InvoiceOutcome(invoice_id), &outcome);

        // Emit BenchmarkUpdated
        env.events().publish(
            (Symbol::new(&env, "BenchmarkUpdated"), sector),
            (stats.avg_yield, stats.avg_days_to_settle, stats.default_rate),
        );

        Ok(())
    }

    fn apply_settlement(
        env: Env,
        invoice_id: BytesN<32>,
        sector: Symbol,
        yield_achieved: i128,
        days_to_settle: u32,
    ) -> Result<(), Error> {
        let mut stats: SectorStats = env
            .storage()
            .persistent()
            .get(&DataKey::SectorBenchmark(sector.clone()))
            .unwrap_or(SectorStats {
                total_settled: 0,
                total_defaults: 0,
                sum_yield: 0,
                sum_days_to_settle: 0,
                avg_yield: 0,
                avg_days_to_settle: 0,
                default_rate: 0,
            });

        stats.total_settled = stats.total_settled.saturating_add(1);
        stats.sum_yield = stats.sum_yield.saturating_add(yield_achieved);
        stats.sum_days_to_settle = stats
            .sum_days_to_settle
            .saturating_add(days_to_settle as u64);

        stats.avg_yield = stats
            .sum_yield
            .checked_div(stats.total_settled as i128)
            .unwrap_or(0);
        stats.avg_days_to_settle = (stats
            .sum_days_to_settle
            .checked_div(stats.total_settled as u64)
            .unwrap_or(0)) as u32;

        let total_invoices = stats.total_settled.saturating_add(stats.total_defaults);
        if total_invoices > 0 {
            stats.default_rate = ((stats.total_defaults as u64)
                .saturating_mul(10_000)
                .checked_div(total_invoices as u64)
                .unwrap_or(0)) as u32;
        }

        env.storage()
            .persistent()
            .set(&DataKey::SectorBenchmark(sector.clone()), &stats);

        let outcome = InvoiceOutcome {
            invoice_id: invoice_id.clone(),
            sector: sector.clone(),
            yield_achieved,
            days_to_settle,
            is_default: false,
            recorded_at: env.ledger().timestamp(),
        };

        env.storage()
            .persistent()
            .set(&DataKey::InvoiceOutcome(invoice_id), &outcome);

        // Emit BenchmarkUpdated event with sector and new averages
        env.events().publish(
            (Symbol::new(&env, "BenchmarkUpdated"), sector),
            (stats.avg_yield, stats.avg_days_to_settle, stats.default_rate),
        );

        Ok(())
    }

    /// Query sector benchmark statistics: avg_yield, avg_days_to_settle, default_rate
    pub fn get_sector_benchmark(env: Env, sector: Symbol) -> SectorBenchmarkInfo {
        let stats: SectorStats = env
            .storage()
            .persistent()
            .get(&DataKey::SectorBenchmark(sector))
            .unwrap_or(SectorStats {
                total_settled: 0,
                total_defaults: 0,
                sum_yield: 0,
                sum_days_to_settle: 0,
                avg_yield: 0,
                avg_days_to_settle: 0,
                default_rate: 0,
            });

        SectorBenchmarkInfo {
            avg_yield: stats.avg_yield,
            avg_days_to_settle: stats.avg_days_to_settle,
            default_rate: stats.default_rate,
            total_settled: stats.total_settled,
            total_defaults: stats.total_defaults,
        }
    }

    /// Compare individual invoice metrics vs its sector peer group averages
    pub fn get_invoice_vs_benchmark(
        env: Env,
        invoice_id: BytesN<32>,
    ) -> Result<InvoiceBenchmarkComparison, Error> {
        let outcome: InvoiceOutcome = env
            .storage()
            .persistent()
            .get(&DataKey::InvoiceOutcome(invoice_id.clone()))
            .ok_or(Error::InvoiceNotFound)?;

        let benchmark = Self::get_sector_benchmark(env.clone(), outcome.sector.clone());

        let yield_diff = outcome.yield_achieved.saturating_sub(benchmark.avg_yield);
        let days_diff = (outcome.days_to_settle as i32)
            .saturating_sub(benchmark.avg_days_to_settle as i32);

        Ok(InvoiceBenchmarkComparison {
            invoice_id,
            sector: outcome.sector,
            invoice_yield: outcome.yield_achieved,
            avg_yield: benchmark.avg_yield,
            yield_diff,
            invoice_days_to_settle: outcome.days_to_settle,
            avg_days_to_settle: benchmark.avg_days_to_settle,
            days_diff,
            sector_default_rate: benchmark.default_rate,
        })
    }

    /// Query raw invoice outcome details
    pub fn get_invoice_outcome(
        env: Env,
        invoice_id: BytesN<32>,
    ) -> Result<InvoiceOutcome, Error> {
        env.storage()
            .persistent()
            .get(&DataKey::InvoiceOutcome(invoice_id))
            .ok_or(Error::InvoiceNotFound)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use soroban_sdk::{testutils::Address as _, Address, BytesN, Env, Symbol};

    #[test]
    fn test_benchmark_recording_and_averages() {
        let env = Env::default();
        env.mock_all_auths();

        let admin = Address::generate(&env);
        let settler = Address::generate(&env);
        let unauth = Address::generate(&env);

        let contract_id = env.register(InvoiceBenchmark, ());
        let client = InvoiceBenchmarkClient::new(&env, &contract_id);
        client.initialize(&admin);

        let tech_sector = Symbol::new(&env, "tech");
        let retail_sector = Symbol::new(&env, "retail");

        let inv1 = BytesN::from_array(&env, &[10u8; 32]);
        let inv2 = BytesN::from_array(&env, &[20u8; 32]);
        let inv3 = BytesN::from_array(&env, &[30u8; 32]);

        client.register_invoice(&inv1, &tech_sector);
        client.register_invoice(&inv2, &tech_sector);
        client.register_invoice(&inv3, &retail_sector);

        // Unauthorized settlement fails
        let unauth_res = client.try_record_settlement_outcome(&unauth, &inv1, &800, &30);
        assert_eq!(unauth_res, Err(Ok(Error::Unauthorized)));

        // Admin authorizes settlement contract
        client.set_authorised_settler(&settler, &true);
        assert_eq!(client.is_authorised_settler(&settler), true);

        // Settlement 1 in tech: yield 800 bps (8%), 30 days
        client.record_settlement_outcome(&settler, &inv1, &800, &30);

        let bench1 = client.get_sector_benchmark(&tech_sector);
        assert_eq!(bench1.avg_yield, 800);
        assert_eq!(bench1.avg_days_to_settle, 30);
        assert_eq!(bench1.default_rate, 0);
        assert_eq!(bench1.total_settled, 1);

        // Settlement 2 in tech: yield 1200 bps (12%), 50 days
        client.record_settlement_outcome(&settler, &inv2, &1200, &50);

        // Rolling average: avg_yield = (800 + 1200) / 2 = 1000
        // avg_days_to_settle = (30 + 50) / 2 = 40
        let bench2 = client.get_sector_benchmark(&tech_sector);
        assert_eq!(bench2.avg_yield, 1000);
        assert_eq!(bench2.avg_days_to_settle, 40);
        assert_eq!(bench2.total_settled, 2);

        // Compare inv1 vs benchmark
        let comp1 = client.get_invoice_vs_benchmark(&inv1);
        assert_eq!(comp1.invoice_yield, 800);
        assert_eq!(comp1.avg_yield, 1000);
        assert_eq!(comp1.yield_diff, -200);
        assert_eq!(comp1.invoice_days_to_settle, 30);
        assert_eq!(comp1.avg_days_to_settle, 40);
        assert_eq!(comp1.days_diff, -10);

        // Compare inv2 vs benchmark
        let comp2 = client.get_invoice_vs_benchmark(&inv2);
        assert_eq!(comp2.invoice_yield, 1200);
        assert_eq!(comp2.avg_yield, 1000);
        assert_eq!(comp2.yield_diff, 200);
        assert_eq!(comp2.invoice_days_to_settle, 50);
        assert_eq!(comp2.avg_days_to_settle, 40);
        assert_eq!(comp2.days_diff, 10);

        // Retail settlement outcome
        client.record_settlement_outcome(&settler, &inv3, &650, &20);
        let retail_bench = client.get_sector_benchmark(&retail_sector);
        assert_eq!(retail_bench.avg_yield, 650);
        assert_eq!(retail_bench.avg_days_to_settle, 20);
        assert_eq!(retail_bench.total_settled, 1);

        // Tech sector default
        let inv_def = BytesN::from_array(&env, &[99u8; 32]);
        client.register_invoice(&inv_def, &tech_sector);
        client.record_default(&settler, &inv_def);

        // Total tech invoices = 2 settled + 1 default = 3
        // Default rate = 1 / 3 = 3333 bps (33.33%)
        let bench_with_default = client.get_sector_benchmark(&tech_sector);
        assert_eq!(bench_with_default.total_defaults, 1);
        assert_eq!(bench_with_default.total_settled, 2);
        assert_eq!(bench_with_default.default_rate, 3333);

        // Test record_settlement_with_sector directly
        let inv4 = BytesN::from_array(&env, &[40u8; 32]);
        client.record_settlement_with_sector(&settler, &inv4, &retail_sector, &750, &26);
        let retail_bench2 = client.get_sector_benchmark(&retail_sector);
        assert_eq!(retail_bench2.total_settled, 2);
        assert_eq!(retail_bench2.avg_yield, 700); // (650 + 750) / 2
        assert_eq!(retail_bench2.avg_days_to_settle, 23); // (20 + 26) / 2

        // Query raw invoice outcome
        let outcome = client.get_invoice_outcome(&inv4);
        assert_eq!(outcome.yield_achieved, 750);
        assert_eq!(outcome.days_to_settle, 26);
        assert_eq!(outcome.is_default, false);

        // Query comparison for non-existent invoice fails
        let fake = BytesN::from_array(&env, &[88u8; 32]);
        assert_eq!(
            client.try_get_invoice_vs_benchmark(&fake),
            Err(Ok(Error::InvoiceNotFound))
        );
    }
}
