# Presale keeper

A command-line client for the [presale program](../README.md). It opens raises, deposits, withdraws, and runs the permissionless cranks: fill, migrate, token claims, and fee claims.

`run` is the hosted process. It watches every presale, fills a full raise, migrates one that is already bought, and then pays each depositor their tokens. It does not crank creator or holder fees. Those are separate commands.

The keeper holds no deposits. User quote sits in the program’s vaults. The keeper only pays transaction fees, and a first deposit escrows 0.003 SOL that later reimburses whoever sends that depositor their tokens.

## Install

Node.js 22 or newer.

```sh
cd keeper
npm install
```

The client loads `idl/presale.json` from the repository root, relative to its source file. Run the npm scripts from `keeper/`.

Every command takes:

| Flag | Default | |
| --- | --- | --- |
| `--cluster` | `mainnet` | `mainnet`, `devnet`, or `localnet` |
| `--url` | the public RPC for `--cluster` | overrides the RPC endpoint |
| `--keypair` | `~/.config/solana/id.json` | the signer |

`--cluster` still chooses the USDC mint on `create` when `--url` points at a private RPC. Pass both.

Amounts are base units. SOL amounts are lamports. USDC amounts have 6 decimals.

## Commands

```sh
npm run keeper -- <command> [options]
```

### Create

The signer becomes the launcher. The quote, metadata, fee mode, and dev wallet are permanent.

```sh
npm run keeper -- create \
  --cluster mainnet \
  --quote sol \
  --name "Example" \
  --symbol EX \
  --uri https://example.com/metadata.json \
  --fee-mode holders \
  --dev-wallet <PUBKEY>
```

`--quote sol` uses wrapped SOL. `--quote usdc` uses mainnet USDC, or devnet USDC `4zMMC9srt5Ri5X14GAgXhaHii3GnPAEERYPJgZJDncDU` when `--cluster devnet`. Pump’s fee program treats only mainnet USDC as the stable quote, so a devnet USDC curve uses the flat fee path. `--quote-mint` overrides the mint. Its owner must be a token program Pump will accept, and the program still rejects any mint that is not wrapped SOL or Pump’s current whitelisted quote.

`--fee-mode dev` later forwards creator fees to `--dev-wallet`. `--fee-mode holders` keeps them in the program for a holder crank. `--dev-wallet` defaults to the signer.

The command prints the presale address, the future mint address, and the nonce.

### Deposit and withdraw

```sh
npm run keeper -- deposit --presale <PRESALE> --amount 1000000000
npm run keeper -- withdraw --presale <PRESALE>
npm run keeper -- withdraw --presale <PRESALE> --amount 500000000
```

Deposit adds quote while the raise is open. SOL is native lamports on the buyer PDA. USDC moves into the presale quote token account, which the keeper creates if it is missing.

Withdraw returns some or all of this wallet’s deposit, and only while the raise is open. Omit `--amount` to return the whole position. A full withdrawal closes the position and returns the 0.003 SOL escrow. A partial withdrawal leaves the position open.

### Fill and migrate

```sh
npm run keeper -- fill --presale <PRESALE>
```

Anyone can run this once `raised == target` and Pump’s curve inputs and fees still match the snapshot. The command creates an address lookup table, waits until the next slot, and sends two versioned transactions with a 1,400,000 compute-unit limit. The signer is the only signer.

The fill transaction:

1. Transfers a 0.05 SOL buffer onto the buyer PDA so Pump can pay buyer-side rent without spending the raise.
2. Creates the coin. The payer is the cranker. The creator is the presale PDA.
3. Buys the curve for exactly the raised quote, with `min_tokens_out` set to the snapshotted real token reserve.
4. Requires the curve to be complete with zero real token reserves.

The next transaction migrates. If the canonical PumpSwap pool already exists, the program opens claims without calling Pump migrate again.

Depositors do not receive tokens here. The signer pays the buffer, Pump’s pool migration fee (about 0.015 SOL), lookup-table rent, fee-recipient token-account rent, and the transaction fees. Budget about 0.1 SOL per fill, on top of the raise, which is spent on the buy.

If fill fails because Pump changed its parameters, the raise stays open and withdrawals still work.

### Watch the chain

```sh
npm run keeper -- run --cluster mainnet --url <RPC> --keypair <PATH> --interval 20000
```

`--interval` is the pause between passes, in milliseconds. The default is 20000. The minimum is 1000.

Each pass loads every presale and every position. A full raise is filled and migrated. A raise already in Bought is migrated. After migration, each open position is paid its tokens. One failure is logged and the loop continues.

### Claim tokens

```sh
npm run keeper -- claim-tokens --presale <PRESALE> --owner <DEPOSITOR>
```

`--owner` defaults to the signer. This is the one-shot form of the payout `run` sends. It is available after status is Migrated. The payer creates the depositor’s token account if needed. A zero payout is an error.

### Fees

```sh
npm run keeper -- crank-fees --presale <PRESALE>
npm run keeper -- claim-holder-fees --presale <PRESALE> --owner <HOLDER>
```

`crank-fees` collects Pump curve fees and PumpSwap coin-creator fees. In creator mode it pays 70% of that quote to the dev wallet and 30% to the platform admin. In holder mode it folds the whole amount into the holder index.

`claim-holder-fees` pays one holder from that index, using their current token balance. `run` does not send either command.

### Status

```sh
npm run keeper -- status --presale <PRESALE>
```

Prints the status, fee mode, target, amount raised, tokens received, and dev wallet.

## Hosting

Run one keeper. A second process is safe — the instructions are permissionless and a raise can only be filled once — but it spends extra SOL creating lookup tables and racing the same claims.

Use a keypair that exists only for this process. Do not use the program upgrade authority. The keeper key can pay fees. It cannot change the program.

### Machine

A small VPS is enough. The process is one Node loop: two `getProgramAccounts` calls per pass, then a transaction only when a raise needs fill, migrate, or a token claim. Give it a couple of gigabytes of disk for `node_modules` and logs.

1. Install Node.js 22+.
2. Clone this repository and run `npm install` inside `keeper/`.
3. Create a system user, for example `keeper`, that owns the checkout and the key file.
4. Write a new keypair somewhere outside the repository, mode `600`, owned by that user:

   ```sh
   sudo -u keeper solana-keygen new --outfile /var/lib/presale-keeper/id.json --no-bip39-passphrase
   ```

5. Send the keeper SOL. A quiet keeper spends only the rent of its own account. Each fill needs about 0.1 SOL. Each token claim spends a little rent up front and receives up to 0.003 SOL back from the depositor’s escrow. Keep a buffer of a few SOL so a busy day cannot stall migrate.
6. Point `--url` at an RPC that allows `getProgramAccounts`. The public endpoint `https://api.mainnet-beta.solana.com` is the default, and it will rate-limit this loop. Pass the paid URL explicitly. `--cluster mainnet` must stay set so USDC and the program id resolve to mainnet.

### systemd

`/etc/systemd/system/presale-keeper.service`:

```ini
[Unit]
Description=Presale keeper
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=keeper
Group=keeper
WorkingDirectory=/opt/presale/keeper
ExecStart=/usr/bin/npm run keeper -- run --cluster mainnet --url https://YOUR_RPC --keypair /var/lib/presale-keeper/id.json --interval 20000
Restart=on-failure
RestartSec=5
NoNewPrivileges=true

[Install]
WantedBy=multi-user.target
```

Replace `/opt/presale` with the checkout path and `https://YOUR_RPC` with the provider URL. `npm` must be the Node 22 binary on that machine (`which npm`).

```sh
sudo systemctl daemon-reload
sudo systemctl enable --now presale-keeper
sudo journalctl -u presale-keeper -f
```

A healthy process prints `watching <rpc> every 20000ms as <pubkey>` and then one line per fill, migrate, or claim. A logged error on one raise does not stop the others. `Restart=on-failure` brings the process back after a crash. Stopping the service leaves every raise on-chain; start it again, or start another keeper, and the same work continues.

Check a raise without waiting for the loop:

```sh
sudo -u keeper npm run keeper -- status --presale <PRESALE> --cluster mainnet --url https://YOUR_RPC --keypair /var/lib/presale-keeper/id.json
```

Run that from `keeper/`, or set `WorkingDirectory` the same way the service does.

### What this host does not do

- It does not collect creator fees or pay holder-fee claims. Run `crank-fees` and `claim-holder-fees` on a schedule if you want those moved without waiting for someone else.
- It does not index trades, serve the website, or pin metadata.
- It does not need the upgrade authority, and it should not have it.

## Local testing

```sh
npm run keeper -- run --cluster localnet --interval 15000
```

`localnet` uses `http://127.0.0.1:8899`.

`npm run fill-curve` is a local helper that deposits into one open SOL raise from fresh wallets until the raise is full. It does not buy the Pump curve. The keeper’s `run` command does that next. Leave `fill-curve` on localnet.
