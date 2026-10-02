use anchor_lang::prelude::*;
use anchor_lang::system_program::{self, Transfer};
use anchor_spl::associated_token::AssociatedToken;
use anchor_spl::token_interface::{self, Mint, TokenAccount, TokenInterface, TransferChecked};

use crate::constants::*;
use crate::error::PresaleError;
use crate::pump::{self, CrankInputs, FillInputs};
use crate::quote::{self, CurveSnapshot};
use crate::state::*;

fn checked_metadata(name: &str, symbol: &str, uri: &str) -> Result<()> {
    require!(
        !name.is_empty()
            && name.len() <= NAME_MAX
            && !symbol.is_empty()
            && symbol.len() <= SYMBOL_MAX
            && !uri.is_empty()
            && uri.len() <= URI_MAX,
        PresaleError::BadMetadata
    );
    Ok(())
}

fn write_bytes<const N: usize>(dest: &mut [u8; N], value: &str) -> u8 {
    dest[..value.len()].copy_from_slice(value.as_bytes());
    value.len() as u8
}

fn mint_pda(presale: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[b"mint", presale.as_ref()], &crate::ID)
}

fn same_snapshot(presale: &Presale, snapshot: &CurveSnapshot, target: u64) -> Result<()> {
    require!(
        presale.quote_target == target
            && presale.virtual_token_reserves == snapshot.virtual_token_reserves
            && presale.virtual_quote_reserves == snapshot.virtual_quote_reserves
            && presale.real_token_reserves == snapshot.real_token_reserves
            && presale.token_total_supply == snapshot.token_total_supply
            && presale.protocol_fee_bps == snapshot.protocol_fee_bps
            && presale.creator_fee_bps == snapshot.creator_fee_bps,
        PresaleError::PumpParametersChanged
    );
    Ok(())
}

struct PresaleSeeds {
    launcher: Pubkey,
    quote_mint: Pubkey,
    nonce: [u8; 8],
    bump: [u8; 1],
}

impl PresaleSeeds {
    fn load(presale: &Presale) -> Self {
        Self {
            launcher: presale.launcher,
            quote_mint: presale.quote_mint,
            nonce: presale.nonce.to_le_bytes(),
            bump: [presale.bump],
        }
    }

    fn slices(&self) -> [&[u8]; 5] {
        [
            b"presale",
            self.launcher.as_ref(),
            self.quote_mint.as_ref(),
            self.nonce.as_ref(),
            self.bump.as_ref(),
        ]
    }
}

#[derive(Accounts)]
#[instruction(args: CreatePresaleArgs)]
pub struct CreatePresale<'info> {
    #[account(mut)]
    pub launcher: Signer<'info>,
    #[account(
        init,
        payer = launcher,
        space = 8 + Presale::INIT_SPACE,
        seeds = [
            b"presale",
            launcher.key().as_ref(),
            quote_mint.key().as_ref(),
            &args.nonce.to_le_bytes(),
        ],
        bump
    )]
    pub presale: Account<'info, Presale>,
    pub quote_mint: InterfaceAccount<'info, Mint>,
    pub quote_token_program: Interface<'info, TokenInterface>,
    /// CHECK: Pump global, address-checked against the canonical PDA.
    pub global: UncheckedAccount<'info>,
    /// CHECK: Pump fee config, address-checked against the canonical PDA.
    pub fee_config: UncheckedAccount<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct CreatePresaleArgs {
    pub name: String,
    pub symbol: String,
    pub uri: String,
    pub fee_mode: FeeMode,
    pub nonce: u64,
    pub dev_wallet: Pubkey,
}

pub fn create_presale(ctx: Context<CreatePresale>, args: CreatePresaleArgs) -> Result<()> {
    checked_metadata(&args.name, &args.symbol, &args.uri)?;
    require!(args.dev_wallet != Pubkey::default(), PresaleError::BadDevWallet);
    require_keys_eq!(
        *ctx.accounts.quote_mint.to_account_info().owner,
        ctx.accounts.quote_token_program.key(),
        PresaleError::QuoteTokenProgram
    );
    let (global_pda, _) = pump::pump_pda(&[b"global"]);
    let (fee_config_pda, _) = pump::fee_pda(&[b"fee_config", PUMP_PROGRAM_ID.as_ref()]);
    require_keys_eq!(ctx.accounts.global.key(), global_pda, PresaleError::BadAccount);
    require_keys_eq!(
        ctx.accounts.fee_config.key(),
        fee_config_pda,
        PresaleError::BadAccount
    );

    let global = quote::decode_global(&ctx.accounts.global.try_borrow_data()?)?;
    let fee_config = quote::decode_fee_config(&ctx.accounts.fee_config.try_borrow_data()?)?;
    let quote_mint = ctx.accounts.quote_mint.key();
    let target = quote::quote_target(&global, &fee_config, &quote_mint)?;
    let snapshot = quote::snapshot_for(&global, &fee_config, &quote_mint)?;

    let presale = &mut ctx.accounts.presale;
    let (mint, mint_bump) = mint_pda(&presale.key());
    presale.launcher = ctx.accounts.launcher.key();
    presale.dev_wallet = args.dev_wallet;
    presale.quote_mint = quote_mint;
    presale.quote_token_program = ctx.accounts.quote_token_program.key();
    presale.fee_mode = args.fee_mode;
    presale.status = Status::Raising;
    presale.nonce = args.nonce;
    presale.name_len = write_bytes(&mut presale.name, &args.name);
    presale.symbol_len = write_bytes(&mut presale.symbol, &args.symbol);
    presale.uri_len = write_bytes(&mut presale.uri, &args.uri);
    presale.quote_target = target;
    presale.quote_raised = 0;
    presale.tokens_received = 0;
    presale.quote_spent = 0;
    presale.virtual_token_reserves = snapshot.virtual_token_reserves;
    presale.virtual_quote_reserves = snapshot.virtual_quote_reserves;
    presale.real_token_reserves = snapshot.real_token_reserves;
    presale.token_total_supply = snapshot.token_total_supply;
    presale.protocol_fee_bps = snapshot.protocol_fee_bps;
    presale.creator_fee_bps = snapshot.creator_fee_bps;
    presale.acc_quote_per_token = 0;
    presale.undistributed_fees = 0;
    presale.bump = ctx.bumps.presale;
    presale.mint_bump = mint_bump;

    emit!(PresaleCreated {
        presale: presale.key(),
        mint,
        quote_mint,
        quote_target: target,
        fee_mode: presale.fee_mode,
        dev_wallet: presale.dev_wallet,
    });
    Ok(())
}

#[derive(Accounts)]
pub struct Deposit<'info> {
    #[account(mut)]
    pub user: Signer<'info>,
    #[account(mut, constraint = presale.status == Status::Raising @ PresaleError::RaiseClosed)]
    pub presale: Account<'info, Presale>,
    #[account(
        init_if_needed,
        payer = user,
        space = 8 + Position::INIT_SPACE,
        seeds = [b"position", presale.key().as_ref(), user.key().as_ref()],
        bump
    )]
    pub position: Account<'info, Position>,
    pub system_program: Program<'info, System>,
    /// CHECK: system-owned SOL vault `["buyer", presale]`. Pump pays the buy from an account with no data.
    #[account(mut, seeds = [b"buyer", presale.key().as_ref()], bump)]
    pub buyer: UncheckedAccount<'info>,
    pub token_program: Option<Interface<'info, TokenInterface>>,
    pub quote_mint: Option<InterfaceAccount<'info, Mint>>,
    #[account(mut)]
    pub user_quote_ata: Option<InterfaceAccount<'info, TokenAccount>>,
    #[account(mut)]
    pub presale_quote_ata: Option<InterfaceAccount<'info, TokenAccount>>,
}

pub fn deposit(ctx: Context<Deposit>, amount: u64) -> Result<()> {
    let room = ctx
        .accounts
        .presale
        .quote_target
        .checked_sub(ctx.accounts.presale.quote_raised)
        .ok_or(PresaleError::MathOverflow)?;
    let amount = amount.min(room);
    require!(amount > 0, PresaleError::NothingToDeposit);

    if ctx.accounts.presale.quote_mint == WSOL_MINT {
        system_program::transfer(
            CpiContext::new(
                ctx.accounts.system_program.to_account_info(),
                Transfer {
                    from: ctx.accounts.user.to_account_info(),
                    to: ctx.accounts.buyer.to_account_info(),
                },
            ),
            amount,
        )?;
    } else {
        transfer_quote_in(&ctx, amount)?;
    }

    let presale_key = ctx.accounts.presale.key();
    let user_key = ctx.accounts.user.key();
    let fresh = ctx.accounts.position.quote_amount == 0;
    if fresh {
        system_program::transfer(
            CpiContext::new(
                ctx.accounts.system_program.to_account_info(),
                Transfer {
                    from: ctx.accounts.user.to_account_info(),
                    to: ctx.accounts.position.to_account_info(),
                },
            ),
            DISTRIBUTION_FEE,
        )?;
    }
    let position = &mut ctx.accounts.position;
    if fresh {
        position.presale = presale_key;
        position.owner = user_key;
        position.bump = ctx.bumps.position;
    }
    require_keys_eq!(position.owner, user_key, PresaleError::BadAccount);
    require_keys_eq!(position.presale, presale_key, PresaleError::BadAccount);
    position.quote_amount = position
        .quote_amount
        .checked_add(amount)
        .ok_or(PresaleError::MathOverflow)?;
    let total = position.quote_amount;
    ctx.accounts.presale.quote_raised = ctx
        .accounts
        .presale
        .quote_raised
        .checked_add(amount)
        .ok_or(PresaleError::MathOverflow)?;

    emit!(Deposited {
        presale: presale_key,
        owner: user_key,
        amount,
        total,
    });
    Ok(())
}

fn transfer_quote_in(ctx: &Context<Deposit>, amount: u64) -> Result<()> {
    let quote_mint = ctx.accounts.quote_mint.as_ref().ok_or(PresaleError::BadAccount)?;
    let user_ata = ctx.accounts.user_quote_ata.as_ref().ok_or(PresaleError::BadAccount)?;
    let vault_ata = ctx.accounts.presale_quote_ata.as_ref().ok_or(PresaleError::BadAccount)?;
    let token_program = ctx.accounts.token_program.as_ref().ok_or(PresaleError::BadAccount)?;
    require_keys_eq!(quote_mint.key(), ctx.accounts.presale.quote_mint, PresaleError::BadAccount);
    require_keys_eq!(user_ata.mint, quote_mint.key(), PresaleError::BadAccount);
    require_keys_eq!(vault_ata.mint, quote_mint.key(), PresaleError::BadAccount);
    require_keys_eq!(user_ata.owner, ctx.accounts.user.key(), PresaleError::BadAccount);
    require_keys_eq!(vault_ata.owner, ctx.accounts.presale.key(), PresaleError::BadAccount);
    token_interface::transfer_checked(
        CpiContext::new(
            token_program.to_account_info(),
            TransferChecked {
                from: user_ata.to_account_info(),
                mint: quote_mint.to_account_info(),
                to: vault_ata.to_account_info(),
                authority: ctx.accounts.user.to_account_info(),
            },
        ),
        amount,
        quote_mint.decimals,
    )?;
    Ok(())
}

#[derive(Accounts)]
pub struct Withdraw<'info> {
    #[account(mut)]
    pub user: Signer<'info>,
    #[account(mut, constraint = presale.status == Status::Raising @ PresaleError::RaiseClosed)]
    pub presale: Account<'info, Presale>,
    #[account(
        mut,
        seeds = [b"position", presale.key().as_ref(), user.key().as_ref()],
        bump = position.bump,
        has_one = presale,
        constraint = position.owner == user.key() @ PresaleError::BadAccount,
    )]
    pub position: Account<'info, Position>,
    pub system_program: Program<'info, System>,
    /// CHECK: system-owned SOL vault `["buyer", presale]`.
    #[account(mut, seeds = [b"buyer", presale.key().as_ref()], bump)]
    pub buyer: UncheckedAccount<'info>,
    pub token_program: Option<Interface<'info, TokenInterface>>,
    pub quote_mint: Option<InterfaceAccount<'info, Mint>>,
    #[account(mut)]
    pub user_quote_ata: Option<InterfaceAccount<'info, TokenAccount>>,
    #[account(mut)]
    pub presale_quote_ata: Option<InterfaceAccount<'info, TokenAccount>>,
}

pub fn withdraw(ctx: Context<Withdraw>, amount: u64) -> Result<()> {
    let position_amount = ctx.accounts.position.quote_amount;
    require!(position_amount > 0, PresaleError::EmptyPosition);
    require!(amount > 0 && amount <= position_amount, PresaleError::AbovePosition);
    ctx.accounts.presale.quote_raised = ctx
        .accounts
        .presale
        .quote_raised
        .checked_sub(amount)
        .ok_or(PresaleError::MathOverflow)?;

    if ctx.accounts.presale.quote_mint == WSOL_MINT {
        let presale_key = ctx.accounts.presale.key();
        let bump = [ctx.bumps.buyer];
        let seeds: [&[u8]; 3] = [b"buyer", presale_key.as_ref(), &bump];
        system_program::transfer(
            CpiContext::new_with_signer(
                ctx.accounts.system_program.to_account_info(),
                Transfer {
                    from: ctx.accounts.buyer.to_account_info(),
                    to: ctx.accounts.user.to_account_info(),
                },
                &[&seeds],
            ),
            amount,
        )?;
    } else {
        transfer_quote_out(&ctx, amount)?;
    }

    let remaining = position_amount
        .checked_sub(amount)
        .ok_or(PresaleError::MathOverflow)?;
    if remaining == 0 {
        // Rent and the distribution fee both return to the depositor.
        ctx.accounts
            .position
            .close(ctx.accounts.user.to_account_info())?;
    } else {
        ctx.accounts.position.quote_amount = remaining;
    }

    emit!(Withdrawn {
        presale: ctx.accounts.presale.key(),
        owner: ctx.accounts.user.key(),
        amount,
    });
    Ok(())
}

fn transfer_quote_out(ctx: &Context<Withdraw>, amount: u64) -> Result<()> {
    let quote_mint = ctx.accounts.quote_mint.as_ref().ok_or(PresaleError::BadAccount)?;
    let user_ata = ctx.accounts.user_quote_ata.as_ref().ok_or(PresaleError::BadAccount)?;
    let vault_ata = ctx.accounts.presale_quote_ata.as_ref().ok_or(PresaleError::BadAccount)?;
    let token_program = ctx.accounts.token_program.as_ref().ok_or(PresaleError::BadAccount)?;
    require_keys_eq!(quote_mint.key(), ctx.accounts.presale.quote_mint, PresaleError::BadAccount);
    require_keys_eq!(vault_ata.owner, ctx.accounts.presale.key(), PresaleError::BadAccount);
    require_keys_eq!(user_ata.owner, ctx.accounts.user.key(), PresaleError::BadAccount);
    let seed_state = PresaleSeeds::load(&ctx.accounts.presale);
    let seeds = seed_state.slices();
    token_interface::transfer_checked(
        CpiContext::new_with_signer(
            token_program.to_account_info(),
            TransferChecked {
                from: vault_ata.to_account_info(),
                mint: quote_mint.to_account_info(),
                to: user_ata.to_account_info(),
                authority: ctx.accounts.presale.to_account_info(),
            },
            &[&seeds],
        ),
        amount,
        quote_mint.decimals,
    )?;
    Ok(())
}

#[derive(Accounts)]
pub struct Fill<'info> {
    #[account(mut)]
    pub cranker: Signer<'info>,
    #[account(mut)]
    pub presale: Account<'info, Presale>,
    pub system_program: Program<'info, System>,
}

pub fn fill<'info>(ctx: Context<'_, '_, '_, 'info, Fill<'info>>) -> Result<()> {
    require!(
        ctx.accounts.presale.status == Status::Raising,
        PresaleError::AlreadyMigrated
    );
    require!(
        ctx.accounts.presale.quote_raised == ctx.accounts.presale.quote_target,
        PresaleError::RaiseNotFull
    );
    let remaining = ctx.remaining_accounts;
    require!(remaining.len() == fill_idx::LEN, PresaleError::BadAccount);

    let global_ai = &remaining[fill_idx::GLOBAL];
    let fee_ai = &remaining[fill_idx::FEE_CONFIG];
    let global = quote::decode_global(&global_ai.try_borrow_data()?)?;
    let fee_config = quote::decode_fee_config(&fee_ai.try_borrow_data()?)?;
    let quote_mint = ctx.accounts.presale.quote_mint;
    let target = quote::quote_target(&global, &fee_config, &quote_mint)?;
    let snapshot = quote::snapshot_for(&global, &fee_config, &quote_mint)?;
    same_snapshot(&ctx.accounts.presale, &snapshot, target)?;

    require_keys_eq!(
        remaining[fill_idx::FEE_RECIPIENT].key(),
        global.fee_recipient,
        PresaleError::BadAccount
    );
    require_keys_eq!(
        remaining[fill_idx::BUYBACK_FEE_RECIPIENT].key(),
        global.buyback_fee_recipient,
        PresaleError::BadAccount
    );
    require_keys_eq!(
        remaining[fill_idx::WITHDRAW_AUTHORITY].key(),
        global.withdraw_authority,
        PresaleError::BadAccount
    );
    require_keys_eq!(
        remaining[fill_idx::QUOTE_TOKEN_PROGRAM].key(),
        ctx.accounts.presale.quote_token_program,
        PresaleError::BadAccount
    );
    require_keys_eq!(
        remaining[fill_idx::ASSOCIATED_TOKEN_PROGRAM].key(),
        anchor_spl::associated_token::ID,
        PresaleError::BadAccount
    );

    let (mint, mint_bump) = mint_pda(&ctx.accounts.presale.key());
    require!(mint_bump == ctx.accounts.presale.mint_bump, PresaleError::BadAccount);
    pump::require_fill_pdas(&mint, &ctx.accounts.presale.key(), &quote_mint, remaining)?;

    // The system program has to materialize the buyer. A direct lamport credit
    // onto a zero-lamport account is dropped, and the next CPI then sees a
    // missing balance.
    system_program::transfer(
        CpiContext::new(
            ctx.accounts.system_program.to_account_info(),
            Transfer {
                from: ctx.accounts.cranker.to_account_info(),
                to: remaining[fill_idx::BUYER].clone(),
            },
        ),
        FILL_SOL_BUFFER,
    )?;

    let name = ctx.accounts.presale.name()?.to_string();
    let symbol = ctx.accounts.presale.symbol()?.to_string();
    let uri = ctx.accounts.presale.uri()?.to_string();
    let mint_bump_seed = [ctx.accounts.presale.mint_bump];
    let presale_key = ctx.accounts.presale.key();
    let min_tokens_out = ctx.accounts.presale.real_token_reserves;
    let spendable = ctx.accounts.presale.quote_target;
    let seed_state = PresaleSeeds::load(&ctx.accounts.presale);
    let cranker = ctx.accounts.cranker.to_account_info();
    let presale_ai = ctx.accounts.presale.to_account_info();
    let system_program = ctx.accounts.system_program.to_account_info();

    {
        let presale_seeds = seed_state.slices();
        let mint_seeds: [&[u8]; 3] = [b"mint", presale_key.as_ref(), &mint_bump_seed];
        let (_, buyer_bump) = Pubkey::find_program_address(&[b"buyer", presale_key.as_ref()], &crate::ID);
        let buyer_bump_seed = [buyer_bump];
        let buyer_seeds: [&[u8]; 3] = [b"buyer", presale_key.as_ref(), &buyer_bump_seed];
        let input = FillInputs {
            remaining,
            cranker,
            presale: presale_ai,
            system_program,
            presale_seeds: &presale_seeds,
            buyer: remaining[fill_idx::BUYER].clone(),
            buyer_seeds: &buyer_seeds,
            mint_seeds: &mint_seeds,
            name: &name,
            symbol: &symbol,
            uri: &uri,
            creator: presale_key,
            spendable_quote_in: spendable,
            min_tokens_out,
        };
        pump::create_v2(&input)?;
        pump::create_buyer_atas(&input)?;
        pump::move_quote_to_buyer(&input)?;
        pump::buy_v2(&input)?;
        require!(
            pump::curve_is_complete(&remaining[fill_idx::BONDING_CURVE])?,
            PresaleError::CurveNotCompleted
        );
    }

    let tokens_received = pump::token_amount(&remaining[fill_idx::BUYER_BASE_ATA])?;
    require!(tokens_received > 0, PresaleError::CurveNotCompleted);
    let presale = &mut ctx.accounts.presale;
    presale.tokens_received = tokens_received;
    presale.quote_spent = spendable;
    presale.status = Status::Bought;

    emit!(Filled {
        presale: presale.key(),
        mint,
        quote_spent: spendable,
        tokens_received,
    });
    Ok(())
}

pub fn migrate<'info>(ctx: Context<'_, '_, '_, 'info, Fill<'info>>) -> Result<()> {
    require!(
        ctx.accounts.presale.status == Status::Bought,
        PresaleError::CurveNotCompleted
    );
    let remaining = ctx.remaining_accounts;
    require!(remaining.len() == fill_idx::LEN, PresaleError::BadAccount);
    let presale_key = ctx.accounts.presale.key();
    let (mint, mint_bump) = mint_pda(&presale_key);
    require!(mint_bump == ctx.accounts.presale.mint_bump, PresaleError::BadAccount);
    pump::require_fill_pdas(&mint, &presale_key, &ctx.accounts.presale.quote_mint, remaining)?;
    let pool = &remaining[fill_idx::POOL];
    if pump::amm_pool_open(pool.owner, pool.data_len()) {
        require!(
            pump::curve_is_complete(&remaining[fill_idx::BONDING_CURVE])?,
            PresaleError::CurveNotCompleted
        );
        ctx.accounts.presale.status = Status::Migrated;
        return Ok(());
    }
    let seed_state = PresaleSeeds::load(&ctx.accounts.presale);
    let presale_seeds = seed_state.slices();
    let mint_bump_seed = [ctx.accounts.presale.mint_bump];
    let mint_seeds: [&[u8]; 3] = [b"mint", presale_key.as_ref(), &mint_bump_seed];
    let buyer_bump_seed = [0u8];
    let buyer_seeds: [&[u8]; 3] = [b"buyer", presale_key.as_ref(), &buyer_bump_seed];
    let input = FillInputs {
        remaining,
        cranker: ctx.accounts.cranker.to_account_info(),
        presale: ctx.accounts.presale.to_account_info(),
        system_program: ctx.accounts.system_program.to_account_info(),
        presale_seeds: &presale_seeds,
        buyer: remaining[fill_idx::BUYER].clone(),
        buyer_seeds: &buyer_seeds,
        mint_seeds: &mint_seeds,
        name: "",
        symbol: "",
        uri: "",
        creator: presale_key,
        spendable_quote_in: 0,
        min_tokens_out: 0,
    };
    pump::migrate_v2(&input)?;
    ctx.accounts.presale.status = Status::Migrated;
    Ok(())
}

#[derive(Accounts)]
pub struct ClaimTokens<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,
    #[account(mut, constraint = presale.status == Status::Migrated @ PresaleError::NotMigrated)]
    pub presale: Box<Account<'info, Presale>>,
    #[account(
        mut,
        seeds = [b"position", presale.key().as_ref(), owner.key().as_ref()],
        bump = position.bump,
        has_one = presale,
        has_one = owner,
        close = owner
    )]
    pub position: Account<'info, Position>,
    /// CHECK: depositor receiving tokens and position rent. Does not sign.
    #[account(mut)]
    pub owner: UncheckedAccount<'info>,
    /// CHECK: system-owned buyer PDA that holds the bought tokens.
    #[account(seeds = [b"buyer", presale.key().as_ref()], bump)]
    pub buyer: UncheckedAccount<'info>,
    #[account(constraint = mint.key() == mint_pda(&presale.key()).0 @ PresaleError::BadAccount)]
    pub mint: InterfaceAccount<'info, Mint>,
    #[account(
        mut,
        associated_token::mint = mint,
        associated_token::authority = buyer,
        associated_token::token_program = token_program
    )]
    pub buyer_ata: InterfaceAccount<'info, TokenAccount>,
    #[account(
        init_if_needed,
        payer = payer,
        associated_token::mint = mint,
        associated_token::authority = owner,
        associated_token::token_program = token_program
    )]
    pub owner_ata: InterfaceAccount<'info, TokenAccount>,
    pub token_program: Interface<'info, TokenInterface>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}

pub fn claim_tokens(ctx: Context<ClaimTokens>) -> Result<()> {
    let quote_amount = ctx.accounts.position.quote_amount;
    let tokens = (quote_amount as u128)
        .checked_mul(ctx.accounts.presale.tokens_received as u128)
        .ok_or(PresaleError::MathOverflow)?
        / (ctx.accounts.presale.quote_target as u128);
    let tokens = u64::try_from(tokens).map_err(|_| PresaleError::MathOverflow)?;
    require!(tokens > 0, PresaleError::ZeroPayout);
    require!(
        ctx.accounts.buyer_ata.amount >= tokens,
        PresaleError::InsufficientVault
    );

    let presale_key = ctx.accounts.presale.key();
    let bump = [ctx.bumps.buyer];
    let seeds: [&[u8]; 3] = [b"buyer", presale_key.as_ref(), &bump];
    token_interface::transfer_checked(
        CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            TransferChecked {
                from: ctx.accounts.buyer_ata.to_account_info(),
                mint: ctx.accounts.mint.to_account_info(),
                to: ctx.accounts.owner_ata.to_account_info(),
                authority: ctx.accounts.buyer.to_account_info(),
            },
            &[&seeds],
        ),
        tokens,
        ctx.accounts.mint.decimals,
    )?;

    let position_info = ctx.accounts.position.to_account_info();
    let rent = Rent::get()?.minimum_balance(position_info.data_len());
    let surplus = position_info.lamports().saturating_sub(rent);
    let reward = surplus.min(DISTRIBUTION_FEE);
    if reward > 0 {
        let payer_info = ctx.accounts.payer.to_account_info();
        **position_info.try_borrow_mut_lamports()? -= reward;
        **payer_info.try_borrow_mut_lamports()? += reward;
    }

    emit!(TokensClaimed {
        presale: ctx.accounts.presale.key(),
        owner: ctx.accounts.owner.key(),
        quote_amount,
        tokens,
    });
    Ok(())
}

#[derive(Accounts)]
pub struct CrankFees<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,
    #[account(mut, constraint = presale.status == Status::Migrated @ PresaleError::NotMigrated)]
    pub presale: Account<'info, Presale>,
    /// CHECK: dev wallet. Receives SOL in dev mode.
    #[account(mut, address = presale.dev_wallet)]
    pub dev_wallet: UncheckedAccount<'info>,
    /// CHECK: coin mint, address-checked.
    pub base_mint: UncheckedAccount<'info>,
    /// CHECK: PumpSwap pool base vault, address-checked.
    pub pool_base_ata: UncheckedAccount<'info>,
    /// CHECK: unsold presale allocation, address-checked.
    pub presale_base_ata: UncheckedAccount<'info>,
    #[account(
        init_if_needed,
        payer = payer,
        space = 8 + FeeTally::INIT_SPACE,
        seeds = [b"fee-tally", presale.key().as_ref()],
        bump
    )]
    pub tally: Account<'info, FeeTally>,
    /// CHECK: platform treasury. Address is the hardcoded admin.
    #[account(mut, address = PLATFORM_ADMIN)]
    pub platform_wallet: UncheckedAccount<'info>,
    /// CHECK: platform quote account. Used for token quotes; ignored for SOL.
    #[account(mut)]
    pub platform_quote_ata: UncheckedAccount<'info>,
    pub system_program: Program<'info, System>,
}

pub fn crank_fees<'info>(ctx: Context<'_, '_, '_, 'info, CrankFees<'info>>) -> Result<()> {
    let (mint, _) = mint_pda(&ctx.accounts.presale.key());
    require_keys_eq!(ctx.accounts.base_mint.key(), mint, PresaleError::BadAccount);
    let (pool_authority, _) = pump::pump_pda(&[b"pool-authority", mint.as_ref()]);
    let (pool, _) = pump::amm_pda(&[
        b"pool",
        &0u16.to_le_bytes(),
        pool_authority.as_ref(),
        mint.as_ref(),
        ctx.accounts.presale.quote_mint.as_ref(),
    ]);
    require_keys_eq!(
        ctx.accounts.pool_base_ata.key(),
        pump::ata(&pool, &mint, &TOKEN_2022_PROGRAM_ID),
        PresaleError::BadAccount
    );
    let (buyer, _) = Pubkey::find_program_address(
        &[b"buyer", ctx.accounts.presale.key().as_ref()],
        &crate::ID,
    );
    require_keys_eq!(
        ctx.accounts.presale_base_ata.key(),
        pump::ata(&buyer, &mint, &TOKEN_2022_PROGRAM_ID),
        PresaleError::BadAccount
    );

    let sol_quote = ctx.accounts.presale.quote_mint == WSOL_MINT;
    let curve_had_fees = pump::curve_vault_has_fees(ctx.remaining_accounts, sol_quote)?;
    let curve_fee = quote::creator_fee_on_buy(
        ctx.accounts.presale.quote_spent,
        ctx.accounts.presale.protocol_fee_bps,
        ctx.accounts.presale.creator_fee_bps,
    )?;
    let seed_state = PresaleSeeds::load(&ctx.accounts.presale);
    let seeds = seed_state.slices();
    let presale_ai = ctx.accounts.presale.to_account_info();
    let system_program = ctx.accounts.system_program.to_account_info();
    let collected = {
        let input = CrankInputs {
            remaining: ctx.remaining_accounts,
            presale: presale_ai,
            system_program,
            presale_seeds: &seeds,
        };
        pump::collect_creator_fees(&input)?
    };
    if !ctx.accounts.tally.seeded {
        ctx.accounts.tally.total = if curve_had_fees { 0 } else { curve_fee };
        ctx.accounts.tally.seeded = true;
    }
    ctx.accounts.tally.total = ctx
        .accounts
        .tally
        .total
        .checked_add(collected)
        .ok_or(PresaleError::MathOverflow)?;
    if collected == 0 {
        emit!(FeesCranked {
            presale: ctx.accounts.presale.key(),
            collected: 0,
            acc_quote_per_token: ctx.accounts.presale.acc_quote_per_token,
        });
        return Ok(());
    }

    if ctx.accounts.presale.fee_mode == FeeMode::Dev {
        let (creator_share, platform_share) = creator_platform_split(collected)?;
        pay_quote(
            &ctx.accounts.presale,
            &ctx.accounts.platform_wallet.to_account_info(),
            &ctx.accounts.platform_quote_ata.to_account_info(),
            ctx.remaining_accounts,
            platform_share,
            &seeds,
        )?;
        pay_quote(
            &ctx.accounts.presale,
            &ctx.accounts.dev_wallet.to_account_info(),
            &ctx.remaining_accounts[crank_idx::DEV_QUOTE_ATA],
            ctx.remaining_accounts,
            creator_share,
            &seeds,
        )?;
    } else {
        let supply = pump::mint_supply(&ctx.accounts.base_mint.to_account_info())?;
        let pool_balance = pump::token_amount(&ctx.accounts.pool_base_ata.to_account_info())?;
        let vault_balance = pump::token_amount(&ctx.accounts.presale_base_ata.to_account_info())?;
        fold_holder_fees(&mut ctx.accounts.presale, supply, pool_balance, vault_balance, collected)?;
    }

    emit!(FeesCranked {
        presale: ctx.accounts.presale.key(),
        collected,
        acc_quote_per_token: ctx.accounts.presale.acc_quote_per_token,
    });
    Ok(())
}

/// Creator receives the remainder. `PLATFORM_FEE_BPS` is the platform share.
fn creator_platform_split(collected: u64) -> Result<(u64, u64)> {
    let platform = u64::try_from(
        (collected as u128)
            .checked_mul(PLATFORM_FEE_BPS as u128)
            .ok_or(PresaleError::MathOverflow)?
            / 10_000,
    )
    .map_err(|_| error!(PresaleError::MathOverflow))?;
    let creator = collected
        .checked_sub(platform)
        .ok_or(PresaleError::MathOverflow)?;
    Ok((creator, platform))
}

fn pay_quote<'info>(
    presale: &Account<'info, Presale>,
    recipient: &AccountInfo<'info>,
    recipient_quote_ata: &AccountInfo<'info>,
    remaining: &[AccountInfo<'info>],
    amount: u64,
    seeds: &[&[u8]],
) -> Result<()> {
    if amount == 0 {
        return Ok(());
    }
    if presale.quote_mint == WSOL_MINT {
        let presale_ai = presale.to_account_info();
        let rent = Rent::get()?.minimum_balance(presale_ai.data_len());
        require!(
            presale_ai.lamports().saturating_sub(amount) >= rent,
            PresaleError::InsufficientVault
        );
        **presale_ai.try_borrow_mut_lamports()? -= amount;
        **recipient.try_borrow_mut_lamports()? += amount;
        return Ok(());
    }
    require!(remaining.len() == crank_idx::LEN, PresaleError::BadAccount);
    let quote_mint = &remaining[crank_idx::QUOTE_MINT];
    let from = &remaining[crank_idx::PRESALE_QUOTE_ATA];
    let token_program = &remaining[crank_idx::QUOTE_TOKEN_PROGRAM];
    require_keys_eq!(
        *recipient_quote_ata.key,
        pump::ata(
            recipient.key,
            &presale.quote_mint,
            &presale.quote_token_program
        ),
        PresaleError::BadAccount
    );
    let to = recipient_quote_ata.clone();
    token_interface::transfer_checked(
        CpiContext::new_with_signer(
            token_program.clone(),
            TransferChecked {
                from: from.clone(),
                mint: quote_mint.clone(),
                to: to.clone(),
                authority: presale.to_account_info(),
            },
            &[seeds],
        ),
        amount,
        pump::mint_decimals(quote_mint)?,
    )?;
    Ok(())
}

fn fold_holder_fees(
    presale: &mut Presale,
    supply: u64,
    pool_balance: u64,
    vault_balance: u64,
    collected: u64,
) -> Result<()> {
    presale.undistributed_fees = presale
        .undistributed_fees
        .checked_add(collected)
        .ok_or(PresaleError::MathOverflow)?;
    let eligible = supply.saturating_sub(pool_balance).saturating_sub(vault_balance);
    if eligible == 0 || presale.undistributed_fees == 0 {
        return Ok(());
    }
    let add = (presale.undistributed_fees as u128)
        .checked_mul(REWARD_SCALE)
        .ok_or(PresaleError::MathOverflow)?
        / (eligible as u128);
    if add == 0 {
        return Ok(());
    }
    presale.acc_quote_per_token = presale
        .acc_quote_per_token
        .checked_add(add)
        .ok_or(PresaleError::MathOverflow)?;
    let consumed = add
        .checked_mul(eligible as u128)
        .ok_or(PresaleError::MathOverflow)?
        / REWARD_SCALE;
    presale.undistributed_fees = presale
        .undistributed_fees
        .checked_sub(u64::try_from(consumed).map_err(|_| PresaleError::MathOverflow)?)
        .ok_or(PresaleError::MathOverflow)?;
    Ok(())
}

#[derive(Accounts)]
pub struct ClaimHolderFees<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,
    #[account(
        mut,
        constraint = presale.status == Status::Migrated @ PresaleError::NotMigrated,
        constraint = presale.fee_mode == FeeMode::Holders @ PresaleError::BadFeeMode
    )]
    pub presale: Box<Account<'info, Presale>>,
    /// CHECK: current holder. Does not sign; the payout is sent to them.
    #[account(mut)]
    pub owner: UncheckedAccount<'info>,
    #[account(
        init_if_needed,
        payer = payer,
        space = 8 + HolderReward::INIT_SPACE,
        seeds = [b"holder-reward", presale.key().as_ref(), owner.key().as_ref()],
        bump
    )]
    pub reward: Account<'info, HolderReward>,
    #[account(constraint = mint.key() == mint_pda(&presale.key()).0 @ PresaleError::BadAccount)]
    pub mint: InterfaceAccount<'info, Mint>,
    #[account(
        associated_token::mint = mint,
        associated_token::authority = owner,
        associated_token::token_program = token_program
    )]
    pub owner_base_ata: InterfaceAccount<'info, TokenAccount>,
    pub token_program: Interface<'info, TokenInterface>,
    pub system_program: Program<'info, System>,
    pub quote_mint: Option<InterfaceAccount<'info, Mint>>,
    pub quote_token_program: Option<Interface<'info, TokenInterface>>,
    #[account(mut)]
    pub presale_quote_ata: Option<InterfaceAccount<'info, TokenAccount>>,
    #[account(mut)]
    pub owner_quote_ata: Option<InterfaceAccount<'info, TokenAccount>>,
}

pub fn claim_holder_fees(ctx: Context<ClaimHolderFees>) -> Result<()> {
    if ctx.accounts.reward.owner == Pubkey::default() {
        ctx.accounts.reward.presale = ctx.accounts.presale.key();
        ctx.accounts.reward.owner = ctx.accounts.owner.key();
        ctx.accounts.reward.bump = ctx.bumps.reward;
        ctx.accounts.reward.reward_debt = 0;
    }
    require_keys_eq!(
        ctx.accounts.reward.owner,
        ctx.accounts.owner.key(),
        PresaleError::BadAccount
    );
    require_keys_eq!(
        ctx.accounts.reward.presale,
        ctx.accounts.presale.key(),
        PresaleError::BadAccount
    );

    let balance = ctx.accounts.owner_base_ata.amount as u128;
    let accumulated = balance
        .checked_mul(ctx.accounts.presale.acc_quote_per_token)
        .ok_or(PresaleError::MathOverflow)?
        / REWARD_SCALE;
    require!(
        accumulated >= ctx.accounts.reward.reward_debt,
        PresaleError::MathOverflow
    );
    let pay = accumulated - ctx.accounts.reward.reward_debt;
    ctx.accounts.reward.reward_debt = accumulated;
    let pay = u64::try_from(pay).map_err(|_| PresaleError::MathOverflow)?;
    if pay == 0 {
        return Ok(());
    }

    let seed_state = PresaleSeeds::load(&ctx.accounts.presale);
    let seeds = seed_state.slices();
    if ctx.accounts.presale.quote_mint == WSOL_MINT {
        let presale_ai = ctx.accounts.presale.to_account_info();
        let rent = Rent::get()?.minimum_balance(presale_ai.data_len());
        require!(
            presale_ai.lamports().saturating_sub(pay) >= rent,
            PresaleError::InsufficientVault
        );
        system_program::transfer(
            CpiContext::new_with_signer(
                ctx.accounts.system_program.to_account_info(),
                Transfer {
                    from: presale_ai,
                    to: ctx.accounts.owner.to_account_info(),
                },
                &[&seeds],
            ),
            pay,
        )?;
    } else {
        let quote_mint = ctx.accounts.quote_mint.as_ref().ok_or(PresaleError::BadAccount)?;
        let from = ctx.accounts.presale_quote_ata.as_ref().ok_or(PresaleError::BadAccount)?;
        let to = ctx.accounts.owner_quote_ata.as_ref().ok_or(PresaleError::BadAccount)?;
        let token_program = ctx.accounts.quote_token_program.as_ref().ok_or(PresaleError::BadAccount)?;
        require_keys_eq!(quote_mint.key(), ctx.accounts.presale.quote_mint, PresaleError::BadAccount);
        require_keys_eq!(from.owner, ctx.accounts.presale.key(), PresaleError::BadAccount);
        require_keys_eq!(to.owner, ctx.accounts.owner.key(), PresaleError::BadAccount);
        require!(from.amount >= pay, PresaleError::InsufficientVault);
        token_interface::transfer_checked(
            CpiContext::new_with_signer(
                token_program.to_account_info(),
                TransferChecked {
                    from: from.to_account_info(),
                    mint: quote_mint.to_account_info(),
                    to: to.to_account_info(),
                    authority: ctx.accounts.presale.to_account_info(),
                },
                &[&seeds],
            ),
            pay,
            quote_mint.decimals,
        )?;
    }

    emit!(HolderFeesClaimed {
        presale: ctx.accounts.presale.key(),
        owner: ctx.accounts.owner.key(),
        amount: pay,
    });
    Ok(())
}

#[cfg(test)]
mod split_tests {
    use super::creator_platform_split;

    #[test]
    fn platform_keeps_thirty_percent_and_dust_stays_with_the_creator() {
        assert_eq!(creator_platform_split(1_000).unwrap(), (700, 300));
        assert_eq!(creator_platform_split(10).unwrap(), (7, 3));
        assert_eq!(creator_platform_split(1).unwrap(), (1, 0));
    }

    #[test]
    fn an_amm_owned_pool_counts_as_already_migrated() {
        let amm = crate::constants::PUMP_AMM_PROGRAM_ID;
        let system = anchor_lang::solana_program::system_program::ID;
        assert!(crate::pump::amm_pool_open(&amm, 8));
        assert!(crate::pump::amm_pool_open(&amm, 300));
        assert!(!crate::pump::amm_pool_open(&amm, 0));
        assert!(!crate::pump::amm_pool_open(&amm, 7));
        assert!(!crate::pump::amm_pool_open(&system, 300));
    }
}
