import { PublicKey } from "@solana/web3.js";
import { getAssociatedTokenAddressSync } from "@solana/spl-token";

export const PUMP_PROGRAM_ID = new PublicKey("6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P");
export const PUMP_AMM_PROGRAM_ID = new PublicKey("pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA");
export const PUMP_FEE_PROGRAM_ID = new PublicKey("pfeeUxB6jkeY1Hxd7CsFCAjcbHA9rWtchMGdZ6VojVZ");
export const MAYHEM_PROGRAM_ID = new PublicKey("MAyhSmzXzV1pTf7LsNkrNwkWKTo4ougAJ1PPg47MD4e");
export const TOKEN_PROGRAM_ID = new PublicKey("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA");
export const TOKEN_2022_PROGRAM_ID = new PublicKey("TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb");
export const ASSOCIATED_TOKEN_PROGRAM_ID = new PublicKey("ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL");
export const WSOL_MINT = new PublicKey("So11111111111111111111111111111111111111112");
export const USDC_MINT = new PublicKey("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v");
export const DEVNET_USDC_MINT = new PublicKey("4zMMC9srt5Ri5X14GAgXhaHii3GnPAEERYPJgZJDncDU");

const GLOBAL_DISC = Buffer.from([167, 232, 232, 177, 200, 108, 114, 127]);

export function pda(program: PublicKey, seeds: Array<Buffer | Uint8Array>): PublicKey {
  return PublicKey.findProgramAddressSync(
    seeds.map((seed) => Buffer.from(seed)),
    program,
  )[0];
}

export function ata(owner: PublicKey, mint: PublicKey, tokenProgram: PublicKey): PublicKey {
  return getAssociatedTokenAddressSync(mint, owner, true, tokenProgram);
}

export interface PumpGlobalAccounts {
  feeRecipient: PublicKey;
  withdrawAuthority: PublicKey;
  buybackFeeRecipient: PublicKey;
}

class Reader {
  private pos = 0;

  constructor(private readonly data: Buffer) {}

  skip(len: number) {
    this.pos += len;
  }

  bool(): boolean {
    const value = this.data[this.pos];
    this.pos += 1;
    if (value > 1) {
      throw new Error("Pump global bool is invalid");
    }
    return value === 1;
  }

  u64() {
    const value = this.data.readBigUInt64LE(this.pos);
    this.pos += 8;
    return value;
  }

  pubkey(): PublicKey {
    const key = new PublicKey(this.data.subarray(this.pos, this.pos + 32));
    this.pos += 32;
    return key;
  }
}

/** Same field walk as `quote::decode_global`. Only the accounts fill needs are returned. */
export function decodeGlobalAccounts(data: Buffer): PumpGlobalAccounts {
  if (data.length < 1045 || !data.subarray(0, 8).equals(GLOBAL_DISC)) {
    throw new Error("Pump global account is missing or has an unexpected discriminator");
  }
  const reader = new Reader(data);
  reader.skip(8);
  reader.bool();
  reader.pubkey();
  const feeRecipient = reader.pubkey();
  reader.u64();
  reader.u64();
  reader.u64();
  reader.u64();
  reader.u64();
  const withdrawAuthority = reader.pubkey();
  reader.bool();
  reader.u64();
  reader.u64();
  reader.skip(32 * 7);
  reader.pubkey();
  reader.pubkey();
  reader.bool();
  reader.pubkey();
  reader.pubkey();
  reader.bool();
  reader.skip(32 * 7);
  reader.bool();
  const buybackFeeRecipient = reader.pubkey();
  return { feeRecipient, withdrawAuthority, buybackFeeRecipient };
}

/** Remaining accounts for `fill`, in `fill_idx` order. Length is 47. */
export function fillRemainingAccounts(input: {
  programId: PublicKey;
  presale: PublicKey;
  mint: PublicKey;
  quoteMint: PublicKey;
  quoteTokenProgram: PublicKey;
  global: PumpGlobalAccounts;
}): PublicKey[] {
  const { programId, presale, mint, quoteMint, quoteTokenProgram, global } = input;
  const buyer = pda(programId, [Buffer.from("buyer"), presale.toBuffer()]);
  const mintAuthority = pda(PUMP_PROGRAM_ID, [Buffer.from("mint-authority")]);
  const bondingCurve = pda(PUMP_PROGRAM_ID, [Buffer.from("bonding-curve"), mint.toBuffer()]);
  const pumpGlobal = pda(PUMP_PROGRAM_ID, [Buffer.from("global")]);
  const pumpEventAuthority = pda(PUMP_PROGRAM_ID, [Buffer.from("__event_authority")]);
  const creatorVault = pda(PUMP_PROGRAM_ID, [Buffer.from("creator-vault"), presale.toBuffer()]);
  const globalVolume = pda(PUMP_PROGRAM_ID, [Buffer.from("global_volume_accumulator")]);
  const userVolume = pda(PUMP_PROGRAM_ID, [
    Buffer.from("user_volume_accumulator"),
    buyer.toBuffer(),
  ]);
  const feeConfig = pda(PUMP_FEE_PROGRAM_ID, [
    Buffer.from("fee_config"),
    PUMP_PROGRAM_ID.toBuffer(),
  ]);
  const sharingConfig = pda(PUMP_FEE_PROGRAM_ID, [Buffer.from("sharing-config"), mint.toBuffer()]);
  const poolAuthority = pda(PUMP_PROGRAM_ID, [Buffer.from("pool-authority"), mint.toBuffer()]);
  const pool = pda(PUMP_AMM_PROGRAM_ID, [
    Buffer.from("pool"),
    Buffer.from([0, 0]),
    poolAuthority.toBuffer(),
    mint.toBuffer(),
    quoteMint.toBuffer(),
  ]);
  const lpMint = pda(PUMP_AMM_PROGRAM_ID, [Buffer.from("pool_lp_mint"), pool.toBuffer()]);
  const ammGlobal = pda(PUMP_AMM_PROGRAM_ID, [Buffer.from("global_config")]);
  const ammEvent = pda(PUMP_AMM_PROGRAM_ID, [Buffer.from("__event_authority")]);
  const mayhemParams = pda(MAYHEM_PROGRAM_ID, [Buffer.from("global-params")]);
  const solVault = pda(MAYHEM_PROGRAM_ID, [Buffer.from("sol-vault")]);
  const mayhemState = pda(MAYHEM_PROGRAM_ID, [Buffer.from("mayhem-state"), mint.toBuffer()]);

  const accounts = [
    mint,
    pumpGlobal,
    feeConfig,
    quoteMint,
    quoteTokenProgram,
    TOKEN_2022_PROGRAM_ID,
    ASSOCIATED_TOKEN_PROGRAM_ID,
    PUMP_PROGRAM_ID,
    pumpEventAuthority,
    MAYHEM_PROGRAM_ID,
    mayhemParams,
    solVault,
    mayhemState,
    ata(solVault, mint, TOKEN_2022_PROGRAM_ID),
    mintAuthority,
    bondingCurve,
    ata(bondingCurve, mint, TOKEN_2022_PROGRAM_ID),
    ata(bondingCurve, quoteMint, quoteTokenProgram),
    global.feeRecipient,
    ata(global.feeRecipient, quoteMint, quoteTokenProgram),
    global.buybackFeeRecipient,
    ata(global.buybackFeeRecipient, quoteMint, quoteTokenProgram),
    ata(presale, mint, TOKEN_2022_PROGRAM_ID),
    ata(presale, quoteMint, quoteTokenProgram),
    creatorVault,
    ata(creatorVault, quoteMint, quoteTokenProgram),
    sharingConfig,
    globalVolume,
    userVolume,
    ata(userVolume, quoteMint, quoteTokenProgram),
    PUMP_FEE_PROGRAM_ID,
    global.withdrawAuthority,
    PUMP_AMM_PROGRAM_ID,
    pool,
    poolAuthority,
    ata(poolAuthority, mint, TOKEN_2022_PROGRAM_ID),
    ata(poolAuthority, quoteMint, quoteTokenProgram),
    ammGlobal,
    lpMint,
    ata(poolAuthority, lpMint, TOKEN_2022_PROGRAM_ID),
    ata(pool, mint, TOKEN_2022_PROGRAM_ID),
    ata(pool, quoteMint, quoteTokenProgram),
    ammEvent,
    new PublicKey("SysvarRent111111111111111111111111111111111"),
    buyer,
    ata(buyer, mint, TOKEN_2022_PROGRAM_ID),
    ata(buyer, quoteMint, quoteTokenProgram),
    pda(PUMP_AMM_PROGRAM_ID, [Buffer.from("boost_vault"), pool.toBuffer()]),
    ata(
      pda(PUMP_AMM_PROGRAM_ID, [Buffer.from("boost_vault"), pool.toBuffer()]),
      quoteMint,
      quoteTokenProgram,
    ),
  ];
  if (accounts.length !== FILL_WRITABLE.length) {
    throw new Error(`fill remaining accounts length ${accounts.length}, expected ${FILL_WRITABLE.length}`);
  }
  return accounts;
}

/** Writable if any Pump CPI in fill mutates the account. Programs stay read-only except Mayhem, which Pump marks writable. */
export const FILL_WRITABLE: boolean[] = [
  true, false, false, false, false, false, false, false, false, true, false, true, true, true, false,
  true, true, true, true, true, true, true, true, true, true, true, false, false, true, true, false,
  true, false, true, true, true,   true, false, true, true, true, true, false, false, true, true, true,
  false, true,
];

/** Remaining accounts for `crank_fees`, in `crank_idx` order. Length is 13. */
export function crankRemainingAccounts(input: {
  presale: PublicKey;
  quoteMint: PublicKey;
  quoteTokenProgram: PublicKey;
  devWallet: PublicKey;
}): PublicKey[] {
  const { presale, quoteMint, quoteTokenProgram, devWallet } = input;
  const creatorVault = pda(PUMP_PROGRAM_ID, [Buffer.from("creator-vault"), presale.toBuffer()]);
  const pumpEventAuthority = pda(PUMP_PROGRAM_ID, [Buffer.from("__event_authority")]);
  const coinCreatorVaultAuthority = pda(PUMP_AMM_PROGRAM_ID, [
    Buffer.from("creator_vault"),
    presale.toBuffer(),
  ]);
  const ammEvent = pda(PUMP_AMM_PROGRAM_ID, [Buffer.from("__event_authority")]);
  const accounts = [
    quoteMint,
    quoteTokenProgram,
    ASSOCIATED_TOKEN_PROGRAM_ID,
    ata(presale, quoteMint, quoteTokenProgram),
    creatorVault,
    ata(creatorVault, quoteMint, quoteTokenProgram),
    pumpEventAuthority,
    PUMP_PROGRAM_ID,
    PUMP_AMM_PROGRAM_ID,
    coinCreatorVaultAuthority,
    ata(coinCreatorVaultAuthority, quoteMint, quoteTokenProgram),
    ammEvent,
    ata(devWallet, quoteMint, quoteTokenProgram),
  ];
  if (accounts.length !== CRANK_WRITABLE.length) {
    throw new Error(`crank remaining accounts length ${accounts.length}, expected ${CRANK_WRITABLE.length}`);
  }
  return accounts;
}

export const CRANK_WRITABLE: boolean[] = [
  false, false, false, true, true, true, false, false, false, false, true, false, true,
];
