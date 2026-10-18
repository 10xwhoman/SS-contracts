use soroban_sdk::{contracttype, Address};

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EventType {
    Settlement,
    KycExpiry,
    WatchlistChange,
    AutoInvest,
    MaturityApproaching,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AlertPreference {
    pub event_type: EventType,
    pub enabled: bool,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StorageKey {
    Preferences(Address),
}
