import { parseArgs } from "node:util";
import { BN } from "@coral-xyz/anchor";
import { Connection, PublicKey } from "@solana/web3.js";
import {
  DEVNET_USDC_MINT,
  TOKEN_PROGRAM_ID,
  USDC_MINT,
  WSOL_MINT,
} from "./pump.js";
import {
  claimHolderFees,
  claimTokens,
  clusterUrl,
  crankFees,
  createPresale,
  defaultKeypairPath,
  deposit,
  fetchPresale,
  fill,
  isSolQuote,
  loadKeypair,
  settle,
  mintPda,
  openProgram,
  textField,
  withdraw,
} from "./client.js";

const USAGE = `Usage: npm run keeper -- <command> [options]

Commands:
  create              Open a presale. Prints the presale and the future mint.
  deposit             Add quote. SOL is lamports; USDC is base units. Capped at the room left.
  withdraw            Return some or all of this wallet's deposit. --amount is base units; omit it to return the whole position. Only while the raise is open.
  fill                Create the Pump coin, buy the curve, and migrate. Uses an address lookup table.
  run                 Watch every raise: fill a full raise, migrate one that is bought, then pay each depositor.
  claim-tokens        Pay a depositor their tokens. Anyone can run this after migration.
  crank-fees          Claim Pump creator fees and forward them (dev) or index them (holders).
  claim-holder-fees   Pay a holder their indexed quote fees.
  status              Print the presale account.

Common options:
  --cluster mainnet|devnet|localnet   default mainnet
  --url <rpc>                         overrides the cluster URL
  --keypair <path>                    default ~/.config/solana/id.json
  --interval <ms>                     run loop delay, default 20000
`;

function optionValue(value: string | boolean | undefined, name: string): string {
  if (typeof value !== "string" || value.length === 0) {
    throw new Error(`missing --${name}`);
  }
  return value;
}

function optional(value: string | boolean | undefined): string | undefined {
  return typeof value === "string" && value.length > 0 ? value : undefined;
}

async function main() {
  const [command, ...argv] = process.argv.slice(2);
  if (!command || command === "help" || command === "--help") {
    console.log(USAGE);
    return;
  }

  const { values } = parseArgs({
    args: argv,
    options: {
      cluster: { type: "string", default: "mainnet" },
      url: { type: "string" },
      keypair: { type: "string" },
      name: { type: "string" },
      symbol: { type: "string" },
      uri: { type: "string" },
      quote: { type: "string" },
      "quote-mint": { type: "string" },
      "fee-mode": { type: "string", default: "holders" },
      "dev-wallet": { type: "string" },
      nonce: { type: "string" },
      presale: { type: "string" },
      amount: { type: "string" },
      owner: { type: "string" },
      interval: { type: "string" },
    },
    strict: true,
  });

  const cluster = optionValue(values.cluster, "cluster");
  const connection = new Connection(optional(values.url) ?? clusterUrl(cluster), "confirmed");
  const payer = loadKeypair(optional(values.keypair) ?? defaultKeypairPath());
  const program = openProgram(connection, payer);

  if (command === "run") {
    const interval = Number(optional(values.interval) ?? "20000");
    if (!Number.isFinite(interval) || interval < 1000) {
      throw new Error("--interval is milliseconds and must be at least 1000");
    }
    console.log(`watching ${connection.rpcEndpoint} every ${interval}ms as ${payer.publicKey.toBase58()}`);
    for (;;) {
      await settle({ program, connection, payer });
      await new Promise((resolve) => setTimeout(resolve, interval));
    }
  }

  if (command === "create") {
    const quote = optionValue(values.quote, "quote");
    const quoteMintOverride = optional(values["quote-mint"]);
    let quoteMint: PublicKey;
    let quoteTokenProgram = TOKEN_PROGRAM_ID;
    if (quoteMintOverride) {
      quoteMint = new PublicKey(quoteMintOverride);
      const info = await connection.getAccountInfo(quoteMint);
      if (!info) {
        throw new Error("quote mint account does not exist");
      }
      quoteTokenProgram = info.owner;
    } else if (quote === "sol") {
      quoteMint = WSOL_MINT;
    } else if (quote === "usdc") {
      quoteMint = cluster === "devnet" ? DEVNET_USDC_MINT : USDC_MINT;
    } else {
      throw new Error("--quote must be sol or usdc");
    }
    const feeMode = optionValue(values["fee-mode"], "fee-mode");
    if (feeMode !== "dev" && feeMode !== "holders") {
      throw new Error("--fee-mode must be dev or holders");
    }
    const nonce = new BN(optional(values.nonce) ?? Date.now().toString());
    const devWallet = new PublicKey(optional(values["dev-wallet"]) ?? payer.publicKey.toBase58());
    const result = await createPresale({
      program,
      connection,
      payer,
      name: optionValue(values.name, "name"),
      symbol: optionValue(values.symbol, "symbol"),
      uri: optionValue(values.uri, "uri"),
      quoteMint,
      quoteTokenProgram,
      feeMode,
      nonce,
      devWallet,
    });
    console.log(`signature ${result.signature}`);
    console.log(`presale ${result.presale.toBase58()}`);
    console.log(`mint ${result.mint.toBase58()}`);
    console.log(`nonce ${nonce.toString()}`);
    return;
  }

  const presaleKey = new PublicKey(optionValue(values.presale, "presale"));

  if (command === "status") {
    const state = await fetchPresale(program, presaleKey);
    const mode = state.feeMode.holders ? "holders" : "dev";
    const status = state.status.migrated ? "migrated" : state.status.bought ? "bought" : "raising";
    console.log(`presale ${presaleKey.toBase58()}`);
    console.log(`mint ${mintPda(program.programId, presaleKey).toBase58()}`);
    console.log(`status ${status}`);
    console.log(`fee mode ${mode}`);
    console.log(`name ${textField(state.nameLen, state.name)}`);
    console.log(`symbol ${textField(state.symbolLen, state.symbol)}`);
    console.log(`quote mint ${state.quoteMint.toBase58()}${isSolQuote(state.quoteMint) ? " (SOL)" : ""}`);
    console.log(`target ${state.quoteTarget.toString()}`);
    console.log(`raised ${state.quoteRaised.toString()}`);
    console.log(`spent ${state.quoteSpent.toString()}`);
    console.log(`tokens received ${state.tokensReceived.toString()}`);
    console.log(`dev wallet ${state.devWallet.toBase58()}`);
    console.log(`launcher ${state.launcher.toBase58()}`);
    console.log(`undistributed fees ${state.undistributedFees.toString()}`);
    console.log(`acc quote per token ${state.accQuotePerToken.toString()}`);
    return;
  }

  if (command === "deposit") {
    const signature = await deposit({
      program,
      connection,
      payer,
      presale: presaleKey,
      amount: new BN(optionValue(values.amount, "amount")),
    });
    console.log(`signature ${signature}`);
    return;
  }

  if (command === "withdraw") {
    const raw = optional(values.amount);
    const signature = await withdraw({
      program,
      connection,
      payer,
      presale: presaleKey,
      amount: raw ? new BN(raw) : undefined,
    });
    console.log(`signature ${signature}`);
    return;
  }

  if (command === "fill") {
    const signature = await fill({ program, connection, payer, presale: presaleKey });
    console.log(`signature ${signature}`);
    console.log(`mint ${mintPda(program.programId, presaleKey).toBase58()}`);
    return;
  }

  if (command === "claim-tokens") {
    const owner = new PublicKey(optional(values.owner) ?? payer.publicKey.toBase58());
    const signature = await claimTokens({ program, connection, payer, presale: presaleKey, owner });
    console.log(`signature ${signature}`);
    return;
  }

  if (command === "crank-fees") {
    const signature = await crankFees({ program, connection, payer, presale: presaleKey });
    console.log(`signature ${signature}`);
    return;
  }

  if (command === "claim-holder-fees") {
    const signature = await claimHolderFees({
      program,
      connection,
      payer,
      presale: presaleKey,
      owner: new PublicKey(optionValue(values.owner, "owner")),
    });
    console.log(`signature ${signature}`);
    return;
  }

  throw new Error(`unknown command ${command}\n\n${USAGE}`);
}

main().catch((error: unknown) => {
  const message = error instanceof Error ? error.message : String(error);
  console.error(message);
  const logs = (error as { logs?: string[] }).logs;
  if (logs) {
    console.error(logs.join("\n"));
  }
  process.exit(1);
});
