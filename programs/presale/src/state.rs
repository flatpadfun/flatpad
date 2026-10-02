use anchor_lang::prelude::*;

use crate::constants::{NAME_MAX, SYMBOL_MAX, URI_MAX};

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq, InitSpace)]
pub enum FeeMode {
    Dev,
    Holders,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq, InitSpace)]
pub enum Status {
    Raising,
    Migrated,
    /// Curve is bought out. Migrate is a second transaction because create, buy,
    /// and migrate together exceed the 64-instruction trace limit.
    Bought,
}

#[account]
#[derive(InitSpace)]
pub struct Presale {
    pub launcher: Pubkey,
    pub dev_wallet: Pubkey,
    pub quote_mint: Pubkey,
    pub quote_token_program: Pubkey,
    pub fee_mode: FeeMode,
    pub status: Status,
    pub nonce: u64,
    pub name_len: u8,
    pub name: [u8; NAME_MAX],
    pub symbol_len: u8,
    pub symbol: [u8; SYMBOL_MAX],
    pub uri_len: u8,
    pub uri: [u8; URI_MAX],
    pub quote_target: u64,
    pub quote_raised: u64,
    pub tokens_received: u64,
    pub quote_spent: u64,
    pub virtual_token_reserves: u64,
    pub virtual_quote_reserves: u64,
    pub real_token_reserves: u64,
    pub token_total_supply: u64,
    pub protocol_fee_bps: u64,
    pub creator_fee_bps: u64,
    /// Cumulative quote paid per base-token unit, scaled by REWARD_SCALE.
    pub acc_quote_per_token: u128,
    /// Fees collected but not yet folded into the index because nobody held tokens.
    pub undistributed_fees: u64,
    pub bump: u8,
    pub mint_bump: u8,
}

impl Presale {
    pub fn name(&self) -> Result<&str> {
        core::str::from_utf8(&self.name[..self.name_len as usize])
            .map_err(|_| error!(crate::error::PresaleError::BadMetadata))
    }

    pub fn symbol(&self) -> Result<&str> {
        core::str::from_utf8(&self.symbol[..self.symbol_len as usize])
            .map_err(|_| error!(crate::error::PresaleError::BadMetadata))
    }

    pub fn uri(&self) -> Result<&str> {
        core::str::from_utf8(&self.uri[..self.uri_len as usize])
            .map_err(|_| error!(crate::error::PresaleError::BadMetadata))
    }
}

#[account]
#[derive(InitSpace)]
pub struct Position {
    pub presale: Pubkey,
    pub owner: Pubkey,
    pub quote_amount: u64,
    pub bump: u8,
}

/// Lifetime creator fees gathered for a presale. Unclaimed Pump vaults are not included.
#[account]
#[derive(InitSpace)]
pub struct FeeTally {
    pub total: u64,
    pub seeded: bool,
    pub bump: u8,
}

#[account]
#[derive(InitSpace)]
pub struct HolderReward {
    pub presale: Pubkey,
    pub owner: Pubkey,
    /// Settled amount of `balance * acc_quote_per_token / REWARD_SCALE`.
    pub reward_debt: u128,
    pub bump: u8,
}

#[event]
pub struct PresaleCreated {
    pub presale: Pubkey,
    pub mint: Pubkey,
    pub quote_mint: Pubkey,
    pub quote_target: u64,
    pub fee_mode: FeeMode,
    pub dev_wallet: Pubkey,
}

#[event]
pub struct Deposited {
    pub presale: Pubkey,
    pub owner: Pubkey,
    pub amount: u64,
    pub total: u64,
}

#[event]
pub struct Withdrawn {
    pub presale: Pubkey,
    pub owner: Pubkey,
    pub amount: u64,
}

#[event]
pub struct Filled {
    pub presale: Pubkey,
    pub mint: Pubkey,
    pub quote_spent: u64,
    pub tokens_received: u64,
}

#[event]
pub struct TokensClaimed {
    pub presale: Pubkey,
    pub owner: Pubkey,
    pub quote_amount: u64,
    pub tokens: u64,
}

#[event]
pub struct FeesCranked {
    pub presale: Pubkey,
    pub collected: u64,
    pub acc_quote_per_token: u128,
}

#[event]
pub struct HolderFeesClaimed {
    pub presale: Pubkey,
    pub owner: Pubkey,
    pub amount: u64,
}
