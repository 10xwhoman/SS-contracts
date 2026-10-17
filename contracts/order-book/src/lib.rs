#![no_std]

mod errors;
mod events;
mod types;

#[cfg(test)]
mod test;

pub use errors::Error;
use soroban_sdk::{contract, contractimpl, token, Address, BytesN, Env, Vec};
pub use types::{BuyOrder, MatchResult, StorageKey};

#[contract]
pub struct OrderBook;

#[contractimpl]
impl OrderBook {
    pub fn initialize(env: Env, admin: Address, token: Address) -> Result<(), Error> {
        if env.storage().instance().has(&StorageKey::Admin) {
            return Err(Error::AlreadyInitialized);
        }
        env.storage().instance().set(&StorageKey::Admin, &admin);
        env.storage().instance().set(&StorageKey::Token, &token);
        env.storage().instance().set(&StorageKey::NextOrderId, &1u32);
        Ok(())
    }

    pub fn place_buy_order(
        env: Env,
        buyer: Address,
        invoice_id: BytesN<32>,
        fraction_amount: i128,
        max_price_per_fraction: i128,
    ) -> Result<u32, Error> {
        buyer.require_auth();

        if fraction_amount <= 0 {
            return Err(Error::InvalidAmount);
        }
        if max_price_per_fraction <= 0 {
            return Err(Error::InvalidPrice);
        }

        let escrowed_funds = fraction_amount
            .checked_mul(max_price_per_fraction)
            .ok_or(Error::InvalidAmount)?;

        let token_addr: Address = env
            .storage()
            .instance()
            .get(&StorageKey::Token)
            .ok_or(Error::NotInitialized)?;

        let client = token::Client::new(&env, &token_addr);
        if client.balance(&buyer) < escrowed_funds {
            return Err(Error::InsufficientEscrow);
        }
        // Escrow funds into contract
        client.transfer(&buyer, &env.current_contract_address(), &escrowed_funds);

        let order_id: u32 = env
            .storage()
            .instance()
            .get(&StorageKey::NextOrderId)
            .unwrap_or(1);
        env.storage()
            .instance()
            .set(&StorageKey::NextOrderId, &(order_id + 1));

        let current_time = env.ledger().timestamp();
        let order = BuyOrder {
            order_id,
            buyer: buyer.clone(),
            invoice_id: invoice_id.clone(),
            fraction_amount,
            max_price_per_fraction,
            escrowed_funds,
            is_active: true,
            created_at: current_time,
        };

        env.storage()
            .persistent()
            .set(&StorageKey::Order(order_id), &order);

        let mut orders: Vec<u32> = env
            .storage()
            .persistent()
            .get(&StorageKey::InvoiceOrders(invoice_id.clone()))
            .unwrap_or(Vec::new(&env));
        orders.push_back(order_id);
        env.storage()
            .persistent()
            .set(&StorageKey::InvoiceOrders(invoice_id.clone()), &orders);

        events::buy_order_placed(
            &env,
            order_id,
            &buyer,
            &invoice_id,
            fraction_amount,
            max_price_per_fraction,
        );

        Ok(order_id)
    }

    pub fn cancel_order(env: Env, buyer: Address, order_id: u32) -> Result<i128, Error> {
        buyer.require_auth();

        let mut order: BuyOrder = env
            .storage()
            .persistent()
            .get(&StorageKey::Order(order_id))
            .ok_or(Error::OrderNotFound)?;

        if order.buyer != buyer {
            return Err(Error::Unauthorized);
        }
        if !order.is_active {
            return Err(Error::OrderNotActive);
        }

        order.is_active = false;
        let refund_amount = order.escrowed_funds;
        order.escrowed_funds = 0;

        env.storage()
            .persistent()
            .set(&StorageKey::Order(order_id), &order);

        let token_addr: Address = env
            .storage()
            .instance()
            .get(&StorageKey::Token)
            .ok_or(Error::NotInitialized)?;

        let client = token::Client::new(&env, &token_addr);
        if refund_amount > 0 && client.balance(&env.current_contract_address()) >= refund_amount {
            client.transfer(&env.current_contract_address(), &buyer, &refund_amount);
        }

        events::order_cancelled(&env, order_id, &buyer, refund_amount);

        Ok(refund_amount)
    }

    pub fn match_listing(
        env: Env,
        seller: Address,
        invoice_id: BytesN<32>,
        listing_amount: i128,
        ask_price_per_fraction: i128,
    ) -> Result<Vec<MatchResult>, Error> {
        seller.require_auth();

        if listing_amount <= 0 {
            return Err(Error::InvalidAmount);
        }
        if ask_price_per_fraction <= 0 {
            return Err(Error::InvalidPrice);
        }

        let token_addr: Address = env
            .storage()
            .instance()
            .get(&StorageKey::Token)
            .ok_or(Error::NotInitialized)?;
        let client = token::Client::new(&env, &token_addr);

        let open_orders = Self::get_open_orders(env.clone(), invoice_id.clone())?;
        let mut results = Vec::new(&env);
        let mut remaining_listing = listing_amount;

        for i in 0..open_orders.len() {
            if remaining_listing <= 0 {
                break;
            }

            let mut order = open_orders.get(i).unwrap();
            if !order.is_active || order.max_price_per_fraction < ask_price_per_fraction {
                continue;
            }

            let match_amount = order.fraction_amount.min(remaining_listing);
            let execution_price = ask_price_per_fraction; // match at seller's ask price
            let total_cost = match_amount * execution_price;

            // Refund any price improvement surplus to buyer
            let escrow_for_matched = match_amount * order.max_price_per_fraction;
            let surplus = escrow_for_matched.saturating_sub(total_cost);

            if surplus > 0 && client.balance(&env.current_contract_address()) >= surplus {
                client.transfer(&env.current_contract_address(), &order.buyer, &surplus);
            }

            // Transfer payment to seller
            if total_cost > 0 && client.balance(&env.current_contract_address()) >= total_cost {
                client.transfer(&env.current_contract_address(), &seller, &total_cost);
            }

            // Update order state
            order.fraction_amount = order.fraction_amount.saturating_sub(match_amount);
            order.escrowed_funds = order.escrowed_funds.saturating_sub(escrow_for_matched);
            if order.fraction_amount == 0 {
                order.is_active = false;
            }

            env.storage()
                .persistent()
                .set(&StorageKey::Order(order.order_id), &order);

            remaining_listing = remaining_listing.saturating_sub(match_amount);

            let res = MatchResult {
                order_id: order.order_id,
                buyer: order.buyer.clone(),
                seller: seller.clone(),
                matched_amount: match_amount,
                execution_price,
                total_cost,
            };
            results.push_back(res);

            events::order_matched(
                &env,
                order.order_id,
                &invoice_id,
                &order.buyer,
                &seller,
                execution_price,
                match_amount,
            );
        }

        Ok(results)
    }

    pub fn get_open_orders(env: Env, invoice_id: BytesN<32>) -> Result<Vec<BuyOrder>, Error> {
        let order_ids: Vec<u32> = env
            .storage()
            .persistent()
            .get(&StorageKey::InvoiceOrders(invoice_id))
            .unwrap_or(Vec::new(&env));

        let mut open_orders = Vec::new(&env);
        for i in 0..order_ids.len() {
            let oid = order_ids.get(i).unwrap();
            if let Some(order) = env
                .storage()
                .persistent()
                .get::<_, BuyOrder>(&StorageKey::Order(oid))
            {
                if order.is_active && order.fraction_amount > 0 {
                    open_orders.push_back(order);
                }
            }
        }

        // Sort by max_price_per_fraction descending (highest price first)
        let len = open_orders.len();
        if len > 1 {
            for i in 0..len {
                for j in (i + 1)..len {
                    let o_i = open_orders.get(i).unwrap();
                    let o_j = open_orders.get(j).unwrap();
                    if o_j.max_price_per_fraction > o_i.max_price_per_fraction {
                        open_orders.set(i, o_j);
                        open_orders.set(j, o_i);
                    }
                }
            }
        }

        Ok(open_orders)
    }

    pub fn get_order(env: Env, order_id: u32) -> Result<BuyOrder, Error> {
        env.storage()
            .persistent()
            .get(&StorageKey::Order(order_id))
            .ok_or(Error::OrderNotFound)
    }
}
