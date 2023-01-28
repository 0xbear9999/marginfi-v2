use anchor_lang::prelude::*;

#[error_code]
pub enum MarginfiError {
    #[msg("Math error")]
    MathError,
    #[msg("Invalid bank index")]
    BankNotFound,
    #[msg("Lending account balance not found")]
    LendingAccountBalanceNotFound,
    #[msg("Bank deposit capacity exceeded")]
    BankDepositCapacityExceeded,
    #[msg("Invalid transfer")]
    InvalidTransfer,
    #[msg("Missing Pyth or Bank account")]
    MissingPythOrBankAccount,
    #[msg("Missing Pyth account")]
    MissingPythAccount,
    #[msg("Invalid Pyth account")]
    InvalidOracleAccount,
    #[msg("Missing Bank account")]
    MissingBankAccount,
    #[msg("Invalid Bank account")]
    InvalidBankAccount,
    #[msg("Bad account health")]
    BadAccountHealth,
    #[msg("Lending account balance slots are full")]
    LendingAccountBalanceSlotsFull,
    #[msg("Bank already exists")]
    BankAlreadyExists,
    #[msg("Borrowing not allowed")]
    BorrowingNotAllowed,
    #[msg("Illegal post liquidation state, account is either not unhealthy or liquidation was too big")]
    AccountIllegalPostLiquidationState,
    #[msg("Account is not bankrupt")]
    AccountNotBankrupt,
    #[msg("Account balance is not bad debt")]
    BalanceNotBadDebt,
    #[msg("Invalid group config")]
    InvalidConfig,
    #[msg("Stale oracle data")]
    StaleOracle,
    #[msg("Bank paused")]
    BankPaused,
    #[msg("Bank is ReduceOnly mode")]
    BankReduceOnly,
    #[msg("Invalid oracle setup")]
    InvalidOracleSetup,
    #[msg("Invalid bank utilization ratio")]
    IllegalUtilizationRatio,
    #[msg("Ixs sysvar verification failed")]
    FlashloanIxsSysvarInvalid,
}

impl From<MarginfiError> for ProgramError {
    fn from(e: MarginfiError) -> Self {
        ProgramError::Custom(e as u32)
    }
}
