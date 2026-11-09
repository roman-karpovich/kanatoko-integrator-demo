//! An example dollar-cost-averaging guard: a user may swap at most once per day
//! through a router, and only while the quote meets their limit. Not audited;
//! not for production.

#![no_std]

use soroban_sdk::{contract, contractclient, contracterror, contractimpl, Address, Env, Vec};

const INTERVAL: u64 = 86_400;

#[contractclient(name = "RouterClient")]
pub trait Router {
    fn router_get_amounts_out(env: Env, amount_in: i128, path: Vec<Address>) -> Vec<i128>;
    fn swap_exact_tokens_for_tokens(
        env: Env,
        amount_in: i128,
        amount_out_min: i128,
        path: Vec<Address>,
        to: Address,
        deadline: u64,
    ) -> Vec<i128>;
}

#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum Error {
    TooEarly = 1,
    BelowLimit = 2,
}

#[contract]
pub struct Dca;

#[contractimpl]
impl Dca {
    pub fn execute(
        env: Env,
        router: Address,
        user: Address,
        path: Vec<Address>,
        amount_in: i128,
        min_out: i128,
    ) -> Result<i128, Error> {
        user.require_auth();

        let store = env.storage().instance();
        let now = env.ledger().timestamp();
        if store
            .get(&user)
            .is_some_and(|last: u64| now < last + INTERVAL)
        {
            return Err(Error::TooEarly);
        }

        let router = RouterClient::new(&env, &router);
        let quote = router.router_get_amounts_out(&amount_in, &path);
        if quote.get_unchecked(1) < min_out {
            return Err(Error::BelowLimit);
        }

        store.set(&user, &now);
        let deadline = now + 60;
        let amounts =
            router.swap_exact_tokens_for_tokens(&amount_in, &min_out, &path, &user, &deadline);
        Ok(amounts.get_unchecked(1))
    }
}
