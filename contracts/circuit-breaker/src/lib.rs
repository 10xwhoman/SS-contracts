//! Circuit Breaker Contract (#578)
//!
//! Monitors platform metrics and automatically pauses operations when
//! anomaly thresholds are breached, protecting users during incidents.

#![no_std]

use soroban_sdk::{contract, contractimpl, contracttype, symbol_short, Address, Env, Symbol};

#[derive(Clone, Copy, PartialEq)]
#[contracttype]
pub enum MetricType {
    HourlyVolume,
    SingleInvestment,
    DefaultRate,
}

#[derive(Clone)]
#[contracttype]
pub struct Threshold {
    pub metric_type: MetricType,
    pub max_value: i128,
}

#[derive(Clone)]
#[contracttype]
pub struct MetricRecord {
    pub metric_type: MetricType,
    pub value: i128,
    pub timestamp: u64,
}

#[derive(Clone)]
#[contracttype]
pub enum DataKey {
    Admin,
    Paused,
    Threshold(MetricType),
    HourlyVolume,
    HourlyVolumeStartTime,
}

#[contract]
pub struct CircuitBreakerContract;

#[contractimpl]
impl CircuitBreakerContract {
    /// Initialize the contract with an admin
    pub fn initialize(env: Env, admin: Address) {
        if env.storage().instance().has(&DataKey::Admin) {
            panic!("Already initialized");
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::Paused, &false);
        env.storage().instance().set(&DataKey::HourlyVolume, &0i128);
        env.storage()
            .instance()
            .set(&DataKey::HourlyVolumeStartTime, &env.ledger().timestamp());
    }

    /// Set threshold for a metric type (admin-only)
    pub fn set_threshold(env: Env, metric_type: MetricType, max_value: i128) {
        let admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
        admin.require_auth();

        let threshold = Threshold {
            metric_type,
            max_value,
        };

        env.storage()
            .instance()
            .set(&DataKey::Threshold(metric_type), &threshold);

        env.events().publish(
            (symbol_short!("set_thr"), metric_type),
            max_value,
        );
    }

    /// Record a metric value from platform contracts
    pub fn record_metric(env: Env, metric_type: MetricType, value: i128) {
        // Check if circuit breaker is already tripped
        let is_paused: bool = env.storage().instance().get(&DataKey::Paused).unwrap_or(false);
        if is_paused {
            panic!("CircuitBreakerActive");
        }

        // Get threshold for this metric type
        let threshold_key = DataKey::Threshold(metric_type);
        let threshold: Option<Threshold> = env.storage().instance().get(&threshold_key);

        if let Some(threshold) = threshold {
            // Handle hourly volume specially (accumulative)
            if metric_type == MetricType::HourlyVolume {
                let current_time = env.ledger().timestamp();
                let start_time: u64 = env
                    .storage()
                    .instance()
                    .get(&DataKey::HourlyVolumeStartTime)
                    .unwrap_or(current_time);

                // Reset if more than 1 hour has passed
                if current_time - start_time >= 3600 {
                    env.storage().instance().set(&DataKey::HourlyVolume, &value);
                    env.storage()
                        .instance()
                        .set(&DataKey::HourlyVolumeStartTime, &current_time);
                } else {
                    let current_volume: i128 = env
                        .storage()
                        .instance()
                        .get(&DataKey::HourlyVolume)
                        .unwrap_or(0);
                    let new_volume = current_volume + value;
                    env.storage().instance().set(&DataKey::HourlyVolume, &new_volume);

                    // Check if threshold breached
                    if new_volume > threshold.max_value {
                        Self::trip_circuit_breaker(env.clone(), metric_type, threshold.max_value, new_volume);
                        return;
                    }
                }
            } else {
                // For non-accumulative metrics, check directly
                if value > threshold.max_value {
                    Self::trip_circuit_breaker(env, metric_type, threshold.max_value, value);
                    return;
                }
            }
        }

        // Record the metric
        env.events().publish(
            (symbol_short!("metric"), metric_type),
            value,
        );
    }

    /// Internal function to trip the circuit breaker
    fn trip_circuit_breaker(env: Env, metric_type: MetricType, threshold: i128, observed: i128) {
        env.storage().instance().set(&DataKey::Paused, &true);

        env.events().publish(
            (symbol_short!("tripped"), metric_type),
            (threshold, observed),
        );
    }

    /// Resume operations after investigation (admin-only)
    pub fn resume_operations(env: Env) {
        let admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
        admin.require_auth();

        env.storage().instance().set(&DataKey::Paused, &false);
        
        // Reset hourly volume tracking
        env.storage().instance().set(&DataKey::HourlyVolume, &0i128);
        env.storage()
            .instance()
            .set(&DataKey::HourlyVolumeStartTime, &env.ledger().timestamp());

        env.events().publish(
            (symbol_short!("resumed"),),
            (),
        );
    }

    /// Check if circuit breaker is active
    pub fn is_paused(env: Env) -> bool {
        env.storage().instance().get(&DataKey::Paused).unwrap_or(false)
    }

    /// Validate operation (called by investment/settlement contracts)
    pub fn validate_operation(env: Env) {
        let is_paused: bool = env.storage().instance().get(&DataKey::Paused).unwrap_or(false);
        if is_paused {
            panic!("CircuitBreakerActive");
        }
    }

    /// Get threshold for a metric type
    pub fn get_threshold(env: Env, metric_type: MetricType) -> Option<i128> {
        env.storage()
            .instance()
            .get::<DataKey, Threshold>(&DataKey::Threshold(metric_type))
            .map(|t| t.max_value)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::Env;

    #[test]
    fn test_set_threshold() {
        let env = Env::default();
        let contract_id = env.register_contract(None, CircuitBreakerContract);
        let client = CircuitBreakerContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.initialize(&admin);

        client.set_threshold(&MetricType::SingleInvestment, &1000000);
        
        let threshold = client.get_threshold(&MetricType::SingleInvestment);
        assert_eq!(threshold, Some(1000000));
    }

    #[test]
    #[should_panic(expected = "CircuitBreakerActive")]
    fn test_circuit_breaker_trips() {
        let env = Env::default();
        let contract_id = env.register_contract(None, CircuitBreakerContract);
        let client = CircuitBreakerContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.initialize(&admin);

        // Set threshold
        client.set_threshold(&MetricType::SingleInvestment, &1000000);

        // Record metric that exceeds threshold
        client.record_metric(&MetricType::SingleInvestment, &2000000);

        // Circuit breaker should be tripped
        assert!(client.is_paused());

        // This should panic
        client.validate_operation();
    }

    #[test]
    fn test_resume_operations() {
        let env = Env::default();
        let contract_id = env.register_contract(None, CircuitBreakerContract);
        let client = CircuitBreakerContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.initialize(&admin);

        client.set_threshold(&MetricType::SingleInvestment, &1000000);
        client.record_metric(&MetricType::SingleInvestment, &2000000);

        assert!(client.is_paused());

        // Resume
        client.resume_operations();
        assert!(!client.is_paused());
    }
}
