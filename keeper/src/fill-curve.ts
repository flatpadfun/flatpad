import { randomInt } from "node:crypto";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { parseArgs } from "node:util";
import { BN } from "@coral-xyz/anchor";
import {
  Connection,
  Keypair,
  PublicKey,
  SystemProgram,
  Transaction,
  sendAndConfirmTransaction,
} from "@solana/web3.js";
import {
  clusterUrl,
  defaultKeypairPath,
  deposit,
  fetchPresale,
  isSolQuote,
  loadKeypair,
  openProgram,
} from "./client.js";

const LAMPORTS = 1_000_000_000n;
const MIN_DEFAULT = 100_000_000n;
const MAX_DEFAULT = 2_500_000_000n;
const CUSHION = 10_000_000n;

const USAGE = `Usage: npm run fill-curve -- [options]

Deposit into one open SOL raise until it is full. Each deposit is a new wallet,
between --min and --max SOL. The last deposit takes whatever room is left.

This does not buy the Pump curve and does not send tokens. After it finishes, run:

  npm run keeper -- run --cluster localnet --keypair <funder>

Options:
  --presale <address>                 required
  --cluster mainnet|devnet|localnet   default localnet
  --url <rpc>
  --keypair <path>                    funder, default ~/.config/solana/id.json
  --min <sol>                         default 0.1
  --max <sol>                         default 2.5
`;

interface BuyRecord {
  pubkey: string;
  secret: number[];
  lamports: string;
  signature: string;
}

function optionValue(value: string | boolean | undefined, name: string): string {
  if (typeof value !== "string" || value.length === 0) {
    throw new Error(`missing --${name}`);
  }
  return value;
}

function optional(value: string | boolean | undefined): string | undefined {
  return typeof value === "string" && value.length > 0 ? value : undefined;
}

function parseSol(value: string, name: string): bigint {
  if (!/^\d+(\.\d{1,9})?$/.test(value)) {
    throw new Error(`--${name} must be SOL, up to 9 decimals`);
  }
  const [whole, frac = ""] = value.split(".");
  return BigInt(whole + frac.padEnd(9, "0"));
}

function formatSol(lamports: bigint): string {
  const sign = lamports < 0n ? "-" : "";
  const abs = lamports < 0n ? -lamports : lamports;
  const whole = abs / LAMPORTS;
  const frac = (abs % LAMPORTS).toString().padStart(9, "0").replace(/0+$/, "");
  return frac.length > 0 ? `${sign}${whole}.${frac}` : `${sign}${whole}`;
}

function chooseAmount(room: bigint, min: bigint, max: bigint): bigint {
  if (room <= min) return room;
  const cap = room < max ? room : max;
  return BigInt(randomInt(Number(min), Number(cap) + 1));
}

function bookPath(presale: PublicKey): string {
  return new URL(`../fills/${presale.toBase58()}.json`, import.meta.url).pathname;
}

function loadBook(path: string): BuyRecord[] {
  try {
    return JSON.parse(readFileSync(path, "utf8")) as BuyRecord[];
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === "ENOENT") return [];
    throw error;
  }
}

async function main() {
  const { values } = parseArgs({
    options: {
      presale: { type: "string" },
      cluster: { type: "string", default: "localnet" },
      url: { type: "string" },
      keypair: { type: "string" },
      min: { type: "string", default: "0.1" },
      max: { type: "string", default: "2.5" },
      help: { type: "boolean" },
    },
    strict: true,
  });
  if (values.help) {
    console.log(USAGE);
    return;
  }

  const min = parseSol(optionValue(values.min, "min"), "min");
  const max = parseSol(optionValue(values.max, "max"), "max");
  if (min <= 0n || max < min) {
    throw new Error("--min and --max must be positive, and --max must be at least --min");
  }
  if (min !== MIN_DEFAULT || max !== MAX_DEFAULT) {
    console.log(`amounts ${formatSol(min)} to ${formatSol(max)} SOL`);
  }

  const presale = new PublicKey(optionValue(values.presale, "presale"));
  const cluster = optionValue(values.cluster, "cluster");
  const connection = new Connection(optional(values.url) ?? clusterUrl(cluster), "confirmed");
  const funder = loadKeypair(optional(values.keypair) ?? defaultKeypairPath());
  const program = openProgram(connection, funder);
  const path = bookPath(presale);
  const buys = loadBook(path);
  mkdirSync(new URL("../fills/", import.meta.url), { recursive: true });

  const state = await fetchPresale(program, presale);
  if (!isSolQuote(state.quoteMint)) {
    throw new Error("this script funds SOL deposits only");
  }
  if (!state.status.raising) {
    throw new Error("the raise is no longer open");
  }

  let raised = BigInt(state.quoteRaised.toString());
  const target = BigInt(state.quoteTarget.toString());
  console.log(
    `filling ${presale.toBase58()} from ${formatSol(raised)} / ${formatSol(target)} SOL with ${funder.publicKey.toBase58()}`,
  );

  let index = buys.length;
  while (raised < target) {
    const room = target - raised;
    const amount = chooseAmount(room, min, max);
    const buyer = Keypair.generate();
    const fund = new Transaction().add(
      SystemProgram.transfer({
        fromPubkey: funder.publicKey,
        toPubkey: buyer.publicKey,
        lamports: Number(amount + CUSHION),
      }),
    );
    await sendAndConfirmTransaction(connection, fund, [funder], { commitment: "confirmed" });
    const signature = await deposit({
      program,
      connection,
      payer: buyer,
      presale,
      amount: new BN(amount.toString()),
    });
    raised += amount;
    index += 1;
    const record: BuyRecord = {
      pubkey: buyer.publicKey.toBase58(),
      secret: Array.from(buyer.secretKey),
      lamports: amount.toString(),
      signature,
    };
    buys.push(record);
    writeFileSync(path, JSON.stringify(buys, null, 2));
    const closing = amount < min ? " closing" : "";
    console.log(
      `${index} ${record.pubkey} ${formatSol(amount)} SOL${closing}  ${formatSol(raised)} / ${formatSol(target)}  ${signature}`,
    );
  }

  console.log(`${buys.length} wallets deposited. The raise is full.`);
  console.log("Start the keeper to buy the curve, migrate, and send the tokens:");
  console.log(`npm run keeper -- run --cluster ${cluster} --keypair ${optional(values.keypair) ?? defaultKeypairPath()}`);
}

main().catch((error: unknown) => {
  console.error(error instanceof Error ? error.message : error);
  process.exit(1);
});
