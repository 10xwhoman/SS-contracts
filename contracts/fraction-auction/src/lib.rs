#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, token, Address, BytesN, Env, Symbol, Vec,
};

#[derive(Clone)]
#[contracttype]
enum DataKey {
    Admin,
    AuctionCount,
    Auction(u64),
    Bids(u64),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[contracttype]
#[repr(u32)]
pub enum AuctionStatus {
    Active = 0,
    Settled = 1,
    Cancelled = 2,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct Auction {
    pub id: u64,
    pub seller: Address,
    pub invoice_id: BytesN<32>,
    pub fraction_token: Address,
    pub payment_token: Address,
    pub amount: i128,
    pub reserve_price: i128,
    pub min_bid: i128,
    pub start_time: u64,
    pub end_time: u64,
    pub highest_bidder: Option<Address>,
    pub highest_bid: i128,
    pub status: AuctionStatus,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct Bid {
    pub bidder: Address,
    pub amount: i128,
    pub timestamp: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[contracttype]
pub struct AuctionStatusInfo {
    pub highest_bid: i128,
    pub reserve_price: i128,
    pub reserve_met: bool,
    pub status: AuctionStatus,
}

#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum Error {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    Unauthorized = 3,
    AuctionNotFound = 4,
    AuctionEnded = 5,
    AuctionNotEnded = 6,
    AuctionNotActive = 7,
    BelowReserve = 8,
    BidTooLow = 9,
    ReserveNotMet = 10,
    InvalidAmount = 11,
    InvalidDuration = 12,
    InvalidPrice = 13,
}

#[contract]
pub struct FractionAuction;

#[contractimpl]
impl FractionAuction {
    /// Initialize the contract with an admin.
    pub fn initialize(env: Env, admin: Address) -> Result<(), Error> {
        if env.storage().instance().has(&DataKey::Admin) {
            return Err(Error::AlreadyInitialized);
        }
        admin.require_auth();
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::AuctionCount, &0u64);
        Ok(())
    }

    /// Seller creates a timed auction for invoice fractions.
    /// Locks fraction tokens in this contract.
    pub fn create_auction(
        env: Env,
        seller: Address,
        invoice_id: BytesN<32>,
        fraction_token: Address,
        payment_token: Address,
        amount: i128,
        reserve_price: i128,
        duration_seconds: u64,
        min_bid: Option<i128>,
    ) -> Result<u64, Error> {
        seller.require_auth();

        if amount <= 0 {
            return Err(Error::InvalidAmount);
        }
        if reserve_price <= 0 {
            return Err(Error::InvalidPrice);
        }
        if duration_seconds == 0 {
            return Err(Error::InvalidDuration);
        }

        // Lock fraction tokens into auction contract
        token::Client::new(&env, &fraction_token).transfer(
            &seller,
            &env.current_contract_address(),
            &amount,
        );

        let mut count: u64 = env
            .storage()
            .instance()
            .get(&DataKey::AuctionCount)
            .unwrap_or(0u64);
        count = count.saturating_add(1);
        env.storage().instance().set(&DataKey::AuctionCount, &count);

        let start_time = env.ledger().timestamp();
        let end_time = start_time.saturating_add(duration_seconds);
        let effective_min_bid = min_bid.unwrap_or(reserve_price);

        let auction = Auction {
            id: count,
            seller: seller.clone(),
            invoice_id: invoice_id.clone(),
            fraction_token,
            payment_token,
            amount,
            reserve_price,
            min_bid: effective_min_bid,
            start_time,
            end_time,
            highest_bidder: None,
            highest_bid: 0,
            status: AuctionStatus::Active,
        };

        env.storage()
            .persistent()
            .set(&DataKey::Auction(count), &auction);

        let bids: Vec<Bid> = Vec::new(&env);
        env.storage().persistent().set(&DataKey::Bids(count), &bids);

        // Publish AuctionCreated event
        env.events().publish(
            (Symbol::new(&env, "AuctionCreated"), count, seller),
            (invoice_id, amount, reserve_price, end_time),
        );

        Ok(count)
    }

    /// Place a bid in an active auction. Locks payment funds.
    pub fn place_bid(
        env: Env,
        bidder: Address,
        auction_id: u64,
        bid_amount: i128,
    ) -> Result<(), Error> {
        bidder.require_auth();

        let mut auction: Auction = env
            .storage()
            .persistent()
            .get(&DataKey::Auction(auction_id))
            .ok_or(Error::AuctionNotFound)?;

        if auction.status != AuctionStatus::Active {
            return Err(Error::AuctionNotActive);
        }

        let now = env.ledger().timestamp();
        if now >= auction.end_time {
            return Err(Error::AuctionEnded);
        }

        if bid_amount < auction.min_bid {
            return Err(Error::BelowReserve);
        }

        if bid_amount <= auction.highest_bid {
            return Err(Error::BidTooLow);
        }

        // Lock payment tokens from bidder
        token::Client::new(&env, &auction.payment_token).transfer(
            &bidder,
            &env.current_contract_address(),
            &bid_amount,
        );

        let now = env.ledger().timestamp();
        let bid = Bid {
            bidder: bidder.clone(),
            amount: bid_amount,
            timestamp: now,
        };

        let mut bids: Vec<Bid> = env
            .storage()
            .persistent()
            .get(&DataKey::Bids(auction_id))
            .unwrap_or_else(|| Vec::new(&env));
        bids.push_back(bid);
        env.storage()
            .persistent()
            .set(&DataKey::Bids(auction_id), &bids);

        auction.highest_bid = bid_amount;
        auction.highest_bidder = Some(bidder.clone());

        env.storage()
            .persistent()
            .set(&DataKey::Auction(auction_id), &auction);

        // Publish BidPlaced event
        env.events().publish(
            (Symbol::new(&env, "BidPlaced"), auction_id, bidder),
            bid_amount,
        );

        Ok(())
    }

    /// Settle auction after duration has passed.
    /// If reserve price is met, delivers fractions to winner and refunds losing bids.
    /// If reserve price is NOT met, cancels auction and refunds all bids and fractions.
    pub fn settle_auction(env: Env, auction_id: u64) -> Result<(), Error> {
        let mut auction: Auction = env
            .storage()
            .persistent()
            .get(&DataKey::Auction(auction_id))
            .ok_or(Error::AuctionNotFound)?;

        if auction.status != AuctionStatus::Active {
            return Err(Error::AuctionNotActive);
        }

        let now = env.ledger().timestamp();
        if now < auction.end_time {
            return Err(Error::AuctionNotEnded);
        }

        let bids: Vec<Bid> = env
            .storage()
            .persistent()
            .get(&DataKey::Bids(auction_id))
            .unwrap_or_else(|| Vec::new(&env));

        let reserve_met =
            auction.highest_bid >= auction.reserve_price && auction.highest_bidder.is_some();

        if reserve_met {
            let winner = auction.highest_bidder.clone().unwrap();

            // Transfer fractions to winning bidder
            token::Client::new(&env, &auction.fraction_token).transfer(
                &env.current_contract_address(),
                &winner,
                &auction.amount,
            );

            // Transfer highest bid payment to seller
            token::Client::new(&env, &auction.payment_token).transfer(
                &env.current_contract_address(),
                &auction.seller,
                &auction.highest_bid,
            );

            // Refund all non-winning bids
            let last_idx = bids.len().saturating_sub(1);
            for i in 0..last_idx {
                let bid = bids.get(i).unwrap();
                token::Client::new(&env, &auction.payment_token).transfer(
                    &env.current_contract_address(),
                    &bid.bidder,
                    &bid.amount,
                );
            }

            auction.status = AuctionStatus::Settled;
            env.storage()
                .persistent()
                .set(&DataKey::Auction(auction_id), &auction);

            // Publish AuctionSettled event
            env.events().publish(
                (Symbol::new(&env, "AuctionSettled"), auction_id, winner),
                (auction.amount, auction.highest_bid),
            );
        } else {
            // Reserve not met: cancel auction and refund all bids and fractions
            env.events().publish(
                (Symbol::new(&env, "ReserveNotMet"), auction_id),
                (auction.highest_bid, auction.reserve_price),
            );

            env.events().publish(
                (Symbol::new(&env, "AuctionCancelledReserveNotMet"), auction_id),
                auction.highest_bid,
            );

            // Refund all bids atomically
            for bid in bids.iter() {
                token::Client::new(&env, &auction.payment_token).transfer(
                    &env.current_contract_address(),
                    &bid.bidder,
                    &bid.amount,
                );
            }

            // Return fractions back to seller
            token::Client::new(&env, &auction.fraction_token).transfer(
                &env.current_contract_address(),
                &auction.seller,
                &auction.amount,
            );

            auction.status = AuctionStatus::Cancelled;
            env.storage()
                .persistent()
                .set(&DataKey::Auction(auction_id), &auction);
        }

        Ok(())
    }

    /// Check if reserve price has been met. Returns Error::ReserveNotMet if not met.
    pub fn verify_reserve(env: Env, auction_id: u64) -> Result<(), Error> {
        let auction: Auction = env
            .storage()
            .persistent()
            .get(&DataKey::Auction(auction_id))
            .ok_or(Error::AuctionNotFound)?;

        if auction.highest_bid < auction.reserve_price || auction.highest_bidder.is_none() {
            return Err(Error::ReserveNotMet);
        }

        Ok(())
    }

    /// Query auction details
    pub fn get_auction(env: Env, auction_id: u64) -> Result<Auction, Error> {
        env.storage()
            .persistent()
            .get(&DataKey::Auction(auction_id))
            .ok_or(Error::AuctionNotFound)
    }

    /// Query auction reserve status and highest bid
    pub fn get_auction_status(env: Env, auction_id: u64) -> Result<AuctionStatusInfo, Error> {
        let auction: Auction = env
            .storage()
            .persistent()
            .get(&DataKey::Auction(auction_id))
            .ok_or(Error::AuctionNotFound)?;

        let reserve_met =
            auction.highest_bid >= auction.reserve_price && auction.highest_bidder.is_some();

        Ok(AuctionStatusInfo {
            highest_bid: auction.highest_bid,
            reserve_price: auction.reserve_price,
            reserve_met,
            status: auction.status,
        })
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use soroban_sdk::{
        testutils::{Address as _, Ledger},
        token::StellarAssetClient as AssetClient,
        Address, BytesN, Env,
    };

    fn setup_test() -> (
        Env,
        Address,
        Address,
        Address,
        Address,
        Address,
        Address,
        FractionAuctionClient<'static>,
    ) {
        let env = Env::default();
        env.mock_all_auths();

        let admin = Address::generate(&env);
        let seller = Address::generate(&env);
        let bidder1 = Address::generate(&env);
        let bidder2 = Address::generate(&env);

        let token_admin = Address::generate(&env);
        let fraction_token_id = env.register_stellar_asset_contract_v2(token_admin.clone());
        let fraction_token = fraction_token_id.address();

        let payment_token_id = env.register_stellar_asset_contract_v2(token_admin);
        let payment_token = payment_token_id.address();

        let contract_id = env.register(FractionAuction, ());
        let client = FractionAuctionClient::new(&env, &contract_id);
        client.initialize(&admin);

        (
            env,
            admin,
            seller,
            bidder1,
            bidder2,
            fraction_token,
            payment_token,
            client,
        )
    }

    #[test]
    fn test_successful_auction_settlement_with_reserve_met() {
        let (env, _admin, seller, bidder1, bidder2, fraction_token, payment_token, client) =
            setup_test();

        let frac_asset = AssetClient::new(&env, &fraction_token);
        let pay_asset = AssetClient::new(&env, &payment_token);

        let fraction_amount: i128 = 1000;
        frac_asset.mint(&seller, &fraction_amount);

        pay_asset.mint(&bidder1, &5000);
        pay_asset.mint(&bidder2, &5000);

        let invoice_id = BytesN::from_array(&env, &[1u8; 32]);
        let reserve_price: i128 = 2000;
        let duration: u64 = 3600;

        // Seller creates auction
        let auction_id = client.create_auction(
            &seller,
            &invoice_id,
            &fraction_token,
            &payment_token,
            &fraction_amount,
            &reserve_price,
            &duration,
            &None,
        );
        assert_eq!(auction_id, 1);

        // Fractions transferred to contract
        assert_eq!(frac_asset.balance(&seller), 0);

        // Bidder 1 bids below reserve price -> rejected with BelowReserve
        let low_bid = client.try_place_bid(&bidder1, &auction_id, &1500);
        assert_eq!(low_bid, Err(Ok(Error::BelowReserve)));

        // Bidder 1 places valid bid meeting reserve
        client.place_bid(&bidder1, &auction_id, &2100);
        assert_eq!(pay_asset.balance(&bidder1), 2900);

        // Bidder 2 places lower or equal bid -> rejected with BidTooLow
        let too_low_bid = client.try_place_bid(&bidder2, &auction_id, &2100);
        assert_eq!(too_low_bid, Err(Ok(Error::BidTooLow)));

        let lower_bid = client.try_place_bid(&bidder2, &auction_id, &2050);
        assert_eq!(lower_bid, Err(Ok(Error::BidTooLow)));

        // Bidder 2 places higher bid
        client.place_bid(&bidder2, &auction_id, &2500);
        assert_eq!(pay_asset.balance(&bidder2), 2500);

        // Check auction status before settlement
        let status_info = client.get_auction_status(&auction_id);
        assert_eq!(status_info.highest_bid, 2500);
        assert_eq!(status_info.reserve_price, 2000);
        assert_eq!(status_info.reserve_met, true);
        assert_eq!(status_info.status, AuctionStatus::Active);

        // Settlement fails before duration expires
        let settle_early = client.try_settle_auction(&auction_id);
        assert_eq!(settle_early, Err(Ok(Error::AuctionNotEnded)));

        // Advance ledger timestamp
        env.ledger().set_timestamp(duration + 10);

        // Settle auction
        client.settle_auction(&auction_id);

        // Fractions transferred to winner (bidder2)
        assert_eq!(frac_asset.balance(&bidder2), fraction_amount);

        // Winning bid transferred to seller
        assert_eq!(pay_asset.balance(&seller), 2500);

        // Losing bidder (bidder1) refunded
        assert_eq!(pay_asset.balance(&bidder1), 5000);

        // Status updated to Settled
        let settled_info = client.get_auction_status(&auction_id);
        assert_eq!(settled_info.status, AuctionStatus::Settled);
    }

    #[test]
    fn test_reserve_not_met_settlement_cancels_and_refunds_all() {
        let (env, _admin, seller, bidder1, _bidder2, fraction_token, payment_token, client) =
            setup_test();

        let frac_asset = AssetClient::new(&env, &fraction_token);
        let pay_asset = AssetClient::new(&env, &payment_token);

        let fraction_amount: i128 = 1000;
        frac_asset.mint(&seller, &fraction_amount);
        pay_asset.mint(&bidder1, &5000);

        let invoice_id = BytesN::from_array(&env, &[2u8; 32]);
        let reserve_price: i128 = 3000;
        let duration: u64 = 3600;

        // Create auction with min_bid lower than reserve price to allow bidding below reserve
        let auction_id = client.create_auction(
            &seller,
            &invoice_id,
            &fraction_token,
            &payment_token,
            &fraction_amount,
            &reserve_price,
            &duration,
            &Some(1000),
        );

        // Bidder 1 bids 1500 (meets min_bid 1000, but below reserve 3000)
        client.place_bid(&bidder1, &auction_id, &1500);
        assert_eq!(pay_asset.balance(&bidder1), 3500);

        // Status check
        let status = client.get_auction_status(&auction_id);
        assert_eq!(status.highest_bid, 1500);
        assert_eq!(status.reserve_price, 3000);
        assert_eq!(status.reserve_met, false);

        // verify_reserve fails
        assert_eq!(
            client.try_verify_reserve(&auction_id),
            Err(Ok(Error::ReserveNotMet))
        );

        // Advance ledger timestamp
        env.ledger().set_timestamp(duration + 10);

        // Settle auction -> reserve not met cancels and refunds
        client.settle_auction(&auction_id);

        // All bids refunded to bidder1
        assert_eq!(pay_asset.balance(&bidder1), 5000);

        // Fractions returned to seller
        assert_eq!(frac_asset.balance(&seller), fraction_amount);

        // Seller receives no payment
        assert_eq!(pay_asset.balance(&seller), 0);

        // Auction status is Cancelled
        let final_status = client.get_auction_status(&auction_id);
        assert_eq!(final_status.status, AuctionStatus::Cancelled);
        assert_eq!(final_status.reserve_met, false);
    }

    #[test]
    fn test_reserve_not_met_with_zero_bids() {
        let (env, _admin, seller, _bidder1, _bidder2, fraction_token, payment_token, client) =
            setup_test();

        let frac_asset = AssetClient::new(&env, &fraction_token);

        let fraction_amount: i128 = 500;
        frac_asset.mint(&seller, &fraction_amount);

        let invoice_id = BytesN::from_array(&env, &[3u8; 32]);
        let reserve_price: i128 = 2000;
        let duration: u64 = 1800;

        let auction_id = client.create_auction(
            &seller,
            &invoice_id,
            &fraction_token,
            &payment_token,
            &fraction_amount,
            &reserve_price,
            &duration,
            &None,
        );

        // Advance ledger timestamp past duration
        env.ledger().set_timestamp(duration + 10);

        // Settle with zero bids
        client.settle_auction(&auction_id);

        // Fractions returned to seller
        assert_eq!(frac_asset.balance(&seller), fraction_amount);

        let status = client.get_auction_status(&auction_id);
        assert_eq!(status.status, AuctionStatus::Cancelled);
        assert_eq!(status.highest_bid, 0);
        assert_eq!(status.reserve_met, false);
    }
}
