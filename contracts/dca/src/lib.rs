//! An example dollar-cost-averaging contract. Its settings are fixed when it is
//! installed. A user can then swap through a router at most once per day, and
//! only while the quote meets their limit. Not audited; not for production.

#![no_std]

use soroban_sdk::{
    contract, contractclient, contracterror, contractimpl, contracttype, symbol_short, Address,
    Env, Symbol, Vec,
};

const SETTINGS: Symbol = symbol_short!("SETTINGS");
const LAST_RUN: Symbol = symbol_short!("LAST_RUN");
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

#[contracttype]
pub struct Settings {
    pub router: Address,
    pub user: Address,
    pub path: Vec<Address>,
    pub amount_in: i128,
    pub min_out: i128,
}

#[contract]
pub struct Dca;

#[contractimpl]
impl Dca {
    pub fn __constructor(
        env: Env,
        router: Address,
        user: Address,
        path: Vec<Address>,
        amount_in: i128,
        min_out: i128,
    ) {
        let settings = Settings {
            router,
            user,
            path,
            amount_in,
            min_out,
        };
        env.storage().instance().set(&SETTINGS, &settings);
    }

    pub fn execute(env: Env) -> Result<i128, Error> {
        let store = env.storage().instance();
        let settings: Settings = store.get(&SETTINGS).unwrap();
        settings.user.require_auth();

        let now = env.ledger().timestamp();
        let last_run: Option<u64> = store.get(&LAST_RUN);
        if last_run.is_some_and(|last_run| now < last_run + INTERVAL) {
            return Err(Error::TooEarly);
        }

        let router = RouterClient::new(&env, &settings.router);
        let quote = router.router_get_amounts_out(&settings.amount_in, &settings.path);
        if quote.get_unchecked(1) < settings.min_out {
            return Err(Error::BelowLimit);
        }

        store.set(&LAST_RUN, &now);
        let amounts = router.swap_exact_tokens_for_tokens(
            &settings.amount_in,
            &settings.min_out,
            &settings.path,
            &settings.user,
            &(now + 60),
        );
        Ok(amounts.get_unchecked(1))
    }
}
