#![cfg(feature = "test-bpf")]
#![allow(dead_code)]

use crate::{
    fixtures::{marginfi_group::*, spl::*, utils::*},
    native,
};
use anchor_lang::prelude::*;
use bincode::deserialize;

use fixed_macro::types::I80F48;
use lazy_static::lazy_static;
use marginfi::state::marginfi_group::{BankConfig, GroupConfig};
use solana_program::{hash::Hash, sysvar};
use solana_program_test::*;
use solana_sdk::{pubkey, signature::Keypair, signer::Signer};
use std::{cell::RefCell, rc::Rc};

use super::marginfi_account::MarginfiAccountFixture;

#[derive(Debug, Clone)]
pub enum BankMint {
    SOL,
    USDC,
}

#[derive(Debug, Clone)]
pub struct BankSetting {
    pub index: u8,
    pub mint: BankMint,
}

#[derive(Debug, Clone)]
pub struct TestSettings {
    pub group_config: GroupConfig,
    pub banks: Vec<BankSetting>,
}

pub struct TestFixture {
    pub context: Rc<RefCell<ProgramTestContext>>,
    pub marginfi_group: MarginfiGroupFixture,
    pub usdc_mint: MintFixture,
    pub sol_mint: MintFixture,
}

pub const PYTH_USDC_FEED: Pubkey = pubkey!("PythUsdcPrice111111111111111111111111111111");
pub const PYTH_SOL_FEED: Pubkey = pubkey!("PythSo1Price1111111111111111111111111111111");
pub const FAKE_PYTH_USDC_FEED: Pubkey = pubkey!("FakePythUsdcPrice11111111111111111111111111");

lazy_static! {
    pub static ref DEFAULT_USDC_TEST_BANK_CONFIG: BankConfig = BankConfig {
        pyth_oracle: PYTH_USDC_FEED,
        max_capacity: native!(1_000_000, "USDC"),
        deposit_weight_init: I80F48!(1).into(),
        ..BankConfig::default()
    };
    pub static ref DEFAULT_SOL_TEST_BANK_CONFIG: BankConfig = BankConfig {
        pyth_oracle: PYTH_SOL_FEED,
        max_capacity: native!(1_000, "SOL"),
        ..BankConfig::default()
    };
}

pub const USDC_MINT_DECIMALS: u8 = 6;
pub const SOL_MINT_DECIMALS: u8 = 9;

impl TestFixture {
    pub async fn new(test_settings: Option<TestSettings>) -> TestFixture {
        let mut program = ProgramTest::new("marginfi", marginfi::ID, processor!(marginfi::entry));

        let usdc_keypair = Keypair::new();
        let sol_keypair = Keypair::new();

        program.add_account(
            PYTH_USDC_FEED,
            craft_pyth_price_account(usdc_keypair.pubkey(), 1, USDC_MINT_DECIMALS.into()),
        );
        program.add_account(
            PYTH_SOL_FEED,
            craft_pyth_price_account(sol_keypair.pubkey(), 10, SOL_MINT_DECIMALS.into()),
        );

        let context = Rc::new(RefCell::new(program.start_with_context().await));
        solana_logger::setup_with_default(RUST_LOG_DEFAULT);

        let usdc_mint_f = MintFixture::new(Rc::clone(&context), Some(usdc_keypair), None).await;
        let sol_mint_f = MintFixture::new(
            Rc::clone(&context),
            Some(sol_keypair),
            Some(SOL_MINT_DECIMALS),
        )
        .await;

        let tester_group = MarginfiGroupFixture::new(
            Rc::clone(&context),
            &usdc_mint_f.key,
            test_settings
                .clone()
                .map(|ts| ts.group_config)
                .unwrap_or(GroupConfig {
                    admin: None,
                    paused: None,
                }),
        )
        .await;

        if let Some(test_settings) = test_settings.clone() {
            for bank in test_settings.banks.iter() {
                let bank_mint = match bank.mint {
                    BankMint::USDC => &usdc_mint_f,
                    BankMint::SOL => &sol_mint_f,
                };
                tester_group
                    .try_lending_pool_add_bank(
                        bank_mint.key,
                        bank.index.into(),
                        *DEFAULT_USDC_TEST_BANK_CONFIG,
                    )
                    .await
                    .unwrap()
            }
        };

        TestFixture {
            context: Rc::clone(&context),
            marginfi_group: tester_group,
            usdc_mint: usdc_mint_f,
            sol_mint: sol_mint_f,
        }
    }

    pub async fn create_marginfi_account(&self) -> MarginfiAccountFixture {
        let marfingi_account_f =
            MarginfiAccountFixture::new(Rc::clone(&self.context), &self.marginfi_group.key).await;

        marfingi_account_f
    }

    pub async fn load_and_deserialize<T: anchor_lang::AccountDeserialize>(
        &self,
        address: &Pubkey,
    ) -> T {
        let ai = self
            .context
            .borrow_mut()
            .banks_client
            .get_account(*address)
            .await
            .unwrap()
            .unwrap();

        T::try_deserialize(&mut ai.data.as_slice()).unwrap()
    }

    pub fn payer(&self) -> Pubkey {
        self.context.borrow().payer.pubkey()
    }

    pub fn payer_keypair(&self) -> Keypair {
        clone_keypair(&self.context.borrow().payer)
    }

    pub fn set_time(&self, timestamp: i64) {
        let clock = sysvar::clock::Clock {
            unix_timestamp: timestamp,
            ..Default::default()
        };
        self.context.borrow_mut().set_sysvar(&clock);
    }

    pub async fn get_minimum_rent_for_size(&self, size: usize) -> u64 {
        self.context
            .borrow_mut()
            .banks_client
            .get_rent()
            .await
            .unwrap()
            .minimum_balance(size)
    }

    pub async fn get_latest_blockhash(&self) -> Hash {
        self.context
            .borrow_mut()
            .banks_client
            .get_latest_blockhash()
            .await
            .unwrap()
    }

    pub async fn get_slot(&self) -> u64 {
        self.context
            .borrow_mut()
            .banks_client
            .get_root_slot()
            .await
            .unwrap()
    }

    pub async fn get_clock(&self) -> Clock {
        deserialize::<Clock>(
            &self
                .context
                .borrow_mut()
                .banks_client
                .get_account(sysvar::clock::ID)
                .await
                .unwrap()
                .unwrap()
                .data,
        )
        .unwrap()
    }
}
