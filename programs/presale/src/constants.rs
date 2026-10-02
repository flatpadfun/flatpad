use anchor_lang::prelude::*;

pub const PUMP_PROGRAM_ID: Pubkey = pubkey!("6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P");
pub const PUMP_AMM_PROGRAM_ID: Pubkey = pubkey!("pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA");
pub const PUMP_FEE_PROGRAM_ID: Pubkey = pubkey!("pfeeUxB6jkeY1Hxd7CsFCAjcbHA9rWtchMGdZ6VojVZ");
pub const MAYHEM_PROGRAM_ID: Pubkey = pubkey!("MAyhSmzXzV1pTf7LsNkrNwkWKTo4ougAJ1PPg47MD4e");
pub const TOKEN_2022_PROGRAM_ID: Pubkey = pubkey!("TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb");
pub const WSOL_MINT: Pubkey = pubkey!("So11111111111111111111111111111111111111112");
pub const USDC_MINT: Pubkey = pubkey!("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v");

pub const GLOBAL_DISC: [u8; 8] = [167, 232, 232, 177, 200, 108, 114, 127];
pub const FEE_CONFIG_DISC: [u8; 8] = [143, 52, 146, 187, 219, 123, 76, 155];
pub const BONDING_CURVE_DISC: [u8; 8] = [23, 183, 248, 55, 96, 216, 172, 96];

pub const CREATE_V2_DISC: [u8; 8] = [214, 144, 76, 236, 95, 139, 49, 180];
pub const BUY_V2_DISC: [u8; 8] = [184, 23, 238, 97, 103, 197, 211, 61];
pub const MIGRATE_V2_DISC: [u8; 8] = [187, 203, 18, 31, 206, 237, 254, 41];
pub const COLLECT_CREATOR_FEE_V2_DISC: [u8; 8] = [207, 17, 138, 242, 4, 34, 19, 56];
pub const COLLECT_COIN_CREATOR_FEE_DISC: [u8; 8] = [160, 57, 89, 42, 181, 139, 43, 66];

/// SOL transferred from the filler onto the buyer PDA so Pump can pay
/// buyer-side rent (volume accumulator, creator vault, quote ATAs) without
/// touching the raised quote.
pub const FILL_SOL_BUFFER: u64 = 50_000_000;

/// SOL escrowed on a new position. It pays whoever later sends that depositor
/// their tokens. A full withdrawal while the raise is open returns it.
pub const DISTRIBUTION_FEE: u64 = 3_000_000;

/// Share of a creator-mode fee that stays with the platform. The rest goes to
/// the dev wallet. Holder mode does not pay this. Dust stays with the creator.
pub const PLATFORM_FEE_BPS: u64 = 3_000;
/// Changing this requires a program upgrade.
pub const PLATFORM_ADMIN: Pubkey = pubkey!("JA9ijmzmtGcZCHNxw12g4kqJFNtTiGZs8MkQr4ddcTvW");

pub const FEE_CONFIG_POST_STABLE_SIZE: usize = 4073;
pub const FEE_CONFIG_CURRENT_SIZE: usize = 4097;
pub const FEE_CONFIG_MIN_SIZE: usize = 2512;
pub const MAX_FEE_TIERS: usize = 50;
pub const FEE_TIER_LEN: usize = 40;

pub const NAME_MAX: usize = 32;
pub const SYMBOL_MAX: usize = 13;
pub const URI_MAX: usize = 200;

/// Reward index scale. Pending = balance * acc / SCALE.
pub const REWARD_SCALE: u128 = 1_000_000_000_000;

pub const GLOBAL_ACCOUNT_MIN: usize = 1045;

/// Keep this list aligned with `keeper/src/accounts.ts`.
pub mod fill_idx {
    pub const MINT: usize = 0;
    pub const GLOBAL: usize = 1;
    pub const FEE_CONFIG: usize = 2;
    pub const QUOTE_MINT: usize = 3;
    pub const QUOTE_TOKEN_PROGRAM: usize = 4;
    pub const BASE_TOKEN_PROGRAM: usize = 5;
    pub const ASSOCIATED_TOKEN_PROGRAM: usize = 6;
    pub const PUMP_PROGRAM: usize = 7;
    pub const PUMP_EVENT_AUTHORITY: usize = 8;
    pub const MAYHEM_PROGRAM: usize = 9;
    pub const MAYHEM_GLOBAL_PARAMS: usize = 10;
    pub const MAYHEM_SOL_VAULT: usize = 11;
    pub const MAYHEM_STATE: usize = 12;
    pub const MAYHEM_TOKEN_VAULT: usize = 13;
    pub const MINT_AUTHORITY: usize = 14;
    pub const BONDING_CURVE: usize = 15;
    pub const ASSOCIATED_BASE_BONDING_CURVE: usize = 16;
    pub const ASSOCIATED_QUOTE_BONDING_CURVE: usize = 17;
    pub const FEE_RECIPIENT: usize = 18;
    pub const ASSOCIATED_QUOTE_FEE_RECIPIENT: usize = 19;
    pub const BUYBACK_FEE_RECIPIENT: usize = 20;
    pub const ASSOCIATED_QUOTE_BUYBACK_FEE_RECIPIENT: usize = 21;
    pub const PRESALE_BASE_ATA: usize = 22;
    pub const PRESALE_QUOTE_ATA: usize = 23;
    pub const CREATOR_VAULT: usize = 24;
    pub const ASSOCIATED_CREATOR_VAULT: usize = 25;
    pub const SHARING_CONFIG: usize = 26;
    pub const GLOBAL_VOLUME_ACCUMULATOR: usize = 27;
    pub const USER_VOLUME_ACCUMULATOR: usize = 28;
    pub const ASSOCIATED_USER_VOLUME_ACCUMULATOR: usize = 29;
    pub const FEE_PROGRAM: usize = 30;
    pub const WITHDRAW_AUTHORITY: usize = 31;
    pub const PUMP_AMM_PROGRAM: usize = 32;
    pub const POOL: usize = 33;
    pub const POOL_AUTHORITY: usize = 34;
    pub const POOL_AUTHORITY_MINT_ACCOUNT: usize = 35;
    pub const POOL_AUTHORITY_QUOTE_ACCOUNT: usize = 36;
    pub const AMM_GLOBAL_CONFIG: usize = 37;
    pub const LP_MINT: usize = 38;
    pub const USER_POOL_TOKEN_ACCOUNT: usize = 39;
    pub const POOL_BASE_TOKEN_ACCOUNT: usize = 40;
    pub const POOL_QUOTE_TOKEN_ACCOUNT: usize = 41;
    pub const PUMP_AMM_EVENT_AUTHORITY: usize = 42;
    pub const RENT: usize = 43;
    /// System-owned PDA `["buyer", presale]`. Pump's buy transfers SOL with the
    /// system program, which refuses a source account that carries data.
    pub const BUYER: usize = 44;
    pub const BUYER_BASE_ATA: usize = 45;
    pub const BUYER_QUOTE_ATA: usize = 46;
    /// Pump `migrate_v2` reads these after the IDL accounts. Seeds `["boost_vault", pool]` on the AMM.
    pub const BOOST_VAULT_AUTHORITY: usize = 47;
    pub const BOOST_VAULT: usize = 48;
    pub const LEN: usize = 49;
}

/// Keep this list aligned with `keeper/src/accounts.ts`.
pub mod crank_idx {
    pub const QUOTE_MINT: usize = 0;
    pub const QUOTE_TOKEN_PROGRAM: usize = 1;
    pub const ASSOCIATED_TOKEN_PROGRAM: usize = 2;
    pub const PRESALE_QUOTE_ATA: usize = 3;
    pub const CREATOR_VAULT: usize = 4;
    pub const CREATOR_VAULT_QUOTE_ATA: usize = 5;
    pub const PUMP_EVENT_AUTHORITY: usize = 6;
    pub const PUMP_PROGRAM: usize = 7;
    pub const PUMP_AMM_PROGRAM: usize = 8;
    pub const COIN_CREATOR_VAULT_AUTHORITY: usize = 9;
    pub const COIN_CREATOR_VAULT_ATA: usize = 10;
    pub const PUMP_AMM_EVENT_AUTHORITY: usize = 11;
    pub const DEV_QUOTE_ATA: usize = 12;
    pub const LEN: usize = 13;
}
