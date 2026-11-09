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
        sha256 = "4dc4e0c72e86b16541c0808afde2eab69ec7cb9a35c8e2aa415e03bec2077c7f",
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

fn usdc_to_xlm(fork: &ScenarioFork<'_>) -> Vec<Address> {
    vec![fork.env(), fork.contract(USDC), fork.contract(XLM)]
}

/// The real pair's reserves, as (XLM, USDC).
fn pool_reserves(fork: &ScenarioFork<'_>) -> (i128, i128) {
    fork.invoke(&fork.contract(PAIR), "get_reserves", ())
}

/// XLM out for ten USDC in, quoted by the real router.
fn quote_ten_usdc(fork: &ScenarioFork<'_>) -> i128 {
    let args = (TEN_USDC, usdc_to_xlm(fork));
    let amounts: Vec<i128> = fork.invoke(&fork.contract(ROUTER), "router_get_amounts_out", args);
    amounts.get_unchecked(1)
}

#[test]
fn dca_runs_against_the_real_pool() {
    runner()
        .run(|fork| {
            fork.mock_all_auths();
            let env = fork.env();
            let router = fork.contract(ROUTER);
            let user = local_user(fork, "dca-user", 10 * TEN_USDC);

            let limit = quote_ten_usdc(fork) * 9 / 10;
            let settings = (router.clone(), user, usdc_to_xlm(fork), TEN_USDC, limit);
            let dca_address = fork.deploy(dca::WASM, settings);
            let dca = dca::Client::new(env, &dca_address);

            // The first run swaps through the real router and moves the real pool.
            let (xlm_before, usdc_before) = pool_reserves(fork);
            let xlm_out = dca.execute();
            let expected = (xlm_before - xlm_out, usdc_before + TEN_USDC);
            assert_eq!(pool_reserves(fork), expected);

            // Immediately again: the contract refuses.
            assert_eq!(dca.try_execute(), Err(Ok(dca::Error::TooEarly)));

            // One day later it runs again.
            env.ledger().set_timestamp(env.ledger().timestamp() + DAY);
            let second_xlm_out = dca.execute();

            // Another account makes a large swap, moving the real pool's price.
            let (_, usdc_reserve) = pool_reserves(fork);
            let whale_usdc = usdc_reserve / 5;
            let whale = local_user(fork, "whale", whale_usdc);
            let swap_args = (whale_usdc, 0_i128, usdc_to_xlm(fork), whale, u64::MAX);
            let _: Vec<i128> = fork.invoke(&router, "swap_exact_tokens_for_tokens", swap_args);
            let moved_quote = quote_ten_usdc(fork);

            // The next day the quote is below the limit, so the contract refuses.
            env.ledger().set_timestamp(env.ledger().timestamp() + DAY);
            assert_eq!(dca.try_execute(), Err(Ok(dca::Error::BelowLimit)));
            println!("first {xlm_out}, second {second_xlm_out}, whale {whale_usdc}");
            println!("quote after the large swap {moved_quote}, limit {limit}");
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
