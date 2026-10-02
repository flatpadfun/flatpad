use anchor_lang::prelude::*;

mod constants;
mod error;
mod instructions;
mod pump;
mod quote;
mod state;

use instructions::*;

declare_id!("EGFHbbVkZKhv3owPhUocyBqTQKeYSuehaxQ7ZMLZ2p1D");

#[program]
pub mod presale {
    use super::*;

    pub fn create_presale(ctx: Context<CreatePresale>, args: CreatePresaleArgs) -> Result<()> {
        instructions::create_presale(ctx, args)
    }

    pub fn deposit(ctx: Context<Deposit>, amount: u64) -> Result<()> {
        instructions::deposit(ctx, amount)
    }

    pub fn withdraw(ctx: Context<Withdraw>, amount: u64) -> Result<()> {
        instructions::withdraw(ctx, amount)
    }

    /// Create the Pump coin and buy the whole curve. Migrate is separate.
    pub fn fill<'info>(ctx: Context<'_, '_, '_, 'info, Fill<'info>>) -> Result<()> {
        instructions::fill(ctx)
    }

    /// Move a bought-out curve to PumpSwap. Run in the transaction after `fill`.
    pub fn migrate<'info>(ctx: Context<'_, '_, '_, 'info, Fill<'info>>) -> Result<()> {
        instructions::migrate(ctx)
    }

    pub fn claim_tokens(ctx: Context<ClaimTokens>) -> Result<()> {
        instructions::claim_tokens(ctx)
    }

    pub fn crank_fees<'info>(ctx: Context<'_, '_, '_, 'info, CrankFees<'info>>) -> Result<()> {
        instructions::crank_fees(ctx)
    }

    pub fn claim_holder_fees(ctx: Context<ClaimHolderFees>) -> Result<()> {
        instructions::claim_holder_fees(ctx)
    }
}
