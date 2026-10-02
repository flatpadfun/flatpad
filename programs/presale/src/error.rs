use anchor_lang::prelude::*;

#[error_code]
pub enum PresaleError {
    #[msg("Quote mint is not an allowlisted Pump quote")]
    QuoteNotAllowlisted,
    #[msg("Quote token program does not own the mint")]
    QuoteTokenProgram,
    #[msg("Name, symbol, or uri is empty or longer than Pump allows")]
    BadMetadata,
    #[msg("Fee mode must be dev or holders")]
    BadFeeMode,
    #[msg("Dev wallet is missing")]
    BadDevWallet,
    #[msg("The raise is no longer accepting deposits")]
    RaiseClosed,
    #[msg("Deposit amount is zero")]
    NothingToDeposit,
    #[msg("This wallet has no open deposit")]
    EmptyPosition,
    #[msg("Withdraw amount is above the open deposit")]
    AbovePosition,
    #[msg("The raise has not reached its target")]
    RaiseNotFull,
    #[msg("Pump curve or fee parameters changed since this presale was created")]
    PumpParametersChanged,
    #[msg("The buy did not complete the bonding curve")]
    CurveNotCompleted,
    #[msg("This presale has already migrated")]
    AlreadyMigrated,
    #[msg("Token claims open only after migration")]
    NotMigrated,
    #[msg("Payout rounded to zero")]
    ZeroPayout,
    #[msg("An account required by the fill or crank is missing or wrong")]
    BadAccount,
    #[msg("Pump fee config could not be read")]
    FeeConfig,
    #[msg("Pump global could not be read")]
    GlobalAccount,
    #[msg("No quote amount completes the current Pump curve")]
    QuoteTargetUnreachable,
    #[msg("The PumpSwap open would not be above the flat presale price")]
    OpenBelowFlat,
    #[msg("Arithmetic overflow")]
    MathOverflow,
    #[msg("Pump create_v2 is disabled")]
    CreateDisabled,
    #[msg("The vault does not hold the payout")]
    InsufficientVault,
}
