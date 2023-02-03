use crate::prelude::{MarginfiGroup, MarginfiResult};
use crate::state::marginfi_group::Bank;
use crate::{
    constants::LIQUIDITY_VAULT_SEED,
    state::marginfi_account::{BankAccountWrapper, MarginfiAccount},
};
use anchor_lang::prelude::*;
use anchor_spl::token::{Token, Transfer};
use fixed::types::I80F48;
use solana_program::clock::Clock;
use solana_program::sysvar::Sysvar;

/// 1. Accrue interest
/// 2. Create the user's bank account for the asset deposited if it does not exist yet
/// 3. Record asset increase in the bank account
/// 4. Transfer funds from the signer's token account to the bank's liquidity vault
///
/// Will error if there is an existing liability <=> repaying is not allowed.
pub fn lending_pool_deposit(ctx: Context<LendingPoolDeposit>, amount: u64) -> MarginfiResult {
    let LendingPoolDeposit {
        marginfi_account,
        signer,
        signer_token_account,
        bank_liquidity_vault,
        token_program,
        bank: bank_loader,
        ..
    } = ctx.accounts;

    bank_loader.load_mut()?.accrue_interest(&Clock::get()?)?;

    let mut bank = bank_loader.load_mut()?;
    let mut marginfi_account = marginfi_account.load_mut()?;

    let mut bank_account = BankAccountWrapper::find_or_create(
        &bank_loader.key(),
        &mut bank,
        &mut marginfi_account.lending_account,
    )?;

    bank_account.deposit(I80F48::from_num(amount))?;
    bank_account.deposit_spl_transfer(
        amount,
        Transfer {
            from: signer_token_account.to_account_info(),
            to: bank_liquidity_vault.to_account_info(),
            authority: signer.to_account_info(),
        },
        token_program.to_account_info(),
    )?;

    Ok(())
}

#[derive(Accounts)]
pub struct LendingPoolDeposit<'info> {
    pub marginfi_group: AccountLoader<'info, MarginfiGroup>,

    #[account(
        mut,
        constraint = marginfi_account.load()?.group == marginfi_group.key(),
    )]
    pub marginfi_account: AccountLoader<'info, MarginfiAccount>,

    #[account(
        address = marginfi_account.load()?.authority,
    )]
    pub signer: Signer<'info>,

    #[account(
        mut,
        constraint = bank.load()?.group == marginfi_group.key(),
    )]
    pub bank: AccountLoader<'info, Bank>,

    /// CHECK: Token mint/authority are checked at transfer
    #[account(mut)]
    pub signer_token_account: AccountInfo<'info>,

    /// CHECK: Seed constraint check
    #[account(
        mut,
        seeds = [
            LIQUIDITY_VAULT_SEED.as_bytes(),
            bank.key().as_ref(),
        ],
        bump = bank.load()?.liquidity_vault_bump,
    )]
    pub bank_liquidity_vault: AccountInfo<'info>,

    pub token_program: Program<'info, Token>,
}
