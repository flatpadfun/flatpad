use anchor_lang::prelude::*;
use anchor_lang::solana_program::{instruction::Instruction, program::invoke_signed};
use anchor_spl::associated_token::spl_associated_token_account::instruction::create_associated_token_account_idempotent;

use crate::constants::*;
use crate::error::PresaleError;

pub fn pump_pda(seeds: &[&[u8]]) -> (Pubkey, u8) {
    Pubkey::find_program_address(seeds, &PUMP_PROGRAM_ID)
}

pub fn amm_pda(seeds: &[&[u8]]) -> (Pubkey, u8) {
    Pubkey::find_program_address(seeds, &PUMP_AMM_PROGRAM_ID)
}

pub fn fee_pda(seeds: &[&[u8]]) -> (Pubkey, u8) {
    Pubkey::find_program_address(seeds, &PUMP_FEE_PROGRAM_ID)
}

pub fn mayhem_pda(seeds: &[&[u8]]) -> (Pubkey, u8) {
    Pubkey::find_program_address(seeds, &MAYHEM_PROGRAM_ID)
}

pub fn ata(owner: &Pubkey, mint: &Pubkey, token_program: &Pubkey) -> Pubkey {
    anchor_spl::associated_token::get_associated_token_address_with_program_id(
        owner,
        mint,
        token_program,
    )
}

fn meta(key: Pubkey, is_signer: bool, is_writable: bool) -> AccountMeta {
    AccountMeta {
        pubkey: key,
        is_signer,
        is_writable,
    }
}

fn push_str(buf: &mut Vec<u8>, value: &str) {
    buf.extend_from_slice(&(value.len() as u32).to_le_bytes());
    buf.extend_from_slice(value.as_bytes());
}

pub struct FillInputs<'a, 'info, 'data> {
    pub remaining: &'a [AccountInfo<'info>],
    pub cranker: AccountInfo<'info>,
    pub presale: AccountInfo<'info>,
    pub system_program: AccountInfo<'info>,
    pub presale_seeds: &'data [&'data [u8]],
    pub buyer: AccountInfo<'info>,
    pub buyer_seeds: &'data [&'data [u8]],
    pub mint_seeds: &'data [&'data [u8]],
    pub name: &'data str,
    pub symbol: &'data str,
    pub uri: &'data str,
    pub creator: Pubkey,
    pub spendable_quote_in: u64,
    pub min_tokens_out: u64,
}

fn acc<'a, 'info>(
    remaining: &'a [AccountInfo<'info>],
    index: usize,
) -> Result<&'a AccountInfo<'info>> {
    remaining
        .get(index)
        .ok_or_else(|| error!(PresaleError::BadAccount))
}

fn key(remaining: &[AccountInfo], index: usize) -> Result<Pubkey> {
    Ok(*acc(remaining, index)?.key)
}

pub fn require_fill_pdas(mint: &Pubkey, presale: &Pubkey, quote_mint: &Pubkey, remaining: &[AccountInfo]) -> Result<()> {
    require!(remaining.len() == fill_idx::LEN, PresaleError::BadAccount);
    let (expect_mint_authority, _) = pump_pda(&[b"mint-authority"]);
    let (bonding_curve, _) = pump_pda(&[b"bonding-curve", mint.as_ref()]);
    let (global, _) = pump_pda(&[b"global"]);
    let (event_authority, _) = pump_pda(&[b"__event_authority"]);
    let (creator_vault, _) = pump_pda(&[b"creator-vault", presale.as_ref()]);
    let (global_volume, _) = pump_pda(&[b"global_volume_accumulator"]);
    let (buyer, _) = Pubkey::find_program_address(&[b"buyer", presale.as_ref()], &crate::ID);
    let (user_volume, _) = pump_pda(&[b"user_volume_accumulator", buyer.as_ref()]);
    let (fee_config, _) = fee_pda(&[b"fee_config", PUMP_PROGRAM_ID.as_ref()]);
    let (sharing, _) = fee_pda(&[b"sharing-config", mint.as_ref()]);
    let (pool_authority, _) = pump_pda(&[b"pool-authority", mint.as_ref()]);
    let (pool, _) = amm_pda(&[
        b"pool",
        &0u16.to_le_bytes(),
        pool_authority.as_ref(),
        mint.as_ref(),
        quote_mint.as_ref(),
    ]);
    let (lp_mint, _) = amm_pda(&[b"pool_lp_mint", pool.as_ref()]);
    let (amm_global, _) = amm_pda(&[b"global_config"]);
    let (amm_event, _) = amm_pda(&[b"__event_authority"]);
    let (mayhem_params, _) = mayhem_pda(&[b"global-params"]);
    let (sol_vault, _) = mayhem_pda(&[b"sol-vault"]);
    let (mayhem_state, _) = mayhem_pda(&[b"mayhem-state", mint.as_ref()]);

    require_keys_eq!(key(remaining, fill_idx::MINT)?, *mint, PresaleError::BadAccount);
    require_keys_eq!(key(remaining, fill_idx::GLOBAL)?, global, PresaleError::BadAccount);
    require_keys_eq!(key(remaining, fill_idx::FEE_CONFIG)?, fee_config, PresaleError::BadAccount);
    require_keys_eq!(key(remaining, fill_idx::QUOTE_MINT)?, *quote_mint, PresaleError::BadAccount);
    require_keys_eq!(key(remaining, fill_idx::BASE_TOKEN_PROGRAM)?, TOKEN_2022_PROGRAM_ID, PresaleError::BadAccount);
    require_keys_eq!(key(remaining, fill_idx::PUMP_PROGRAM)?, PUMP_PROGRAM_ID, PresaleError::BadAccount);
    require_keys_eq!(key(remaining, fill_idx::PUMP_EVENT_AUTHORITY)?, event_authority, PresaleError::BadAccount);
    require_keys_eq!(key(remaining, fill_idx::MAYHEM_PROGRAM)?, MAYHEM_PROGRAM_ID, PresaleError::BadAccount);
    require_keys_eq!(key(remaining, fill_idx::MAYHEM_GLOBAL_PARAMS)?, mayhem_params, PresaleError::BadAccount);
    require_keys_eq!(key(remaining, fill_idx::MAYHEM_SOL_VAULT)?, sol_vault, PresaleError::BadAccount);
    require_keys_eq!(key(remaining, fill_idx::MAYHEM_STATE)?, mayhem_state, PresaleError::BadAccount);
    require_keys_eq!(
        key(remaining, fill_idx::MAYHEM_TOKEN_VAULT)?,
        ata(&sol_vault, mint, &TOKEN_2022_PROGRAM_ID),
        PresaleError::BadAccount
    );
    require_keys_eq!(key(remaining, fill_idx::MINT_AUTHORITY)?, expect_mint_authority, PresaleError::BadAccount);
    require_keys_eq!(key(remaining, fill_idx::BONDING_CURVE)?, bonding_curve, PresaleError::BadAccount);
    require_keys_eq!(
        key(remaining, fill_idx::ASSOCIATED_BASE_BONDING_CURVE)?,
        ata(&bonding_curve, mint, &TOKEN_2022_PROGRAM_ID),
        PresaleError::BadAccount
    );
    let quote_program = key(remaining, fill_idx::QUOTE_TOKEN_PROGRAM)?;
    require_keys_eq!(
        key(remaining, fill_idx::ASSOCIATED_QUOTE_BONDING_CURVE)?,
        ata(&bonding_curve, quote_mint, &quote_program),
        PresaleError::BadAccount
    );
    require_keys_eq!(
        key(remaining, fill_idx::PRESALE_BASE_ATA)?,
        ata(presale, mint, &TOKEN_2022_PROGRAM_ID),
        PresaleError::BadAccount
    );
    require_keys_eq!(
        key(remaining, fill_idx::PRESALE_QUOTE_ATA)?,
        ata(presale, quote_mint, &quote_program),
        PresaleError::BadAccount
    );
    require_keys_eq!(key(remaining, fill_idx::BUYER)?, buyer, PresaleError::BadAccount);
    require_keys_eq!(
        key(remaining, fill_idx::BUYER_BASE_ATA)?,
        ata(&buyer, mint, &TOKEN_2022_PROGRAM_ID),
        PresaleError::BadAccount
    );
    require_keys_eq!(
        key(remaining, fill_idx::BUYER_QUOTE_ATA)?,
        ata(&buyer, quote_mint, &quote_program),
        PresaleError::BadAccount
    );
    require_keys_eq!(key(remaining, fill_idx::CREATOR_VAULT)?, creator_vault, PresaleError::BadAccount);
    require_keys_eq!(
        key(remaining, fill_idx::ASSOCIATED_CREATOR_VAULT)?,
        ata(&creator_vault, quote_mint, &quote_program),
        PresaleError::BadAccount
    );
    require_keys_eq!(key(remaining, fill_idx::SHARING_CONFIG)?, sharing, PresaleError::BadAccount);
    require_keys_eq!(key(remaining, fill_idx::GLOBAL_VOLUME_ACCUMULATOR)?, global_volume, PresaleError::BadAccount);
    require_keys_eq!(key(remaining, fill_idx::USER_VOLUME_ACCUMULATOR)?, user_volume, PresaleError::BadAccount);
    require_keys_eq!(
        key(remaining, fill_idx::ASSOCIATED_USER_VOLUME_ACCUMULATOR)?,
        ata(&user_volume, quote_mint, &quote_program),
        PresaleError::BadAccount
    );
    require_keys_eq!(key(remaining, fill_idx::FEE_PROGRAM)?, PUMP_FEE_PROGRAM_ID, PresaleError::BadAccount);
    require_keys_eq!(key(remaining, fill_idx::PUMP_AMM_PROGRAM)?, PUMP_AMM_PROGRAM_ID, PresaleError::BadAccount);
    require_keys_eq!(key(remaining, fill_idx::POOL_AUTHORITY)?, pool_authority, PresaleError::BadAccount);
    require_keys_eq!(key(remaining, fill_idx::POOL)?, pool, PresaleError::BadAccount);
    require_keys_eq!(
        key(remaining, fill_idx::POOL_AUTHORITY_MINT_ACCOUNT)?,
        ata(&pool_authority, mint, &TOKEN_2022_PROGRAM_ID),
        PresaleError::BadAccount
    );
    require_keys_eq!(
        key(remaining, fill_idx::POOL_AUTHORITY_QUOTE_ACCOUNT)?,
        ata(&pool_authority, quote_mint, &quote_program),
        PresaleError::BadAccount
    );
    require_keys_eq!(key(remaining, fill_idx::AMM_GLOBAL_CONFIG)?, amm_global, PresaleError::BadAccount);
    require_keys_eq!(key(remaining, fill_idx::LP_MINT)?, lp_mint, PresaleError::BadAccount);
    require_keys_eq!(
        key(remaining, fill_idx::USER_POOL_TOKEN_ACCOUNT)?,
        ata(&pool_authority, &lp_mint, &TOKEN_2022_PROGRAM_ID),
        PresaleError::BadAccount
    );
    require_keys_eq!(
        key(remaining, fill_idx::POOL_BASE_TOKEN_ACCOUNT)?,
        ata(&pool, mint, &TOKEN_2022_PROGRAM_ID),
        PresaleError::BadAccount
    );
    require_keys_eq!(
        key(remaining, fill_idx::POOL_QUOTE_TOKEN_ACCOUNT)?,
        ata(&pool, quote_mint, &quote_program),
        PresaleError::BadAccount
    );
    let (boost_vault_authority, _) = amm_pda(&[b"boost_vault", pool.as_ref()]);
    require_keys_eq!(
        key(remaining, fill_idx::BOOST_VAULT_AUTHORITY)?,
        boost_vault_authority,
        PresaleError::BadAccount
    );
    require_keys_eq!(
        key(remaining, fill_idx::BOOST_VAULT)?,
        ata(&boost_vault_authority, quote_mint, &quote_program),
        PresaleError::BadAccount
    );
    require_keys_eq!(key(remaining, fill_idx::PUMP_AMM_EVENT_AUTHORITY)?, amm_event, PresaleError::BadAccount);
    require_keys_eq!(
        key(remaining, fill_idx::RENT)?,
        anchor_lang::solana_program::sysvar::rent::ID,
        PresaleError::BadAccount
    );
    let _ = presale;
    Ok(())
}

fn ix(program: Pubkey, accounts: Vec<AccountMeta>, data: Vec<u8>) -> Instruction {
    Instruction {
        program_id: program,
        accounts,
        data,
    }
}

fn infos<'a>(list: &[&AccountInfo<'a>]) -> Vec<AccountInfo<'a>> {
    list.iter().map(|a| (*a).clone()).collect()
}

pub fn create_v2(input: &FillInputs<'_, '_, '_>) -> Result<()> {
    let r = input.remaining;
    let data = {
        let mut data = CREATE_V2_DISC.to_vec();
        push_str(&mut data, input.name);
        push_str(&mut data, input.symbol);
        push_str(&mut data, input.uri);
        data.extend_from_slice(input.creator.as_ref());
        data.push(0); // is_mayhem_mode
        data.push(0); // is_cashback_enabled = false
        data.extend_from_slice(&0u64.to_le_bytes()); // creator_fee_bps
        data.push(0); // is_holder_reward
        data
    };
    let accounts = vec![
        meta(key(r, fill_idx::MINT)?, true, true),
        meta(key(r, fill_idx::MINT_AUTHORITY)?, false, false),
        meta(key(r, fill_idx::BONDING_CURVE)?, false, true),
        meta(key(r, fill_idx::ASSOCIATED_BASE_BONDING_CURVE)?, false, true),
        meta(key(r, fill_idx::GLOBAL)?, false, false),
        meta(input.cranker.key(), true, true),
        meta(input.system_program.key(), false, false),
        meta(key(r, fill_idx::BASE_TOKEN_PROGRAM)?, false, false),
        meta(key(r, fill_idx::ASSOCIATED_TOKEN_PROGRAM)?, false, false),
        meta(key(r, fill_idx::MAYHEM_PROGRAM)?, false, true),
        meta(key(r, fill_idx::MAYHEM_GLOBAL_PARAMS)?, false, false),
        meta(key(r, fill_idx::MAYHEM_SOL_VAULT)?, false, true),
        meta(key(r, fill_idx::MAYHEM_STATE)?, false, true),
        meta(key(r, fill_idx::MAYHEM_TOKEN_VAULT)?, false, true),
        meta(key(r, fill_idx::PUMP_EVENT_AUTHORITY)?, false, false),
        meta(key(r, fill_idx::PUMP_PROGRAM)?, false, false),
        meta(key(r, fill_idx::QUOTE_MINT)?, false, false),
        meta(key(r, fill_idx::ASSOCIATED_QUOTE_BONDING_CURVE)?, false, true),
        meta(key(r, fill_idx::QUOTE_TOKEN_PROGRAM)?, false, false),
    ];
    let account_infos = infos(&[
        acc(r, fill_idx::MINT)?,
        acc(r, fill_idx::MINT_AUTHORITY)?,
        acc(r, fill_idx::BONDING_CURVE)?,
        acc(r, fill_idx::ASSOCIATED_BASE_BONDING_CURVE)?,
        acc(r, fill_idx::GLOBAL)?,
        &input.cranker,
        &input.system_program,
        acc(r, fill_idx::BASE_TOKEN_PROGRAM)?,
        acc(r, fill_idx::ASSOCIATED_TOKEN_PROGRAM)?,
        acc(r, fill_idx::MAYHEM_PROGRAM)?,
        acc(r, fill_idx::MAYHEM_GLOBAL_PARAMS)?,
        acc(r, fill_idx::MAYHEM_SOL_VAULT)?,
        acc(r, fill_idx::MAYHEM_STATE)?,
        acc(r, fill_idx::MAYHEM_TOKEN_VAULT)?,
        acc(r, fill_idx::PUMP_EVENT_AUTHORITY)?,
        acc(r, fill_idx::PUMP_PROGRAM)?,
        acc(r, fill_idx::QUOTE_MINT)?,
        acc(r, fill_idx::ASSOCIATED_QUOTE_BONDING_CURVE)?,
        acc(r, fill_idx::QUOTE_TOKEN_PROGRAM)?,
    ]);
    invoke_signed(
        &ix(PUMP_PROGRAM_ID, accounts, data),
        &account_infos,
        &[input.mint_seeds],
    )
    .map_err(|e| e.into())
}

/// Pump's buy expects the buyer's base token account to exist. Claims read this
/// same account, so fill does not open a second one.
pub fn create_buyer_atas(input: &FillInputs<'_, '_, '_>) -> Result<()> {
    create_ata(
        input,
        &input.buyer,
        fill_idx::BUYER_BASE_ATA,
        fill_idx::MINT,
        fill_idx::BASE_TOKEN_PROGRAM,
    )
}

fn create_ata<'info>(
    input: &FillInputs<'_, 'info, '_>,
    owner: &AccountInfo<'info>,
    ata_index: usize,
    mint_index: usize,
    token_program_index: usize,
) -> Result<()> {
    let r = input.remaining;
    let instruction = create_associated_token_account_idempotent(
        input.cranker.key,
        owner.key,
        acc(r, mint_index)?.key,
        acc(r, token_program_index)?.key,
    );
    let account_infos = infos(&[
        &input.cranker,
        acc(r, ata_index)?,
        owner,
        acc(r, mint_index)?,
        &input.system_program,
        acc(r, token_program_index)?,
        acc(r, fill_idx::ASSOCIATED_TOKEN_PROGRAM)?,
    ]);
    invoke_signed(&instruction, &account_infos, &[]).map_err(|e| e.into())
}

/// Buys exactly the curve's real token reserves. `buy_exact_quote_in_v2` cannot
/// finish this curve: the last lamport of quote jumps from just under the real
/// reserves to just over them, and Pump rejects the overshoot.
pub fn buy_v2(input: &FillInputs<'_, '_, '_>) -> Result<()> {
    let r = input.remaining;
    let mut data = BUY_V2_DISC.to_vec();
    data.extend_from_slice(&input.min_tokens_out.to_le_bytes());
    data.extend_from_slice(&input.spendable_quote_in.to_le_bytes());
    let accounts = vec![
        meta(key(r, fill_idx::GLOBAL)?, false, false),
        meta(key(r, fill_idx::MINT)?, false, false),
        meta(key(r, fill_idx::QUOTE_MINT)?, false, false),
        meta(key(r, fill_idx::BASE_TOKEN_PROGRAM)?, false, false),
        meta(key(r, fill_idx::QUOTE_TOKEN_PROGRAM)?, false, false),
        meta(key(r, fill_idx::ASSOCIATED_TOKEN_PROGRAM)?, false, false),
        meta(key(r, fill_idx::FEE_RECIPIENT)?, false, true),
        meta(key(r, fill_idx::ASSOCIATED_QUOTE_FEE_RECIPIENT)?, false, true),
        meta(key(r, fill_idx::BUYBACK_FEE_RECIPIENT)?, false, true),
        meta(key(r, fill_idx::ASSOCIATED_QUOTE_BUYBACK_FEE_RECIPIENT)?, false, true),
        meta(key(r, fill_idx::BONDING_CURVE)?, false, true),
        meta(key(r, fill_idx::ASSOCIATED_BASE_BONDING_CURVE)?, false, true),
        meta(key(r, fill_idx::ASSOCIATED_QUOTE_BONDING_CURVE)?, false, true),
        meta(input.buyer.key(), true, true),
        meta(key(r, fill_idx::BUYER_BASE_ATA)?, false, true),
        meta(key(r, fill_idx::BUYER_QUOTE_ATA)?, false, true),
        meta(key(r, fill_idx::CREATOR_VAULT)?, false, true),
        meta(key(r, fill_idx::ASSOCIATED_CREATOR_VAULT)?, false, true),
        meta(key(r, fill_idx::SHARING_CONFIG)?, false, false),
        meta(key(r, fill_idx::GLOBAL_VOLUME_ACCUMULATOR)?, false, false),
        meta(key(r, fill_idx::USER_VOLUME_ACCUMULATOR)?, false, true),
        meta(key(r, fill_idx::ASSOCIATED_USER_VOLUME_ACCUMULATOR)?, false, true),
        meta(key(r, fill_idx::FEE_CONFIG)?, false, false),
        meta(key(r, fill_idx::FEE_PROGRAM)?, false, false),
        meta(input.system_program.key(), false, false),
        meta(key(r, fill_idx::PUMP_EVENT_AUTHORITY)?, false, false),
        meta(key(r, fill_idx::PUMP_PROGRAM)?, false, false),
    ];
    let account_infos = infos(&[
        acc(r, fill_idx::GLOBAL)?,
        acc(r, fill_idx::MINT)?,
        acc(r, fill_idx::QUOTE_MINT)?,
        acc(r, fill_idx::BASE_TOKEN_PROGRAM)?,
        acc(r, fill_idx::QUOTE_TOKEN_PROGRAM)?,
        acc(r, fill_idx::ASSOCIATED_TOKEN_PROGRAM)?,
        acc(r, fill_idx::FEE_RECIPIENT)?,
        acc(r, fill_idx::ASSOCIATED_QUOTE_FEE_RECIPIENT)?,
        acc(r, fill_idx::BUYBACK_FEE_RECIPIENT)?,
        acc(r, fill_idx::ASSOCIATED_QUOTE_BUYBACK_FEE_RECIPIENT)?,
        acc(r, fill_idx::BONDING_CURVE)?,
        acc(r, fill_idx::ASSOCIATED_BASE_BONDING_CURVE)?,
        acc(r, fill_idx::ASSOCIATED_QUOTE_BONDING_CURVE)?,
        &input.buyer,
        acc(r, fill_idx::BUYER_BASE_ATA)?,
        acc(r, fill_idx::BUYER_QUOTE_ATA)?,
        acc(r, fill_idx::CREATOR_VAULT)?,
        acc(r, fill_idx::ASSOCIATED_CREATOR_VAULT)?,
        acc(r, fill_idx::SHARING_CONFIG)?,
        acc(r, fill_idx::GLOBAL_VOLUME_ACCUMULATOR)?,
        acc(r, fill_idx::USER_VOLUME_ACCUMULATOR)?,
        acc(r, fill_idx::ASSOCIATED_USER_VOLUME_ACCUMULATOR)?,
        acc(r, fill_idx::FEE_CONFIG)?,
        acc(r, fill_idx::FEE_PROGRAM)?,
        &input.system_program,
        acc(r, fill_idx::PUMP_EVENT_AUTHORITY)?,
        acc(r, fill_idx::PUMP_PROGRAM)?,
    ]);
    invoke_signed(
        &ix(PUMP_PROGRAM_ID, accounts, data),
        &account_infos,
        &[input.buyer_seeds],
    )
    .map_err(|e| e.into())
}

/// Non-SOL quotes are spent from the buyer's quote account, not the presale vault.
pub fn move_quote_to_buyer(input: &FillInputs<'_, '_, '_>) -> Result<()> {
    let r = input.remaining;
    if key(r, fill_idx::QUOTE_MINT)? == WSOL_MINT {
        return Ok(());
    }
    let decimals = mint_decimals(acc(r, fill_idx::QUOTE_MINT)?)?;
    anchor_spl::token_interface::transfer_checked(
        CpiContext::new_with_signer(
            acc(r, fill_idx::QUOTE_TOKEN_PROGRAM)?.clone(),
            anchor_spl::token_interface::TransferChecked {
                from: acc(r, fill_idx::PRESALE_QUOTE_ATA)?.clone(),
                mint: acc(r, fill_idx::QUOTE_MINT)?.clone(),
                to: acc(r, fill_idx::BUYER_QUOTE_ATA)?.clone(),
                authority: input.presale.clone(),
            },
            &[input.presale_seeds],
        ),
        input.spendable_quote_in,
        decimals,
    )
}

/// Claim reads the presale base account, so the bought tokens move there.
pub fn pull_tokens_to_presale(input: &FillInputs<'_, '_, '_>) -> Result<()> {
    let r = input.remaining;
    let from = acc(r, fill_idx::BUYER_BASE_ATA)?;
    let amount = token_amount(from)?;
    let decimals = mint_decimals(acc(r, fill_idx::MINT)?)?;
    let token_program = acc(r, fill_idx::BASE_TOKEN_PROGRAM)?.clone();
    anchor_spl::token_2022::transfer_checked(
        CpiContext::new_with_signer(
            token_program.clone(),
            anchor_spl::token_2022::TransferChecked {
                from: from.clone(),
                mint: acc(r, fill_idx::MINT)?.clone(),
                to: acc(r, fill_idx::PRESALE_BASE_ATA)?.clone(),
                authority: input.buyer.clone(),
            },
            &[input.buyer_seeds],
        ),
        amount,
        decimals,
    )?;
    anchor_spl::token_2022::close_account(CpiContext::new_with_signer(
        token_program,
        anchor_spl::token_2022::CloseAccount {
            account: from.clone(),
            destination: input.cranker.clone(),
            authority: input.buyer.clone(),
        },
        &[input.buyer_seeds],
    ))?;
    let leftover = input.buyer.lamports();
    if leftover == 0 {
        return Ok(());
    }
    invoke_signed(
        &anchor_lang::solana_program::system_instruction::transfer(
            input.buyer.key,
            input.presale.key,
            leftover,
        ),
        &infos(&[&input.buyer, &input.presale, &input.system_program]),
        &[input.buyer_seeds],
    )
    .map_err(|e| e.into())
}

pub fn migrate_v2(input: &FillInputs<'_, '_, '_>) -> Result<()> {
    let r = input.remaining;
    let accounts = vec![
        meta(key(r, fill_idx::GLOBAL)?, false, false),
        meta(key(r, fill_idx::WITHDRAW_AUTHORITY)?, false, true),
        meta(key(r, fill_idx::MINT)?, false, false),
        meta(key(r, fill_idx::QUOTE_MINT)?, false, false),
        meta(key(r, fill_idx::BONDING_CURVE)?, false, true),
        meta(key(r, fill_idx::ASSOCIATED_BASE_BONDING_CURVE)?, false, true),
        meta(key(r, fill_idx::ASSOCIATED_QUOTE_BONDING_CURVE)?, false, true),
        meta(input.cranker.key(), true, true),
        meta(input.system_program.key(), false, false),
        meta(key(r, fill_idx::PUMP_AMM_PROGRAM)?, false, false),
        meta(key(r, fill_idx::POOL)?, false, true),
        meta(key(r, fill_idx::POOL_AUTHORITY)?, false, true),
        meta(key(r, fill_idx::POOL_AUTHORITY_MINT_ACCOUNT)?, false, true),
        meta(key(r, fill_idx::POOL_AUTHORITY_QUOTE_ACCOUNT)?, false, true),
        meta(key(r, fill_idx::AMM_GLOBAL_CONFIG)?, false, false),
        meta(key(r, fill_idx::LP_MINT)?, false, true),
        meta(key(r, fill_idx::USER_POOL_TOKEN_ACCOUNT)?, false, true),
        meta(key(r, fill_idx::POOL_BASE_TOKEN_ACCOUNT)?, false, true),
        meta(key(r, fill_idx::POOL_QUOTE_TOKEN_ACCOUNT)?, false, true),
        meta(key(r, fill_idx::BASE_TOKEN_PROGRAM)?, false, false),
        meta(key(r, fill_idx::QUOTE_TOKEN_PROGRAM)?, false, false),
        meta(key(r, fill_idx::BASE_TOKEN_PROGRAM)?, false, false),
        meta(key(r, fill_idx::ASSOCIATED_TOKEN_PROGRAM)?, false, false),
        meta(key(r, fill_idx::PUMP_AMM_EVENT_AUTHORITY)?, false, false),
        meta(key(r, fill_idx::RENT)?, false, false),
        meta(key(r, fill_idx::PUMP_EVENT_AUTHORITY)?, false, false),
        meta(key(r, fill_idx::PUMP_PROGRAM)?, false, false),
        meta(key(r, fill_idx::BOOST_VAULT_AUTHORITY)?, false, false),
        meta(key(r, fill_idx::BOOST_VAULT)?, false, true),
    ];
    let account_infos = infos(&[
        acc(r, fill_idx::GLOBAL)?,
        acc(r, fill_idx::WITHDRAW_AUTHORITY)?,
        acc(r, fill_idx::MINT)?,
        acc(r, fill_idx::QUOTE_MINT)?,
        acc(r, fill_idx::BONDING_CURVE)?,
        acc(r, fill_idx::ASSOCIATED_BASE_BONDING_CURVE)?,
        acc(r, fill_idx::ASSOCIATED_QUOTE_BONDING_CURVE)?,
        &input.cranker,
        &input.system_program,
        acc(r, fill_idx::PUMP_AMM_PROGRAM)?,
        acc(r, fill_idx::POOL)?,
        acc(r, fill_idx::POOL_AUTHORITY)?,
        acc(r, fill_idx::POOL_AUTHORITY_MINT_ACCOUNT)?,
        acc(r, fill_idx::POOL_AUTHORITY_QUOTE_ACCOUNT)?,
        acc(r, fill_idx::AMM_GLOBAL_CONFIG)?,
        acc(r, fill_idx::LP_MINT)?,
        acc(r, fill_idx::USER_POOL_TOKEN_ACCOUNT)?,
        acc(r, fill_idx::POOL_BASE_TOKEN_ACCOUNT)?,
        acc(r, fill_idx::POOL_QUOTE_TOKEN_ACCOUNT)?,
        acc(r, fill_idx::BASE_TOKEN_PROGRAM)?,
        acc(r, fill_idx::QUOTE_TOKEN_PROGRAM)?,
        acc(r, fill_idx::BASE_TOKEN_PROGRAM)?,
        acc(r, fill_idx::ASSOCIATED_TOKEN_PROGRAM)?,
        acc(r, fill_idx::PUMP_AMM_EVENT_AUTHORITY)?,
        acc(r, fill_idx::RENT)?,
        acc(r, fill_idx::PUMP_EVENT_AUTHORITY)?,
        acc(r, fill_idx::PUMP_PROGRAM)?,
        acc(r, fill_idx::BOOST_VAULT_AUTHORITY)?,
        acc(r, fill_idx::BOOST_VAULT)?,
    ]);
    invoke_signed(&ix(PUMP_PROGRAM_ID, accounts, MIGRATE_V2_DISC.to_vec()), &account_infos, &[])
        .map_err(|e| e.into())
}

/// Pump's migrate is permissionless. The canonical pool is an AMM PDA, so only
/// the AMM program can create it. Once that account holds data, migrate_v2
/// reverts and this program must open claims without calling it again.
pub fn amm_pool_open(owner: &Pubkey, data_len: usize) -> bool {
    *owner == PUMP_AMM_PROGRAM_ID && data_len >= 8
}

pub fn curve_is_complete(bonding_curve: &AccountInfo) -> Result<bool> {
    let data = bonding_curve.try_borrow_data()?;
    if data.len() < 49 || data[..8] != BONDING_CURVE_DISC {
        return err!(PresaleError::CurveNotCompleted);
    }
    let real = u64::from_le_bytes(data[24..32].try_into().unwrap());
    let complete = data[48] == 1;
    Ok(complete && real == 0)
}

pub fn token_amount(account: &AccountInfo) -> Result<u64> {
    let data = account.try_borrow_data()?;
    if data.len() < 72 {
        return Ok(0);
    }
    Ok(u64::from_le_bytes(data[64..72].try_into().unwrap()))
}

pub fn mint_supply(account: &AccountInfo) -> Result<u64> {
    let data = account.try_borrow_data()?;
    if data.len() < 44 {
        return err!(PresaleError::BadAccount);
    }
    Ok(u64::from_le_bytes(data[36..44].try_into().unwrap()))
}

pub fn mint_decimals(account: &AccountInfo) -> Result<u8> {
    let data = account.try_borrow_data()?;
    if data.len() < 45 {
        return err!(PresaleError::BadAccount);
    }
    Ok(data[44])
}

fn has_tokens(account: &AccountInfo) -> Result<bool> {
    Ok(token_amount(account)? > 0)
}

fn lamports_above_rent(account: &AccountInfo) -> Result<bool> {
    let rent = Rent::get()?;
    let minimum = rent.minimum_balance(account.data_len());
    Ok(account.lamports() > minimum)
}

pub struct CrankInputs<'a, 'info, 'data> {
    pub remaining: &'a [AccountInfo<'info>],
    pub presale: AccountInfo<'info>,
    pub system_program: AccountInfo<'info>,
    pub presale_seeds: &'data [&'data [u8]],
}

pub fn curve_vault_has_fees(remaining: &[AccountInfo], sol_quote: bool) -> Result<bool> {
    require!(remaining.len() == crank_idx::LEN, PresaleError::BadAccount);
    if sol_quote {
        lamports_above_rent(acc(remaining, crank_idx::CREATOR_VAULT)?)
    } else {
        has_tokens(acc(remaining, crank_idx::CREATOR_VAULT_QUOTE_ATA)?)
    }
}

pub fn collect_creator_fees(input: &CrankInputs<'_, '_, '_>) -> Result<u64> {
    let r = input.remaining;
    require!(r.len() == crank_idx::LEN, PresaleError::BadAccount);
    let sol_quote = key(r, crank_idx::QUOTE_MINT)? == WSOL_MINT;
    let before = if sol_quote {
        input.presale.lamports()
    } else {
        token_amount(acc(r, crank_idx::PRESALE_QUOTE_ATA)?)?
    };

    let vault = acc(r, crank_idx::CREATOR_VAULT)?;
    let curve_has_fees = curve_vault_has_fees(r, sol_quote)?;
    if curve_has_fees {
        let accounts = vec![
            meta(input.presale.key(), false, true),
            meta(key(r, crank_idx::PRESALE_QUOTE_ATA)?, false, true),
            meta(vault.key(), false, true),
            meta(key(r, crank_idx::CREATOR_VAULT_QUOTE_ATA)?, false, true),
            meta(key(r, crank_idx::QUOTE_MINT)?, false, false),
            meta(key(r, crank_idx::QUOTE_TOKEN_PROGRAM)?, false, false),
            meta(key(r, crank_idx::ASSOCIATED_TOKEN_PROGRAM)?, false, false),
            meta(input.system_program.key(), false, false),
            meta(key(r, crank_idx::PUMP_EVENT_AUTHORITY)?, false, false),
            meta(key(r, crank_idx::PUMP_PROGRAM)?, false, false),
        ];
        let account_infos = infos(&[
            &input.presale,
            acc(r, crank_idx::PRESALE_QUOTE_ATA)?,
            vault,
            acc(r, crank_idx::CREATOR_VAULT_QUOTE_ATA)?,
            acc(r, crank_idx::QUOTE_MINT)?,
            acc(r, crank_idx::QUOTE_TOKEN_PROGRAM)?,
            acc(r, crank_idx::ASSOCIATED_TOKEN_PROGRAM)?,
            &input.system_program,
            acc(r, crank_idx::PUMP_EVENT_AUTHORITY)?,
            acc(r, crank_idx::PUMP_PROGRAM)?,
        ]);
        invoke_signed(
            &ix(PUMP_PROGRAM_ID, accounts, COLLECT_CREATOR_FEE_V2_DISC.to_vec()),
            &account_infos,
            &[],
        )?;
    }

    let amm_vault = acc(r, crank_idx::COIN_CREATOR_VAULT_ATA)?;
    if amm_vault.lamports() > 0 && has_tokens(amm_vault)? {
        let accounts = vec![
            meta(key(r, crank_idx::QUOTE_MINT)?, false, false),
            meta(key(r, crank_idx::QUOTE_TOKEN_PROGRAM)?, false, false),
            meta(input.presale.key(), false, false),
            meta(key(r, crank_idx::COIN_CREATOR_VAULT_AUTHORITY)?, false, false),
            meta(amm_vault.key(), false, true),
            meta(key(r, crank_idx::PRESALE_QUOTE_ATA)?, false, true),
            meta(key(r, crank_idx::PUMP_AMM_EVENT_AUTHORITY)?, false, false),
            meta(key(r, crank_idx::PUMP_AMM_PROGRAM)?, false, false),
        ];
        let account_infos = infos(&[
            acc(r, crank_idx::QUOTE_MINT)?,
            acc(r, crank_idx::QUOTE_TOKEN_PROGRAM)?,
            &input.presale,
            acc(r, crank_idx::COIN_CREATOR_VAULT_AUTHORITY)?,
            amm_vault,
            acc(r, crank_idx::PRESALE_QUOTE_ATA)?,
            acc(r, crank_idx::PUMP_AMM_EVENT_AUTHORITY)?,
            acc(r, crank_idx::PUMP_AMM_PROGRAM)?,
        ]);
        invoke_signed(
            &ix(PUMP_AMM_PROGRAM_ID, accounts, COLLECT_COIN_CREATOR_FEE_DISC.to_vec()),
            &account_infos,
            &[],
        )?;
    }

    let mut rent_kept = 0u64;
    if sol_quote {
        let quote_ata = acc(r, crank_idx::PRESALE_QUOTE_ATA)?;
        let wrapped = token_amount(quote_ata)?;
        if wrapped > 0 {
            let pre_close = input.presale.lamports();
            anchor_spl::token::close_account(CpiContext::new_with_signer(
                acc(r, crank_idx::QUOTE_TOKEN_PROGRAM)?.clone(),
                anchor_spl::token::CloseAccount {
                    account: quote_ata.clone(),
                    destination: input.presale.clone(),
                    authority: input.presale.clone(),
                },
                &[input.presale_seeds],
            ))?;
            let gained = input.presale.lamports().saturating_sub(pre_close);
            rent_kept = gained.saturating_sub(wrapped);
        }
    }

    let after = if sol_quote {
        input.presale.lamports()
    } else {
        token_amount(acc(r, crank_idx::PRESALE_QUOTE_ATA)?)?
    };
    Ok(after.saturating_sub(before).saturating_sub(rent_kept))
}
