use soroban_sdk::contracterror;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    Unauthorized = 3,
    RuleNotFound = 4,
    RuleDisabled = 5,
    InvoiceNotFound = 6,
    InvoiceNotOpen = 7,
    CriteriaMismatch = 8,
    BudgetExceeded = 9,
    InvalidAmount = 10,
}
