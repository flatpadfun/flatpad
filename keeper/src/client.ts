import { readFileSync } from "node:fs";
import { homedir } from "node:os";
import { AnchorProvider, BN, Program, type Idl, Wallet } from "@coral-xyz/anchor";
import {
  ASSOCIATED_TOKEN_PROGRAM_ID,
  createAssociatedTokenAccountIdempotentInstruction,
} from "@solana/spl-token";
import {
  AddressLookupTableProgram,
  ComputeBudgetProgram,
  Connection,
  Keypair,
  PublicKey,
  SystemProgram,
  TransactionInstruction,
  TransactionMessage,
  VersionedTransaction,
} from "@solana/web3.js";
import {
  PUMP_PROGRAM_ID,
  TOKEN_2022_PROGRAM_ID,
  WSOL_MINT,
  ata,
  CRANK_WRITABLE,
  FILL_WRITABLE,
  crankRemainingAccounts,
  decodeGlobalAccounts,
  fillRemainingAccounts,
  pda,
} from "./pump.js";

const IDL_URL = new URL("../../idl/presale.json", import.meta.url);
/** Matches `PLATFORM_ADMIN` in the presale program. */
const PLATFORM_ADMIN = new PublicKey("JA9ijmzmtGcZCHNxw12g4kqJFNtTiGZs8MkQr4ddcTvW");

export interface PresaleAccount {
  launcher: PublicKey;
  devWallet: PublicKey;
  quoteMint: PublicKey;
  quoteTokenProgram: PublicKey;
  feeMode: { dev?: Record<string, never>; holders?: Record<string, never> };
  status: { raising?: Record<string, never>; migrated?: Record<string, never>; bought?: Record<string, never> };
  nonce: BN;
  nameLen: number;
  name: number[];
  symbolLen: number;
  symbol: number[];
  uriLen: number;
  uri: number[];
  quoteTarget: BN;
  quoteRaised: BN;
  tokensReceived: BN;
  quoteSpent: BN;
  protocolFeeBps: BN;
  creatorFeeBps: BN;
  accQuotePerToken: BN;
  undistributedFees: BN;
  bump: number;
  mintBump: number;
}

export function loadIdl(): Idl {
  return JSON.parse(readFileSync(IDL_URL, "utf8")) as Idl;
}

export function loadKeypair(path: string): Keypair {
  const secret = JSON.parse(readFileSync(path, "utf8")) as number[];
  return Keypair.fromSecretKey(Uint8Array.from(secret));
}

export function defaultKeypairPath(): string {
  return `${homedir()}/.config/solana/id.json`;
}

export function clusterUrl(cluster: string): string {
  switch (cluster) {
    case "mainnet":
      return "https://api.mainnet-beta.solana.com";
    case "devnet":
      return "https://api.devnet.solana.com";
    case "localnet":
      return "http://127.0.0.1:8899";
    default:
      throw new Error(`unknown cluster ${cluster}; use mainnet, devnet, or localnet`);
  }
}

export function presalePda(programId: PublicKey, launcher: PublicKey, quoteMint: PublicKey, nonce: BN): PublicKey {
  return pda(programId, [
    Buffer.from("presale"),
    launcher.toBuffer(),
    quoteMint.toBuffer(),
    nonce.toArrayLike(Buffer, "le", 8),
  ]);
}

export function mintPda(programId: PublicKey, presale: PublicKey): PublicKey {
  return pda(programId, [Buffer.from("mint"), presale.toBuffer()]);
}

export function positionPda(programId: PublicKey, presale: PublicKey, owner: PublicKey): PublicKey {
  return pda(programId, [Buffer.from("position"), presale.toBuffer(), owner.toBuffer()]);
}

export function holderRewardPda(programId: PublicKey, presale: PublicKey, owner: PublicKey): PublicKey {
  return pda(programId, [Buffer.from("holder-reward"), presale.toBuffer(), owner.toBuffer()]);
}

export function openProgram(connection: Connection, payer: Keypair): Program {
  const provider = new AnchorProvider(connection, new Wallet(payer), { commitment: "confirmed" });
  return new Program(loadIdl(), provider);
}

export async function fetchPresale(program: Program, address: PublicKey): Promise<PresaleAccount> {
  const accounts = program.account as unknown as {
    presale: { fetch: (key: PublicKey) => Promise<PresaleAccount> };
  };
  return accounts.presale.fetch(address);
}

export function textField(len: number, bytes: number[]): string {
  return Buffer.from(bytes.slice(0, len)).toString("utf8");
}

export function isSolQuote(mint: PublicKey): boolean {
  return mint.equals(WSOL_MINT);
}

function quoteAccounts(presale: PresaleAccount, presaleKey: PublicKey, user: PublicKey) {
  if (isSolQuote(presale.quoteMint)) {
    return {
      tokenProgram: null,
      quoteMint: null,
      userQuoteAta: null,
      presaleQuoteAta: null,
    };
  }
  return {
    tokenProgram: presale.quoteTokenProgram,
    quoteMint: presale.quoteMint,
    userQuoteAta: ata(user, presale.quoteMint, presale.quoteTokenProgram),
    presaleQuoteAta: ata(presaleKey, presale.quoteMint, presale.quoteTokenProgram),
  };
}

async function send(
  connection: Connection,
  payer: Keypair,
  instructions: TransactionInstruction[],
  lookupTables: Awaited<ReturnType<Connection["getAddressLookupTable"]>>["value"][] = [],
): Promise<string> {
  const latest = await connection.getLatestBlockhash("confirmed");
  const message = new TransactionMessage({
    payerKey: payer.publicKey,
    recentBlockhash: latest.blockhash,
    instructions: [ComputeBudgetProgram.setComputeUnitLimit({ units: 1_400_000 }), ...instructions],
  }).compileToV0Message(lookupTables.filter((table): table is NonNullable<typeof table> => table !== null));
  const tx = new VersionedTransaction(message);
  tx.sign([payer]);
  let signature: string;
  try {
    signature = await connection.sendTransaction(tx, { skipPreflight: false });
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    if (!message.includes("Max instruction trace length exceeded")) {
      throw error;
    }
    signature = await connection.sendTransaction(tx, { skipPreflight: true });
  }
  const confirmed = await connection.confirmTransaction(
    { signature, blockhash: latest.blockhash, lastValidBlockHeight: latest.lastValidBlockHeight },
    "confirmed",
  );
  if (confirmed.value.err) {
    throw new Error(`Transaction ${signature} failed: ${JSON.stringify(confirmed.value.err)}`);
  }
  return signature;
}

export async function createPresale(input: {
  program: Program;
  connection: Connection;
  payer: Keypair;
  name: string;
  symbol: string;
  uri: string;
  quoteMint: PublicKey;
  quoteTokenProgram: PublicKey;
  feeMode: "dev" | "holders";
  nonce: BN;
  devWallet: PublicKey;
}): Promise<{ signature: string; presale: PublicKey; mint: PublicKey }> {
  const presale = presalePda(input.program.programId, input.payer.publicKey, input.quoteMint, input.nonce);
  const mint = mintPda(input.program.programId, presale);
  const global = pda(PUMP_PROGRAM_ID, [Buffer.from("global")]);
  const feeConfig = pda(
    new PublicKey("pfeeUxB6jkeY1Hxd7CsFCAjcbHA9rWtchMGdZ6VojVZ"),
    [Buffer.from("fee_config"), PUMP_PROGRAM_ID.toBuffer()],
  );
  const ix = await input.program.methods
    .createPresale({
      name: input.name,
      symbol: input.symbol,
      uri: input.uri,
      feeMode: input.feeMode === "dev" ? { dev: {} } : { holders: {} },
      nonce: input.nonce,
      devWallet: input.devWallet,
    })
    .accounts({
      launcher: input.payer.publicKey,
      presale,
      quoteMint: input.quoteMint,
      quoteTokenProgram: input.quoteTokenProgram,
      global,
      feeConfig,
      systemProgram: SystemProgram.programId,
    })
    .instruction();
  const signature = await send(input.connection, input.payer, [ix]);
  return { signature, presale, mint };
}

export async function deposit(input: {
  program: Program;
  connection: Connection;
  payer: Keypair;
  presale: PublicKey;
  amount: BN;
}): Promise<string> {
  const state = await fetchPresale(input.program, input.presale);
  const quotes = quoteAccounts(state, input.presale, input.payer.publicKey);
  const setup: TransactionInstruction[] = [];
  if (quotes.presaleQuoteAta && quotes.quoteMint && quotes.tokenProgram) {
    setup.push(
      createAssociatedTokenAccountIdempotentInstruction(
        input.payer.publicKey,
        quotes.presaleQuoteAta,
        input.presale,
        quotes.quoteMint,
        quotes.tokenProgram,
      ),
    );
  }
  const ix = await input.program.methods
    .deposit(input.amount)
    .accounts({
      user: input.payer.publicKey,
      presale: input.presale,
      position: positionPda(input.program.programId, input.presale, input.payer.publicKey),
      systemProgram: SystemProgram.programId,
      buyer: pda(input.program.programId, [Buffer.from("buyer"), input.presale.toBuffer()]),
      ...quotes,
    } as never)
    .instruction();
  return send(input.connection, input.payer, [...setup, ix]);
}

export async function withdraw(input: {
  program: Program;
  connection: Connection;
  payer: Keypair;
  presale: PublicKey;
  amount?: BN;
}): Promise<string> {
  const state = await fetchPresale(input.program, input.presale);
  const position = positionPda(input.program.programId, input.presale, input.payer.publicKey);
  let amount = input.amount;
  if (!amount) {
    const accounts = input.program.account as unknown as {
      position: { fetch: (key: PublicKey) => Promise<{ quoteAmount: BN }> };
    };
    amount = (await accounts.position.fetch(position)).quoteAmount;
  }
  const ix = await input.program.methods
    .withdraw(amount)
    .accounts({
      user: input.payer.publicKey,
      presale: input.presale,
      position,
      systemProgram: SystemProgram.programId,
      buyer: pda(input.program.programId, [Buffer.from("buyer"), input.presale.toBuffer()]),
      ...quoteAccounts(state, input.presale, input.payer.publicKey),
    } as never)
    .instruction();
  return send(input.connection, input.payer, [ix]);
}

async function waitUntilSlot(connection: Connection, after: number) {
  for (;;) {
    const slot = await connection.getSlot("processed");
    if (slot > after) {
      return;
    }
    await new Promise((resolve) => setTimeout(resolve, 400));
  }
}

export async function fill(input: {
  program: Program;
  connection: Connection;
  payer: Keypair;
  presale: PublicKey;
}): Promise<string> {
  const state = await fetchPresale(input.program, input.presale);
  const mint = mintPda(input.program.programId, input.presale);
  const globalKey = pda(PUMP_PROGRAM_ID, [Buffer.from("global")]);
  const globalInfo = await input.connection.getAccountInfo(globalKey);
  if (!globalInfo) {
    throw new Error("Pump global account is missing on this cluster");
  }
  const global = decodeGlobalAccounts(globalInfo.data);
  const remaining = fillRemainingAccounts({
    programId: input.program.programId,
    presale: input.presale,
    mint,
    quoteMint: state.quoteMint,
    quoteTokenProgram: state.quoteTokenProgram,
    global,
  });

  const slot = await input.connection.getSlot("finalized");
  const [createIx, lookupTable] = AddressLookupTableProgram.createLookupTable({
    authority: input.payer.publicKey,
    payer: input.payer.publicKey,
    recentSlot: slot,
  });
  const extendIxs: TransactionInstruction[] = [];
  const addresses = [...new Set([input.presale.toBase58(), SystemProgram.programId.toBase58(), ...remaining.map((key) => key.toBase58())])].map(
    (value) => new PublicKey(value),
  );
  for (let offset = 0; offset < addresses.length; offset += 20) {
    extendIxs.push(
      AddressLookupTableProgram.extendLookupTable({
        payer: input.payer.publicKey,
        authority: input.payer.publicKey,
        lookupTable,
        addresses: addresses.slice(offset, offset + 20),
      }),
    );
  }
  const createSig = await send(input.connection, input.payer, [createIx]);
  const extendSigs: string[] = [];
  for (const extendIx of extendIxs) {
    extendSigs.push(await send(input.connection, input.payer, [extendIx]));
  }
  console.log(`lookup table ${lookupTable.toBase58()} created in ${createSig} and extended in ${extendSigs.join(", ")}`);
  const writtenSlot = await input.connection.getSlot("confirmed");
  await waitUntilSlot(input.connection, writtenSlot);

  const table = await input.connection.getAddressLookupTable(lookupTable);
  if (!table.value) {
    throw new Error("address lookup table was not found after creation");
  }
  const buyer = remaining[44];
  const prepare = [global.feeRecipient, global.buybackFeeRecipient, buyer].map((owner) =>
    createAssociatedTokenAccountIdempotentInstruction(
      input.payer.publicKey,
      ata(owner, state.quoteMint, state.quoteTokenProgram),
      owner,
      state.quoteMint,
      state.quoteTokenProgram,
    ),
  );
  const prepareSig = await send(input.connection, input.payer, prepare);
  console.log(`quote accounts prepared in ${prepareSig}`);
  const metas = remaining.map((pubkey, index) => ({
    pubkey,
    isSigner: false,
    isWritable: FILL_WRITABLE[index] ?? false,
  }));
  if (state.status.raising) {
    const ix = await input.program.methods
      .fill()
      .accounts({
        cranker: input.payer.publicKey,
        presale: input.presale,
        systemProgram: SystemProgram.programId,
      })
      .remainingAccounts(metas)
      .instruction();
    const fillSig = await send(input.connection, input.payer, [ix], [table.value]);
    console.log(`curve bought in ${fillSig}`);
  } else if (!state.status.bought) {
    throw new Error("presale is already migrated");
  }
  const migrateIx = await input.program.methods
    .migrate()
    .accounts({
      cranker: input.payer.publicKey,
      presale: input.presale,
      systemProgram: SystemProgram.programId,
    })
    .remainingAccounts(
      remaining.map((pubkey, index) => ({
        pubkey,
        isSigner: false,
        isWritable: FILL_WRITABLE[index] ?? false,
      })),
    )
    .instruction();
  return send(input.connection, input.payer, [migrateIx], [table.value]);
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

export async function settle(input: { program: Program; connection: Connection; payer: Keypair }): Promise<void> {
  const accounts = input.program.account as unknown as {
    presale: { all: () => Promise<{ publicKey: PublicKey; account: PresaleAccount }[]> };
    position: {
      all: () => Promise<{ publicKey: PublicKey; account: { presale: PublicKey; owner: PublicKey; quoteAmount: BN } }[]>;
    };
  };
  const [presales, positions] = await Promise.all([accounts.presale.all(), accounts.position.all()]);
  for (const row of presales) {
    const state = row.account;
    const address = row.publicKey;
    const label = address.toBase58();
    try {
      const full = !state.quoteTarget.isZero() && state.quoteRaised.gte(state.quoteTarget);
      if ((state.status.raising && full) || state.status.bought) {
        console.log(state.status.bought ? `migrating ${label}` : `filling ${label}`);
        const signature = await fill({ ...input, presale: address });
        console.log(`migrated ${label} in ${signature}`);
        continue;
      }
      if (!state.status.migrated) continue;
      const open = positions.filter(
        (position) => position.account.presale.equals(address) && position.account.quoteAmount.gt(new BN(0)),
      );
      for (const position of open) {
        const owner = position.account.owner.toBase58();
        try {
          const signature = await claimTokens({ ...input, presale: address, owner: position.account.owner });
          console.log(`claimed ${owner} on ${label} in ${signature}`);
        } catch (error) {
          console.error(`claim ${owner} on ${label}: ${errorMessage(error)}`);
        }
      }
    } catch (error) {
      console.error(`${label}: ${errorMessage(error)}`);
    }
  }
}

export async function claimTokens(input: {
  program: Program;
  connection: Connection;
  payer: Keypair;
  presale: PublicKey;
  owner: PublicKey;
}): Promise<string> {
  const mint = mintPda(input.program.programId, input.presale);
  const buyer = pda(input.program.programId, [Buffer.from("buyer"), input.presale.toBuffer()]);
  const ix = await input.program.methods
    .claimTokens()
    .accounts({
      payer: input.payer.publicKey,
      presale: input.presale,
      position: positionPda(input.program.programId, input.presale, input.owner),
      owner: input.owner,
      buyer,
      mint,
      buyerAta: ata(buyer, mint, TOKEN_2022_PROGRAM_ID),
      ownerAta: ata(input.owner, mint, TOKEN_2022_PROGRAM_ID),
      tokenProgram: TOKEN_2022_PROGRAM_ID,
      associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
      systemProgram: SystemProgram.programId,
    })
    .instruction();
  return send(input.connection, input.payer, [ix]);
}

export async function crankFees(input: {
  program: Program;
  connection: Connection;
  payer: Keypair;
  presale: PublicKey;
}): Promise<string> {
  const state = await fetchPresale(input.program, input.presale);
  const mint = mintPda(input.program.programId, input.presale);
  const poolAuthority = pda(PUMP_PROGRAM_ID, [Buffer.from("pool-authority"), mint.toBuffer()]);
  const pool = pda(
    new PublicKey("pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA"),
    [
      Buffer.from("pool"),
      Buffer.from([0, 0]),
      poolAuthority.toBuffer(),
      mint.toBuffer(),
      state.quoteMint.toBuffer(),
    ],
  );
  const remaining = crankRemainingAccounts({
    presale: input.presale,
    quoteMint: state.quoteMint,
    quoteTokenProgram: state.quoteTokenProgram,
    devWallet: state.devWallet,
  });
  const platformQuoteAta = ata(PLATFORM_ADMIN, state.quoteMint, state.quoteTokenProgram);
  const setup: TransactionInstruction[] = [];
  if (!isSolQuote(state.quoteMint)) {
    setup.push(
      createAssociatedTokenAccountIdempotentInstruction(
        input.payer.publicKey,
        ata(state.devWallet, state.quoteMint, state.quoteTokenProgram),
        state.devWallet,
        state.quoteMint,
        state.quoteTokenProgram,
      ),
      createAssociatedTokenAccountIdempotentInstruction(
        input.payer.publicKey,
        platformQuoteAta,
        PLATFORM_ADMIN,
        state.quoteMint,
        state.quoteTokenProgram,
      ),
    );
  }
  const ix = await input.program.methods
    .crankFees()
    .accounts({
      payer: input.payer.publicKey,
      presale: input.presale,
      devWallet: state.devWallet,
      baseMint: mint,
      poolBaseAta: ata(pool, mint, TOKEN_2022_PROGRAM_ID),
      presaleBaseAta: ata(
        pda(input.program.programId, [Buffer.from("buyer"), input.presale.toBuffer()]),
        mint,
        TOKEN_2022_PROGRAM_ID,
      ),
      tally: pda(input.program.programId, [Buffer.from("fee-tally"), input.presale.toBuffer()]),
      platformWallet: PLATFORM_ADMIN,
      platformQuoteAta,
      systemProgram: SystemProgram.programId,
    })
    .remainingAccounts(
      remaining.map((pubkey, index) => ({
        pubkey,
        isSigner: false,
        isWritable: CRANK_WRITABLE[index] ?? false,
      })),
    )
    .instruction();
  return send(input.connection, input.payer, [...setup, ix]);
}

export async function claimHolderFees(input: {
  program: Program;
  connection: Connection;
  payer: Keypair;
  presale: PublicKey;
  owner: PublicKey;
}): Promise<string> {
  const state = await fetchPresale(input.program, input.presale);
  const mint = mintPda(input.program.programId, input.presale);
  const sol = isSolQuote(state.quoteMint);
  const ix = await input.program.methods
    .claimHolderFees()
    .accounts({
      payer: input.payer.publicKey,
      presale: input.presale,
      owner: input.owner,
      reward: holderRewardPda(input.program.programId, input.presale, input.owner),
      mint,
      ownerBaseAta: ata(input.owner, mint, TOKEN_2022_PROGRAM_ID),
      tokenProgram: TOKEN_2022_PROGRAM_ID,
      systemProgram: SystemProgram.programId,
      quoteMint: sol ? null : state.quoteMint,
      quoteTokenProgram: sol ? null : state.quoteTokenProgram,
      presaleQuoteAta: sol ? null : ata(input.presale, state.quoteMint, state.quoteTokenProgram),
      ownerQuoteAta: sol ? null : ata(input.owner, state.quoteMint, state.quoteTokenProgram),
    } as never)
    .instruction();
  return send(input.connection, input.payer, [ix]);
}
