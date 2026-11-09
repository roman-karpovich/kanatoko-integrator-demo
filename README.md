# Kanatoko integrator demo

Write your own Soroban contract on top of a real protocol and test it locally
against real mainnet state, with no deployment, wallet, faucet or fees, while
the test controls time and price. This repository shows that with
[Kanatoko](https://github.com/roman-karpovich/kanatoko).

- This is an example contract. It is not audited and not for production.
- It is not affiliated with Soroswap.
- The deployed code is not modified: the Soroswap router and pair run their real
  WASM from the capture.
- Ledger time and price here are local test conditions. They say nothing about
  how the network or market will behave.
- A passing test does not prove fees, transaction admission, ordering or state
  archival.

## What it shows

`contracts/dca` lets a user swap through a router at most once per day, and only
while the quote meets the user's limit. Its settings are fixed at installation.
The test deploys it locally and runs it against the real Soroswap router
`CAG5LRYQ5JVEUI5TEID72EYOVX44TTUJT5BQR2J6J77FH65PCCFAJDDH` and XLM/USDC pair
`CAM7DY53G63XA4AJRS24Z6VFYAFSSF76C3RZ45BE5YU3FQS5255OOABP`.

```rust
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
```

## Observed results

Captured at mainnet ledger 65391822, protocol 29. All amounts are in stroops.
The limit is 90% of the starting quote for 10 USDC.

| Step | Result |
| --- | --- |
| Starting reserves (XLM, USDC) | `(3671540542787, 543638564537)` |
| First `execute` | swaps 10 USDC for `673214699` XLM; reserves become `(3670867328088, 543738564537)` |
| Immediately again | `TooEarly` |
| One day later | swaps 10 USDC for `672967491` XLM |
| Another account swaps `108767712907` USDC | 10 USDC now quotes `467414937` XLM, below the limit of `605893229` |
| Next day | `BelowLimit` |
| USDC balance of an address outside the capture | `UnknownLedgerKeys` (`count: 1`) |

`cargo test --locked -- --nocapture` prints these swap and quote values.

## Run it

```sh
git clone https://github.com/roman-karpovich/kanatoko-integrator-demo.git
cd kanatoko-integrator-demo
cargo test --locked
```

The tests replay the committed bundle offline and make no RPC calls.

## Re-capture

Delete the bundle and run the main test. With no bundle present, the runner
captures a new one from the public RPC `https://mainnet.sorobanrpc.com`:

```sh
rm fixtures/capture.json
cargo test --locked dca_runs_against_the_real_pool
cargo test --locked
```

The assertions are relative, so they hold on a new capture. The table above
describes the committed one.

## Rebuild the contract

`fixtures/dca.wasm` is committed, and its SHA-256 is pinned in
`tests/demo.rs`: `4dc4e0c72e86b16541c0808afde2eab69ec7cb9a35c8e2aa415e03bec2077c7f`.

```sh
RUSTUP_TOOLCHAIN=1.92.0 stellar contract build \
  --manifest-path contracts/dca/Cargo.toml \
  --out-dir fixtures \
  --optimize \
  --locked
```

Built with stellar-cli 27.0.0 and rustc 1.92.0. The contract is excluded from
the host build.

## License

Licensed under either the MIT License or the Apache License, Version 2.0, at
your option. See `LICENSE-MIT`, `LICENSE-APACHE` and `NOTICE`.
