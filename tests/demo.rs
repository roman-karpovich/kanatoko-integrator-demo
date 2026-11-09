//! A locally deployed DCA contract against the real Soroswap router and pair.

use std::path::Path;

use kanatoko::{
    mainnet,
    soroban_sdk::{testutils::Ledger as _, vec, Address, Vec},
    AutoRunError, AutoRunner, CaptureError, PreviewAuth, ScenarioFork,
};

mod dca {
    use kanatoko::soroban_sdk;

    soroban_sdk::contractimport!(
        file = "fixtures/dca.wasm",
        sha256 = "c033142dfdca6dc646095d308b23c66f4b984c3b0e716927cd0a561172445807",
    );
}

const BUNDLE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/capture.json");
const ROUTER: &str = "CAG5LRYQ5JVEUI5TEID72EYOVX44TTUJT5BQR2J6J77FH65PCCFAJDDH";
const PAIR: &str = "CAM7DY53G63XA4AJRS24Z6VFYAFSSF76C3RZ45BE5YU3FQS5255OOABP";
const XLM: &str = "CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA";
const USDC: &str = "CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75";
const DAY: u64 = 86_400;
const TEN_USDC: i128 = 100_000_000;

/// Replays the committed bundle offline. If the bundle has been deleted, this
/// captures a new one from the public RPC instead.
fn runner() -> AutoRunner {
    let runner = mainnet().cache(BUNDLE);
    if Path::new(BUNDLE).exists() {
        runner.offline()
    } else {
        runner
    }
}

/// A local account holding XLM and `usdc`. No real account or key is involved.
fn local_user(fork: &ScenarioFork<'_>, label: &str, usdc: i128) -> Address {
    let user = fork.local_account(label);
    fork.fund_local_account(&user, 100_000_000);
    let token = fork.contract(USDC);
    fork.invoke::<()>(&token, "trust", (user.clone(),));
    fork.invoke::<()>(&token, "set_authorized", (user.clone(), true));
    fork.invoke::<()>(&token, "mint", (user.clone(), usdc));
    user
}

#[test]
fn dca_runs_against_the_real_pool() {
    runner()
        .run(|fork| {
            fork.mock_all_auths();
            let env = fork.env();
            let (router, pair) = (fork.contract(ROUTER), fork.contract(PAIR));
            let path = vec![env, fork.contract(USDC), fork.contract(XLM)];
            let reserves = || -> (i128, i128) { fork.invoke(&pair, "get_reserves", ()) };
            let quote = || {
                let args = (TEN_USDC, path.clone());
                let out: Vec<i128> = fork.invoke(&router, "router_get_amounts_out", args);
                out.get_unchecked(1)
            };

            let user = local_user(fork, "dca-user", 10 * TEN_USDC);
            let limit = quote() * 9 / 10;
            let id = fork.deploy(dca::WASM, ());
            let dca = dca::Client::new(env, &id);
            let execute = || dca.try_execute(&router, &user, &path, &TEN_USDC, &limit);

            // The first run swaps through the real router and moves the real pool.
            let before = reserves();
            let first = execute().unwrap().unwrap();
            assert_eq!(reserves(), (before.0 - first, before.1 + TEN_USDC));

            // Immediately again: the contract refuses.
            assert_eq!(execute(), Err(Ok(dca::Error::TooEarly)));

            // One day later it runs again.
            let start = env.ledger().timestamp();
            env.ledger().set_timestamp(start + DAY);
            let second = execute().unwrap().unwrap();

            // Another account makes a large swap, moving the real pool's price.
            let size = reserves().1 / 5;
            let whale = local_user(fork, "whale", size);
            let args = (size, 0_i128, path.clone(), whale, u64::MAX);
            let _: Vec<i128> = fork.invoke(&router, "swap_exact_tokens_for_tokens", args);
            let moved = quote();

            // The next day the quote is below the limit, so the contract refuses.
            env.ledger().set_timestamp(start + 2 * DAY);
            assert_eq!(execute(), Err(Ok(dca::Error::BelowLimit)));
            println!("first {first}, second {second}, whale {size}, quote {moved}, limit {limit}");
        })
        .expect("the story must replay from the bundle");
}

#[test]
fn a_read_outside_the_capture_fails_closed() {
    let result = mainnet().cache(BUNDLE).offline().run(|fork| {
        let args = (fork.contract(ROUTER),);
        let _ = fork.preview(&fork.contract(USDC), "balance", args, PreviewAuth::Record);
    });
    assert!(matches!(
        result,
        Err(AutoRunError::Capture(CaptureError::UnknownLedgerKeys {
            count: 1
        }))
    ));
}
