#![no_std]

#[cfg(test)]
mod test;

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, Address, BytesN, Env, String, Symbol, Vec,
};

const SECONDS_PER_DAY: u64 = 86_400;
/// Longest extension allowed beyond the original maturity.
const MAX_EXTENSION_DAYS: u64 = 90;
/// Weighted approval required for an extension to pass, in percent.
const APPROVAL_PERCENT: i128 = 51;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    InvoiceAlreadyRegistered = 3,
    InvoiceNotFound = 4,
    InvalidAmount = 5,
    DueDatePassed = 6,
    InvalidMaturity = 7,
    MaxExtensionExceeded = 8,
    ProposalNotFound = 9,
    NotHolder = 10,
    AlreadyVoted = 11,
    AlreadyExecuted = 12,
    ApprovalNotMet = 13,
    Overflow = 14,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Holding {
    pub holder: Address,
    pub fractions: i128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Invoice {
    pub issuer: Address,
    pub original_maturity: u64,
    pub maturity: u64,
    pub total_fractions: i128,
    pub holdings: Vec<Holding>,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Proposal {
    pub invoice_id: BytesN<32>,
    pub new_maturity: u64,
    pub reason: String,
    pub votes_for: i128,
    pub votes_against: i128,
    pub executed: bool,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StorageKey {
    Admin,
    ProposalCount,
    Invoice(BytesN<32>),
    Proposal(u64),
    Voted(u64, Address),
}

#[contract]
pub struct InvoiceExtension;

#[contractimpl]
impl InvoiceExtension {
    pub fn initialize(env: Env, admin: Address) -> Result<(), Error> {
        if env.storage().instance().has(&StorageKey::Admin) {
            return Err(Error::AlreadyInitialized);
        }
        env.storage().instance().set(&StorageKey::Admin, &admin);
        Ok(())
    }

    /// Registers an invoice with its issuer, maturity and fraction holders (the vote weights).
    pub fn register_invoice(
        env: Env,
        invoice_id: BytesN<32>,
        issuer: Address,
        maturity: u64,
        holdings: Vec<Holding>,
    ) -> Result<(), Error> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&StorageKey::Admin)
            .ok_or(Error::NotInitialized)?;
        admin.require_auth();
        let key = StorageKey::Invoice(invoice_id);
        if env.storage().persistent().has(&key) {
            return Err(Error::InvoiceAlreadyRegistered);
        }
        let mut total_fractions: i128 = 0;
        for h in holdings.iter() {
            if h.fractions <= 0 {
                return Err(Error::InvalidAmount);
            }
            total_fractions = total_fractions
                .checked_add(h.fractions)
                .ok_or(Error::Overflow)?;
        }
        if total_fractions == 0 {
            return Err(Error::InvalidAmount);
        }
        env.storage().persistent().set(
            &key,
            &Invoice {
                issuer,
                original_maturity: maturity,
                maturity,
                total_fractions,
                holdings,
            },
        );
        Ok(())
    }

    #[allow(deprecated)]
    pub fn propose_extension(
        env: Env,
        invoice_id: BytesN<32>,
        new_maturity: u64,
        reason: String,
    ) -> Result<u64, Error> {
        let invoice = Self::load_invoice(&env, &invoice_id)?;
        invoice.issuer.require_auth();
        if env.ledger().timestamp() >= invoice.maturity {
            return Err(Error::DueDatePassed);
        }
        if new_maturity <= invoice.maturity {
            return Err(Error::InvalidMaturity);
        }
        if new_maturity > invoice.original_maturity + MAX_EXTENSION_DAYS * SECONDS_PER_DAY {
            return Err(Error::MaxExtensionExceeded);
        }

        let proposal_id: u64 = env
            .storage()
            .instance()
            .get(&StorageKey::ProposalCount)
            .unwrap_or(0)
            + 1;
        env.storage()
            .instance()
            .set(&StorageKey::ProposalCount, &proposal_id);
        env.storage().persistent().set(
            &StorageKey::Proposal(proposal_id),
            &Proposal {
                invoice_id: invoice_id.clone(),
                new_maturity,
                reason,
                votes_for: 0,
                votes_against: 0,
                executed: false,
            },
        );

        let topics = (Symbol::new(&env, "ExtensionProposed"), invoice_id);
        env.events()
            .publish(topics, (proposal_id, invoice.maturity, new_maturity));
        Ok(proposal_id)
    }

    /// Fraction-weighted vote; voting closes once the invoice's current maturity is reached.
    #[allow(deprecated)]
    pub fn vote_extension(
        env: Env,
        proposal_id: u64,
        voter: Address,
        support: bool,
    ) -> Result<(), Error> {
        voter.require_auth();
        let mut proposal = Self::load_proposal(&env, proposal_id)?;
        if proposal.executed {
            return Err(Error::AlreadyExecuted);
        }
        let invoice = Self::load_invoice(&env, &proposal.invoice_id)?;
        if env.ledger().timestamp() >= invoice.maturity {
            return Err(Error::DueDatePassed);
        }
        let voted_key = StorageKey::Voted(proposal_id, voter.clone());
        if env.storage().persistent().has(&voted_key) {
            return Err(Error::AlreadyVoted);
        }
        let weight = invoice
            .holdings
            .iter()
            .find(|h| h.holder == voter)
            .map(|h| h.fractions)
            .ok_or(Error::NotHolder)?;

        if support {
            proposal.votes_for += weight;
        } else {
            proposal.votes_against += weight;
        }
        env.storage().persistent().set(&voted_key, &true);
        env.storage()
            .persistent()
            .set(&StorageKey::Proposal(proposal_id), &proposal);

        let topics = (Symbol::new(&env, "ExtensionVoteCast"), proposal_id);
        env.events().publish(topics, (voter, support, weight));
        Ok(())
    }

    /// Applies the new maturity once at least 51% of all fractions voted in favour.
    #[allow(deprecated)]
    pub fn execute_extension(env: Env, proposal_id: u64) -> Result<(), Error> {
        let mut proposal = Self::load_proposal(&env, proposal_id)?;
        if proposal.executed {
            return Err(Error::AlreadyExecuted);
        }
        let mut invoice = Self::load_invoice(&env, &proposal.invoice_id)?;
        if env.ledger().timestamp() >= invoice.maturity {
            return Err(Error::DueDatePassed);
        }
        if proposal.new_maturity <= invoice.maturity {
            return Err(Error::InvalidMaturity);
        }
        if proposal.votes_for * 100 < invoice.total_fractions * APPROVAL_PERCENT {
            return Err(Error::ApprovalNotMet);
        }

        let old_maturity = invoice.maturity;
        invoice.maturity = proposal.new_maturity;
        proposal.executed = true;
        env.storage()
            .persistent()
            .set(&StorageKey::Invoice(proposal.invoice_id.clone()), &invoice);
        env.storage()
            .persistent()
            .set(&StorageKey::Proposal(proposal_id), &proposal);

        let topics = (Symbol::new(&env, "ExtensionExecuted"), proposal.invoice_id);
        env.events()
            .publish(topics, (proposal_id, old_maturity, invoice.maturity));
        Ok(())
    }

    pub fn get_maturity(env: Env, invoice_id: BytesN<32>) -> Result<u64, Error> {
        Ok(Self::load_invoice(&env, &invoice_id)?.maturity)
    }

    pub fn get_proposal(env: Env, proposal_id: u64) -> Result<Proposal, Error> {
        Self::load_proposal(&env, proposal_id)
    }
}

impl InvoiceExtension {
    fn load_invoice(env: &Env, invoice_id: &BytesN<32>) -> Result<Invoice, Error> {
        env.storage()
            .persistent()
            .get(&StorageKey::Invoice(invoice_id.clone()))
            .ok_or(Error::InvoiceNotFound)
    }

    fn load_proposal(env: &Env, proposal_id: u64) -> Result<Proposal, Error> {
        env.storage()
            .persistent()
            .get(&StorageKey::Proposal(proposal_id))
            .ok_or(Error::ProposalNotFound)
    }
}
