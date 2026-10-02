use anchor_lang::prelude::*;

use crate::constants::{
    FEE_CONFIG_CURRENT_SIZE, FEE_CONFIG_DISC, FEE_CONFIG_MIN_SIZE, FEE_CONFIG_POST_STABLE_SIZE,
    FEE_TIER_LEN, GLOBAL_ACCOUNT_MIN, GLOBAL_DISC, MAX_FEE_TIERS, USDC_MINT, WSOL_MINT,
};
use crate::error::PresaleError;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QuoteKind {
    Sol,
    Token,
}

#[derive(Clone, Copy)]
pub struct FeesBps {
    pub protocol: u64,
    pub creator: u64,
}

#[derive(Clone)]
pub struct FeeTier {
    pub market_cap_lamports_threshold: u128,
    pub fees: FeesBps,
}

pub struct PumpGlobal {
    pub initial_virtual_token_reserves: u64,
    pub initial_virtual_sol_reserves: u64,
    pub initial_real_token_reserves: u64,
    pub token_total_supply: u64,
    pub fee_basis_points: u64,
    pub withdraw_authority: Pubkey,
    pub creator_fee_basis_points: u64,
    pub fee_recipient: Pubkey,
    pub create_v2_enabled: bool,
    pub buyback_fee_recipient: Pubkey,
    pub initial_virtual_quote_reserves: u64,
    pub whitelisted_quote_mint: Pubkey,
}

pub struct FeeConfigView {
    pub flat: FeesBps,
    pub fee_tiers: Vec<FeeTier>,
    pub stable_tiers: Vec<FeeTier>,
    pub exotic: FeesBps,
    pub exotic_set: bool,
}

pub struct CurveSnapshot {
    pub virtual_token_reserves: u64,
    pub virtual_quote_reserves: u64,
    pub real_token_reserves: u64,
    pub token_total_supply: u64,
    pub protocol_fee_bps: u64,
    pub creator_fee_bps: u64,
}

struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.pos)
    }

    fn take(&mut self, len: usize) -> Result<&'a [u8]> {
        if self.remaining() < len {
            return err!(PresaleError::GlobalAccount);
        }
        let start = self.pos;
        self.pos += len;
        Ok(&self.data[start..self.pos])
    }

    fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }

    fn bool(&mut self) -> Result<bool> {
        match self.u8()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => err!(PresaleError::GlobalAccount),
        }
    }

    fn u64(&mut self) -> Result<u64> {
        let bytes: [u8; 8] = self.take(8)?.try_into().unwrap();
        Ok(u64::from_le_bytes(bytes))
    }

    fn u128(&mut self) -> Result<u128> {
        let bytes: [u8; 16] = self.take(16)?.try_into().unwrap();
        Ok(u128::from_le_bytes(bytes))
    }

    fn pubkey(&mut self) -> Result<Pubkey> {
        let bytes: [u8; 32] = self.take(32)?.try_into().unwrap();
        Ok(Pubkey::new_from_array(bytes))
    }

    fn skip(&mut self, len: usize) -> Result<()> {
        self.take(len)?;
        Ok(())
    }
}

pub fn decode_global(data: &[u8]) -> Result<PumpGlobal> {
    if data.len() < GLOBAL_ACCOUNT_MIN || data.len() < 8 || data[..8] != GLOBAL_DISC {
        return err!(PresaleError::GlobalAccount);
    }
    let mut r = Reader::new(data);
    r.skip(8)?;
    r.bool()?; // initialized
    r.pubkey()?; // authority
    let fee_recipient = r.pubkey()?;
    let initial_virtual_token_reserves = r.u64()?;
    let initial_virtual_sol_reserves = r.u64()?;
    let initial_real_token_reserves = r.u64()?;
    let token_total_supply = r.u64()?;
    let fee_basis_points = r.u64()?;
    let withdraw_authority = r.pubkey()?;
    r.bool()?; // enable_migrate, unused by Pump
    r.u64()?; // pool_migration_fee
    let creator_fee_basis_points = r.u64()?;
    r.skip(32 * 7)?; // fee_recipients
    r.pubkey()?; // set_creator_authority
    r.pubkey()?; // admin_set_creator_authority
    let create_v2_enabled = r.bool()?;
    r.pubkey()?; // whitelist_pda
    r.pubkey()?; // reserved_fee_recipient
    r.bool()?; // mayhem_mode_enabled
    r.skip(32 * 7)?; // reserved_fee_recipients
    r.bool()?; // is_cashback_enabled
    let buyback_fee_recipient = r.pubkey()?;
    r.skip(32 * 7)?; // remaining buyback recipients
    r.u64()?; // buyback_basis_points
    let initial_virtual_quote_reserves = r.u64()?;
    let whitelisted_quote_mint = r.pubkey()?;

    if fee_recipient == Pubkey::default() || buyback_fee_recipient == Pubkey::default() {
        return err!(PresaleError::GlobalAccount);
    }

    Ok(PumpGlobal {
        initial_virtual_token_reserves,
        initial_virtual_sol_reserves,
        initial_real_token_reserves,
        token_total_supply,
        fee_basis_points,
        withdraw_authority,
        creator_fee_basis_points,
        fee_recipient,
        create_v2_enabled,
        buyback_fee_recipient,
        initial_virtual_quote_reserves,
        whitelisted_quote_mint,
    })
}

fn read_fees(r: &mut Reader) -> Result<FeesBps> {
    let _lp = r.u64()?;
    Ok(FeesBps {
        protocol: r.u64()?,
        creator: r.u64()?,
    })
}

fn read_tiers(r: &mut Reader) -> Result<Vec<FeeTier>> {
    let count = u32::from_le_bytes(r.take(4)?.try_into().unwrap()) as usize;
    if count > MAX_FEE_TIERS {
        return err!(PresaleError::FeeConfig);
    }
    let mut tiers = Vec::with_capacity(count);
    for _ in 0..count {
        if r.remaining() < FEE_TIER_LEN {
            return err!(PresaleError::FeeConfig);
        }
        let market_cap_lamports_threshold = r.u128()?;
        let fees = read_fees(r)?;
        tiers.push(FeeTier {
            market_cap_lamports_threshold,
            fees,
        });
    }
    Ok(tiers)
}

pub fn decode_fee_config(data: &[u8]) -> Result<FeeConfigView> {
    if data.len() < FEE_CONFIG_MIN_SIZE || data.len() < 8 || data[..8] != FEE_CONFIG_DISC {
        return err!(PresaleError::FeeConfig);
    }
    let mut r = Reader::new(data);
    r.skip(8)?;
    r.u8()?; // bump
    r.pubkey()?; // admin
    let flat = read_fees(&mut r)?;
    let fee_tiers = read_tiers(&mut r)?;
    if fee_tiers.is_empty() {
        return err!(PresaleError::FeeConfig);
    }
    let stable_tiers = if data.len() >= FEE_CONFIG_POST_STABLE_SIZE {
        read_tiers(&mut r)?
    } else {
        Vec::new()
    };
    let (exotic, exotic_set) = if data.len() >= FEE_CONFIG_CURRENT_SIZE {
        let exotic = read_fees(&mut r)?;
        let exotic_set = exotic.protocol != 0 || exotic.creator != 0;
        (exotic, exotic_set)
    } else {
        (FeesBps { protocol: 0, creator: 0 }, false)
    };
    Ok(FeeConfigView {
        flat,
        fee_tiers,
        stable_tiers,
        exotic,
        exotic_set,
    })
}

pub fn quote_kind(global: &PumpGlobal, quote_mint: &Pubkey) -> Result<QuoteKind> {
    if *quote_mint == WSOL_MINT {
        return Ok(QuoteKind::Sol);
    }
    if *quote_mint != Pubkey::default() && *quote_mint == global.whitelisted_quote_mint {
        return Ok(QuoteKind::Token);
    }
    err!(PresaleError::QuoteNotAllowlisted)
}

fn select_tier(tiers: &[FeeTier], market_cap: u128) -> Result<FeesBps> {
    if tiers.is_empty() {
        return err!(PresaleError::FeeConfig);
    }
    if market_cap < tiers[0].market_cap_lamports_threshold {
        return Ok(tiers[0].fees);
    }
    for tier in tiers.iter().rev() {
        if market_cap >= tier.market_cap_lamports_threshold {
            return Ok(tier.fees);
        }
    }
    Ok(tiers[0].fees)
}

pub fn curve_fees(
    kind: QuoteKind,
    quote_mint: &Pubkey,
    global: &PumpGlobal,
    fee_config: &FeeConfigView,
    virtual_quote: u64,
    virtual_token: u64,
) -> Result<FeesBps> {
    if virtual_token == 0 || global.token_total_supply == 0 {
        return err!(PresaleError::GlobalAccount);
    }
    let market_cap = (virtual_quote as u128)
        .checked_mul(global.token_total_supply as u128)
        .ok_or(PresaleError::MathOverflow)?
        / (virtual_token as u128);

    if fee_config.fee_tiers.is_empty() {
        return Ok(FeesBps {
            protocol: global.fee_basis_points,
            creator: global.creator_fee_basis_points,
        });
    }

    let selected = match kind {
        QuoteKind::Sol => select_tier(&fee_config.fee_tiers, market_cap)?,
        QuoteKind::Token if *quote_mint == USDC_MINT => {
            if fee_config.stable_tiers.is_empty() {
                select_tier(&fee_config.fee_tiers, market_cap)?
            } else {
                select_tier(&fee_config.stable_tiers, market_cap)?
            }
        }
        QuoteKind::Token if fee_config.exotic_set => fee_config.exotic,
        QuoteKind::Token => fee_config.flat,
    };
    Ok(selected)
}

fn ceil_div(a: u128, b: u128) -> Result<u128> {
    if b == 0 {
        return err!(PresaleError::MathOverflow);
    }
    Ok(a.checked_add(b - 1)
        .ok_or(PresaleError::MathOverflow)?
        .checked_div(b)
        .ok_or(PresaleError::MathOverflow)?)
}

/// Quote that enters the curve after Pump deducts protocol and creator fees
/// from `spendable`, using the `buy_exact_sol_in` formula.
pub fn net_quote_in(spendable: u64, protocol_bps: u64, creator_bps: u64) -> Result<u64> {
    let total = (protocol_bps as u128)
        .checked_add(creator_bps as u128)
        .ok_or(PresaleError::MathOverflow)?;
    let spendable = spendable as u128;
    let mut net = spendable
        .checked_mul(10_000)
        .ok_or(PresaleError::MathOverflow)?
        / (10_000 + total);
    let fees = ceil_div(net * protocol_bps as u128, 10_000)?
        + ceil_div(net * creator_bps as u128, 10_000)?;
    if net + fees > spendable {
        net -= net + fees - spendable;
    }
    u64::try_from(net).map_err(|_| error!(PresaleError::MathOverflow))
}

/// Creator fee Pump charges on a curve buy of `spendable` quote.
pub fn creator_fee_on_buy(spendable: u64, protocol_bps: u64, creator_bps: u64) -> Result<u64> {
    if spendable == 0 || creator_bps == 0 {
        return Ok(0);
    }
    let net = net_quote_in(spendable, protocol_bps, creator_bps)?;
    let fee = ceil_div(
        (net as u128)
            .checked_mul(creator_bps as u128)
            .ok_or(PresaleError::MathOverflow)?,
        10_000,
    )?;
    u64::try_from(fee).map_err(|_| error!(PresaleError::MathOverflow))
}

/// Tokens the exact-quote formula would buy, before the real-reserve cap.
pub fn tokens_out_uncapped(
    spendable: u64,
    virtual_token: u64,
    virtual_quote: u64,
    protocol_bps: u64,
    creator_bps: u64,
) -> Result<u64> {
    let net = net_quote_in(spendable, protocol_bps, creator_bps)?;
    if net == 0 {
        return Ok(0);
    }
    let net = net as u128;
    let numerator = (net - 1)
        .checked_mul(virtual_token as u128)
        .ok_or(PresaleError::MathOverflow)?;
    let denominator = (virtual_quote as u128)
        .checked_add(net - 1)
        .ok_or(PresaleError::MathOverflow)?;
    if denominator == 0 {
        return err!(PresaleError::MathOverflow);
    }
    u64::try_from(numerator / denominator).map_err(|_| error!(PresaleError::MathOverflow))
}

/// Smallest spendable quote that buys at least `real_tokens`, fees included.
pub fn minimum_spendable(snapshot: &CurveSnapshot) -> Result<u64> {
    if snapshot.virtual_token_reserves <= snapshot.real_token_reserves
        || snapshot.real_token_reserves == 0
        || snapshot.virtual_quote_reserves == 0
    {
        return err!(PresaleError::GlobalAccount);
    }
    let guess_net = ceil_div(
        (snapshot.real_token_reserves as u128) * (snapshot.virtual_quote_reserves as u128),
        snapshot.virtual_token_reserves as u128 - snapshot.real_token_reserves as u128,
    )? + 1;
    let total_bps = snapshot.protocol_fee_bps as u128 + snapshot.creator_fee_bps as u128;
    let mut hi = ceil_div(guess_net * (10_000 + total_bps), 10_000)?;
    hi = hi.max(1);
    let mut hi = u64::try_from(hi).map_err(|_| error!(PresaleError::MathOverflow))?;

    let mut guard = 0u32;
    while tokens_out_uncapped(
        hi,
        snapshot.virtual_token_reserves,
        snapshot.virtual_quote_reserves,
        snapshot.protocol_fee_bps,
        snapshot.creator_fee_bps,
    )? < snapshot.real_token_reserves
    {
        hi = hi.checked_mul(2).ok_or(PresaleError::QuoteTargetUnreachable)?;
        guard += 1;
        if guard > 64 {
            return err!(PresaleError::QuoteTargetUnreachable);
        }
    }

    let mut lo = 0u64;
    while lo + 1 < hi {
        let mid = lo + (hi - lo) / 2;
        if tokens_out_uncapped(
            mid,
            snapshot.virtual_token_reserves,
            snapshot.virtual_quote_reserves,
            snapshot.protocol_fee_bps,
            snapshot.creator_fee_bps,
        )? >= snapshot.real_token_reserves
        {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    Ok(hi)
}

/// PumpSwap is seeded with the unsold supply and the net quote. That open is
/// above the flat price when this inequality holds.
pub fn open_beats_flat(snapshot: &CurveSnapshot, quote_target: u64) -> Result<bool> {
    let net = net_quote_in(
        quote_target,
        snapshot.protocol_fee_bps,
        snapshot.creator_fee_bps,
    )?;
    let pool_tokens = snapshot
        .token_total_supply
        .checked_sub(snapshot.real_token_reserves)
        .ok_or(PresaleError::GlobalAccount)?;
    if pool_tokens == 0 || snapshot.real_token_reserves == 0 {
        return err!(PresaleError::GlobalAccount);
    }
    let left = (net as u128) * (snapshot.real_token_reserves as u128);
    let right = (quote_target as u128) * (pool_tokens as u128);
    Ok(left > right)
}

pub fn snapshot_for(
    global: &PumpGlobal,
    fee_config: &FeeConfigView,
    quote_mint: &Pubkey,
) -> Result<CurveSnapshot> {
    let kind = quote_kind(global, quote_mint)?;
    let virtual_quote = match kind {
        QuoteKind::Sol => global.initial_virtual_sol_reserves,
        QuoteKind::Token => global.initial_virtual_quote_reserves,
    };
    let fees = curve_fees(
        kind,
        quote_mint,
        global,
        fee_config,
        virtual_quote,
        global.initial_virtual_token_reserves,
    )?;
    Ok(CurveSnapshot {
        virtual_token_reserves: global.initial_virtual_token_reserves,
        virtual_quote_reserves: virtual_quote,
        real_token_reserves: global.initial_real_token_reserves,
        token_total_supply: global.token_total_supply,
        protocol_fee_bps: fees.protocol,
        creator_fee_bps: fees.creator,
    })
}

pub fn quote_target(
    global: &PumpGlobal,
    fee_config: &FeeConfigView,
    quote_mint: &Pubkey,
) -> Result<u64> {
    if !global.create_v2_enabled {
        return err!(PresaleError::CreateDisabled);
    }
    let snapshot = snapshot_for(global, fee_config, quote_mint)?;
    let target = minimum_spendable(&snapshot)?;
    if !open_beats_flat(&snapshot, target)? {
        return err!(PresaleError::OpenBelowFlat);
    }
    Ok(target)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pump_curve(protocol: u64, creator: u64) -> CurveSnapshot {
        CurveSnapshot {
            virtual_token_reserves: 1_073_000_000_000_000,
            virtual_quote_reserves: 30_000_000_000,
            real_token_reserves: 793_100_000_000_000,
            token_total_supply: 1_000_000_000_000_000,
            protocol_fee_bps: protocol,
            creator_fee_bps: creator,
        }
    }

    #[test]
    fn minimum_spendable_completes_the_curve() {
        let snapshot = pump_curve(100, 30);
        let target = minimum_spendable(&snapshot).unwrap();
        let bought = tokens_out_uncapped(
            target,
            snapshot.virtual_token_reserves,
            snapshot.virtual_quote_reserves,
            snapshot.protocol_fee_bps,
            snapshot.creator_fee_bps,
        )
        .unwrap();
        let one_less = tokens_out_uncapped(
            target - 1,
            snapshot.virtual_token_reserves,
            snapshot.virtual_quote_reserves,
            snapshot.protocol_fee_bps,
            snapshot.creator_fee_bps,
        )
        .unwrap();
        assert!(bought >= snapshot.real_token_reserves);
        assert!(one_less < snapshot.real_token_reserves);
        assert!(open_beats_flat(&snapshot, target).unwrap());
        let net = net_quote_in(target, 100, 30).unwrap();
        assert!(target > net);
        assert_eq!(creator_fee_on_buy(86_067_926_048, 95, 30).unwrap(), 255_016_078);
        // About 85 SOL into the curve, plus the fee.
        assert!(net > 80_000_000_000 && net < 90_000_000_000);
    }

    #[test]
    fn zero_fee_still_beats_the_open() {
        let snapshot = pump_curve(0, 0);
        let target = minimum_spendable(&snapshot).unwrap();
        assert!(open_beats_flat(&snapshot, target).unwrap());
    }

    #[test]
    fn global_roundtrip_reads_whitelist_and_curve() {
        let mut data = Vec::new();
        data.extend_from_slice(&GLOBAL_DISC);
        data.push(1); // initialized
        data.extend_from_slice(&[7u8; 32]); // authority
        data.extend_from_slice(&[3u8; 32]); // fee recipient
        data.extend_from_slice(&1_073_000_000_000_000u64.to_le_bytes());
        data.extend_from_slice(&30_000_000_000u64.to_le_bytes());
        data.extend_from_slice(&793_100_000_000_000u64.to_le_bytes());
        data.extend_from_slice(&1_000_000_000_000_000u64.to_le_bytes());
        data.extend_from_slice(&100u64.to_le_bytes()); // fee bps
        data.extend_from_slice(&[9u8; 32]); // withdraw
        data.push(1); // enable migrate
        data.extend_from_slice(&15_000_001u64.to_le_bytes());
        data.extend_from_slice(&30u64.to_le_bytes()); // creator fee
        data.extend_from_slice(&[1u8; 32 * 7]); // fee recipients
        data.extend_from_slice(&[2u8; 32]); // set creator
        data.extend_from_slice(&[2u8; 32]); // admin set creator
        data.push(1); // create v2
        data.extend_from_slice(&[4u8; 32]); // whitelist pda
        data.extend_from_slice(&[5u8; 32]); // reserved fee recipient
        data.push(0); // mayhem
        data.extend_from_slice(&[6u8; 32 * 7]);
        data.push(0); // cashback
        data.extend_from_slice(&[8u8; 32]); // buyback 0
        data.extend_from_slice(&[8u8; 32 * 7]);
        data.extend_from_slice(&0u64.to_le_bytes());
        data.extend_from_slice(&5_000_000_000u64.to_le_bytes()); // virtual quote
        let usdc = USDC_MINT.to_bytes();
        data.extend_from_slice(&usdc);
        while data.len() < GLOBAL_ACCOUNT_MIN {
            data.push(0);
        }
        let global = decode_global(&data).unwrap();
        assert_eq!(global.initial_virtual_sol_reserves, 30_000_000_000);
        assert_eq!(global.initial_virtual_quote_reserves, 5_000_000_000);
        assert_eq!(global.whitelisted_quote_mint, USDC_MINT);
        assert!(global.create_v2_enabled);
        assert_eq!(global.fee_recipient, Pubkey::new_from_array([3u8; 32]));
        assert_eq!(quote_kind(&global, &WSOL_MINT).unwrap(), QuoteKind::Sol);
        assert_eq!(quote_kind(&global, &USDC_MINT).unwrap(), QuoteKind::Token);
        assert!(quote_kind(&global, &Pubkey::new_from_array([1u8; 32])).is_err());
    }
}
