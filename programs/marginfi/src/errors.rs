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
    #[msg("Invalid Pyth account")]
    InvalidPythAccount,
    #[msg("Missing Pyth account")]
    MissingPythAccount,
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
    #[msg("Group operations are paused")]
    GroupPaused,
}

impl From<MarginfiError> for ProgramError {
    fn from(e: MarginfiError) -> Self {
        ProgramError::Custom(e as u32)
    }
}
