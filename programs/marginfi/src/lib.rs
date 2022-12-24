pub mod constants;
pub mod errors;
pub mod instructions;
pub mod macros;
pub mod prelude;
pub mod state;
pub mod utils;

use anchor_lang::prelude::*;
use instructions::*;
use prelude::*;
use state::marginfi_group::{BankConfig, BankConfigOpt};
use static_assertions::assert_cfg;

#[cfg(feature = "mainnet-beta")] // mainnet
declare_id!("Fg6PaFpoGXkYsidMpWTK6W2BeZ7FEfcYkg476zPFsLnS");
#[cfg(feature = "devnet")] // devnet
declare_id!("HfHBtENWH9C27kXMwP62WCSMm734kzKj9YnzUaHPzk6i");
#[cfg(all(not(feature = "mainnet-beta"), not(feature = "devnet")))] // other
declare_id!("Fg6PaFpoGXkYsidMpWTK6W2BeZ7FEfcYkg476zPFsLnS");

assert_cfg!(
    not(all(feature = "mainnet-beta", feature = "devnet")),
    "Devnet feature must be disabled for a mainnet release"
);
assert_cfg!(
    not(all(feature = "mainnet-beta", feature = "test")),
    "Test feature must be disabled for a mainnet release"
);
/// Marginfi v2 program entrypoint.
///
/// Instructions:
/// Admin instructions:
/// - `initialize_marginfi_group` - Initializes a new marginfi group.
/// - `configure_marginfi_group` - Configures a marginfi group.
/// - `lending_pool_add_bank` - Adds a bank to a lending pool.
/// - `lending_pool_configure_bank` - Configures a bank in a lending pool.
///
/// User instructions:
/// - `create_margin_account` - Creates a new margin account.
/// - `lending_pool_deposit` - Deposits liquidity into a bank.
/// - `lending_pool_withdraw` - Withdraws liquidity from a bank.
/// - `liquidate` - Liquidates a margin account.
///
/// Operational instructions:
/// - `accrue_interest` - Accrues interest for a reserve.
#[program]
pub mod marginfi {

    use super::*;

    pub fn initialize_marginfi_group(ctx: Context<InitializeMarginfiGroup>) -> MarginfiResult {
        marginfi_group::initialize(ctx)
    }

    pub fn configure_marginfi_group(
        ctx: Context<ConfigureMarginfiGroup>,
        config: GroupConfig,
    ) -> MarginfiResult {
        marginfi_group::configure(ctx, config)
    }

    pub fn lending_pool_add_bank(
        ctx: Context<LendingPoolAddBank>,
        bank_index: u16,
        bank_config: BankConfig,
    ) -> MarginfiResult {
        marginfi_group::lending_pool_add_bank(ctx, bank_index, bank_config)
    }

    pub fn lending_pool_configure_bank(
        ctx: Context<LendingPoolConfigureBank>,
        bank_index: u16,
        bank_config_opt: BankConfigOpt,
    ) -> MarginfiResult {
        marginfi_group::lending_pool_configure_bank(ctx, bank_index, bank_config_opt)
    }

    // User instructions
    pub fn initialize_marginfi_account(ctx: Context<InitializeMarginfiAccount>) -> MarginfiResult {
        marginfi_account::initialize(ctx)
    }

    pub fn bank_deposit(ctx: Context<BankDeposit>, amount: u64) -> MarginfiResult {
        marginfi_account::bank_deposit(ctx, amount)
    }

    pub fn bank_withdraw(ctx: Context<BankWithdraw>, amount: u64) -> MarginfiResult {
        marginfi_account::bank_withdraw(ctx, amount)
    }

    pub fn liquidate(
        ctx: Context<LendingAccountLiquidate>,
        asset_bank_index: u16,
        asset_amount: u64,
        liab_bank_index: u16,
    ) -> MarginfiResult {
        marginfi_account::lending_account_liquidate(
            ctx,
            asset_bank_index,
            asset_amount,
            liab_bank_index,
        )
    }

    // Operational instructions
    pub fn bank_accrue_interest(
        ctx: Context<LendingPoolBankAccrueInterest>,
        bank_index: u16,
    ) -> MarginfiResult {
        marginfi_group::lending_pool_bank_accrue_interest(ctx, bank_index)
    }
}
