# Presale

A Solana program for a flat-price raise that fills into a [Pump.fun](https://pump.fun) coin.

People deposit SOL or USDC while the raise is open. Every depositor pays the same price: the quote required to buy the entire Pump bonding curve, including Pump’s protocol and creator fees, divided by the tokens that buy receives. When the vault reaches that target, anyone can fill it. Fill creates the coin and buys the whole curve. A second transaction migrates the coin to PumpSwap. Depositors then claim their tokens.

The Flatpad site is a separate app. This repository is the on-chain program and the [keeper](keeper/README.md) that sends fill, migrate, and claim transactions. Those instructions are permissionless, so anyone can run a keeper.

Program id: `EGFHbbVkZKhv3owPhUocyBqTQKeYSuehaxQ7ZMLZ2p1D`

## Lifecycle

1. **Create.** The launcher chooses the quote mint, metadata, and fee mode. Those choices are permanent. The coin mint is a program PDA, so its address is known before the coin exists. The presale account, not this program’s executable id, is the Pump creator.
2. **Deposit.** Anyone can deposit while the status is Raising. Deposits from the same wallet stack on one position. The program caps each deposit at the room left, so the vault lands on the target exactly.
3. **Withdraw.** The depositor can take back some or all of their own quote, and only while the raise is still open. Withdrawing the full position closes it and returns the small SOL fee escrowed on the first deposit.
4. **Fill.** Anyone can fill once `quote_raised` equals `quote_target` and Pump’s curve inputs and fees still match the snapshot taken at creation. Fill creates the coin with `create_v2` and buys the curve with `buy_v2`. Status becomes Bought. Withdrawals are closed. Tokens stay in the buyer account.
5. **Migrate.** A second transaction calls Pump `migrate_v2` and sets Migrated. Create, buy, and migrate together exceed Solana’s instruction-trace limit, which is why they are split. If Pump has already migrated that coin, this instruction sees the canonical PumpSwap pool and opens claims without calling migrate again.
6. **Claim.** After Migrated, anyone can pay a depositor their tokens. The payout is `position.quote * tokens_received / quote_target`, rounded down. Dust stays on the buyer account. The position closes.

If Pump’s global or fee config changes before fill, fill refuses and the raise stays open. Withdrawals still work. A new raise has to be created against the new parameters.

## Quote

The quote mint must be wrapped SOL, or the single mint in Pump’s `Global.whitelisted_quote_mints`. Today that mint is mainnet USDC. Any other mint is rejected.

| Quote | Where the vault holds it |
| --- | --- |
| SOL | Native lamports on the buyer PDA `["buyer", presale]` |
| USDC | The presale quote token account |

Pass the wrapped SOL mint when the pair is SOL. The buyer account is system-owned because Pump’s buy transfers SOL with the system program, which refuses a source account that carries data.

## Fees

The launcher picks one fee destination at creation.

**Creator.** Pump’s creator fee is collected in the quote asset and split 70% to the dev wallet and 30% to the platform admin. Dust stays with the creator. Both shares are paid in the same crank. Neither side can collect alone.

**Holders.** The whole fee is indexed for current token holders. The platform takes no cut. The index pays by token balance at claim time, excluding tokens still in the pool and tokens still unclaimed in the buyer account. Pump keeps mint authority and there is no transfer hook, so a wallet can buy just before the crank and sell after. That is a property of this mode.

The platform admin is hardcoded. Changing it takes a program upgrade.

```text
JA9ijmzmtGcZCHNxw12g4kqJFNtTiGZs8MkQr4ddcTvW
```

A first deposit also escrows 0.003 SOL on the position. That pays whoever later sends the depositor their tokens. A full withdrawal while the raise is open returns it.

Leave Pump’s fee-sharing config unset for this mint. That config blocks `collect_creator_fee_v2`. Pump’s admin can still reassign the coin creator later. After that, this program stops receiving the creator fee.

The program does not swap fees into the token. Mayhem mode, cashback, and Pump’s holder-reward flag stay off.

## Status

| Status | Meaning |
| --- | --- |
| Raising | Deposits and withdrawals are open. |
| Bought | The curve is bought. Withdrawals are closed. Claims wait for migration. |
| Migrated | The coin is on PumpSwap. Token claims are open. |

## Build

Install [Solana platform-tools](https://docs.solanalabs.com/cli/install) v1.51 (rustc 1.84) and Anchor 0.30.1.

```sh
cargo test --manifest-path programs/presale/Cargo.toml
cargo-build-sbf --manifest-path programs/presale/Cargo.toml
anchor idl build --program-name presale --out idl/presale.json
```

`Cargo.lock` pins a few transitive crates so those platform-tools can parse them. A blanket `cargo update` can pull crates the tools cannot compile.

The program deploy keypair is generated at `target/deploy/presale-keypair.json` and is gitignored. Back it up before any deploy. Mainnet deploys use the platform admin key as the upgrade authority. That key is not in this repository. Back it up, and move the upgrade authority to a multisig before the program holds other people’s funds.

## Related programs

| Name | Address |
| --- | --- |
| This program | `EGFHbbVkZKhv3owPhUocyBqTQKeYSuehaxQ7ZMLZ2p1D` |
| Pump | `6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P` |
| PumpSwap | `pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA` |
| Pump fees | `pfeeUxB6jkeY1Hxd7CsFCAjcbHA9rWtchMGdZ6VojVZ` |
| Token-2022 | `TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb` |
| Wrapped SOL | `So11111111111111111111111111111111111111112` |
| USDC | `EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v` |

## Keeper

Build and run instructions, including how to host a keeper, are in [keeper/README.md](keeper/README.md).
