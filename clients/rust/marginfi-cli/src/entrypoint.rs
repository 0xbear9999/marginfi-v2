use crate::{
    config::GlobalOptions,
    processor::{self, process_list_deposits, process_list_lip_campaigns},
    profile::{load_profile, Profile},
};
use anchor_client::Cluster;
use anyhow::Result;
use clap::{clap_derive::ArgEnum, Parser};

#[cfg(feature = "admin")]
use fixed::types::I80F48;
#[cfg(any(feature = "admin", feature = "dev"))]
use marginfi::state::marginfi_group::BankConfigOpt;
use marginfi::state::marginfi_group::BankOperationalState;
#[cfg(feature = "dev")]
use marginfi::{
    prelude::{GroupConfig, MarginfiGroup},
    state::{
        marginfi_account::{Balance, LendingAccount, MarginfiAccount},
        marginfi_group::{Bank, BankConfig, InterestRateConfig, OracleConfig, WrappedI80F48},
    },
};
use solana_sdk::{commitment_config::CommitmentLevel, pubkey::Pubkey};
#[cfg(feature = "dev")]
use type_layout::TypeLayout;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Parser)]
#[clap(version = VERSION)]
pub struct Opts {
    #[clap(flatten)]
    pub cfg_override: GlobalOptions,
    #[clap(subcommand)]
    pub command: Command,
}

#[derive(Debug, Parser)]
pub enum Command {
    Group {
        #[clap(subcommand)]
        subcmd: GroupCommand,
    },
    Bank {
        #[clap(subcommand)]
        subcmd: BankCommand,
    },
    Profile {
        #[clap(subcommand)]
        subcmd: ProfileCommand,
    },
    #[cfg(feature = "dev")]
    InspectPadding {},
    Account {
        #[clap(subcommand)]
        subcmd: AccountCommand,
    },
    Lip {
        #[clap(subcommand)]
        subcmd: LipCommand,
    },
}

#[derive(Debug, Parser)]
pub enum GroupCommand {
    Get {
        marginfi_group: Option<Pubkey>,
    },
    GetAll {},
    #[cfg(feature = "admin")]
    Create {
        admin: Option<Pubkey>,
        #[clap(short = 'f', long = "override")]
        override_existing_profile_group: bool,
    },
    #[cfg(feature = "admin")]
    Update {
        admin: Option<Pubkey>,
    },
    #[cfg(feature = "admin")]
    AddBank {
        #[clap(long)]
        mint: Pubkey,
        #[clap(long)]
        asset_weight_init: f64,
        #[clap(long)]
        asset_weight_maint: f64,
        #[clap(long)]
        liability_weight_init: f64,
        #[clap(long)]
        liability_weight_maint: f64,
        #[clap(long)]
        deposit_limit: u64,
        #[clap(long)]
        borrow_limit: u64,
        #[clap(long)]
        pyth_oracle: Pubkey,
        #[clap(long)]
        optimal_utilization_rate: f64,
        #[clap(long)]
        plateau_interest_rate: f64,
        #[clap(long)]
        max_interest_rate: f64,
        #[clap(long)]
        insurance_fee_fixed_apr: f64,
        #[clap(long)]
        insurance_ir_fee: f64,
        #[clap(long)]
        protocol_fixed_fee_apr: f64,
        #[clap(long)]
        protocol_ir_fee: f64,
    },
}

#[derive(Clone, Copy, Debug, Parser, ArgEnum)]
pub enum BankOperationalStateArg {
    Paused,
    Operational,
    ReduceOnly,
}

impl From<BankOperationalStateArg> for BankOperationalState {
    fn from(val: BankOperationalStateArg) -> Self {
        match val {
            BankOperationalStateArg::Paused => BankOperationalState::Paused,
            BankOperationalStateArg::Operational => BankOperationalState::Operational,
            BankOperationalStateArg::ReduceOnly => BankOperationalState::ReduceOnly,
        }
    }
}

#[derive(Debug, Parser)]
pub enum BankCommand {
    Get {
        bank: Option<Pubkey>,
    },
    GetAll {
        marginfi_group: Option<Pubkey>,
    },
    #[cfg(feature = "admin")]
    Update {
        bank_pk: Pubkey,
        #[clap(long)]
        asset_weight_init: Option<f32>,
        #[clap(long)]
        asset_weight_maint: Option<f32>,

        #[clap(long)]
        liability_weight_init: Option<f32>,
        #[clap(long)]
        liability_weight_maint: Option<f32>,

        #[clap(long)]
        deposit_limit: Option<u64>,

        #[clap(long)]
        borrow_limit: Option<u64>,

        #[clap(long, arg_enum)]
        operational_state: Option<BankOperationalStateArg>,
    },
}

#[derive(Debug, Parser)]
pub enum ProfileCommand {
    Create {
        #[clap(long)]
        name: String,
        #[clap(long)]
        cluster: Cluster,
        #[clap(long)]
        keypair_path: String,
        #[clap(long)]
        rpc_url: String,
        #[clap(long)]
        program_id: Option<Pubkey>,
        #[clap(long)]
        commitment: Option<CommitmentLevel>,
        #[clap(long)]
        group: Option<Pubkey>,
        #[clap(long)]
        account: Option<Pubkey>,
    },
    Show,
    List,
    Set {
        name: String,
    },
    Update {
        name: String,
        #[clap(long)]
        cluster: Option<Cluster>,
        #[clap(long)]
        keypair_path: Option<String>,
        #[clap(long)]
        rpc_url: Option<String>,
        #[clap(long)]
        program_id: Option<Pubkey>,
        #[clap(long)]
        commitment: Option<CommitmentLevel>,
        #[clap(long)]
        group: Option<Pubkey>,
        #[clap(long)]
        account: Option<Pubkey>,
    },
}

#[derive(Debug, Parser)]
pub enum AccountCommand {
    List,
    Use {
        account: Pubkey,
    },
    Get {
        account: Option<Pubkey>,
    },
    Deposit {
        bank: Pubkey,
        ui_amount: f64,
    },
    Withdraw {
        bank: Pubkey,
        ui_amount: f64,
        #[clap(short = 'a', long = "all")]
        withdraw_all: bool,
    },
    Borrow {
        bank: Pubkey,
        ui_amount: f64,
    },
    Create,
}

#[derive(Debug, Parser)]
pub enum LipCommand {
    ListCampaigns,
    ListDeposits,
}

pub fn entry(opts: Opts) -> Result<()> {
    env_logger::init();

    match opts.command {
        Command::Group { subcmd } => group(subcmd, &opts.cfg_override),
        Command::Bank { subcmd } => bank(subcmd, &opts.cfg_override),
        Command::Profile { subcmd } => profile(subcmd),
        #[cfg(feature = "dev")]
        Command::InspectPadding {} => inspect_padding(),
        Command::Account { subcmd } => process_account_subcmd(subcmd, &opts.cfg_override),
        Command::Lip { subcmd } => process_lip_subcmd(subcmd, &opts.cfg_override),
    }
}

fn profile(subcmd: ProfileCommand) -> Result<()> {
    match subcmd {
        ProfileCommand::Create {
            name,
            cluster,
            keypair_path,
            rpc_url,
            program_id,
            commitment,
            group,
            account,
        } => processor::create_profile(
            name,
            cluster,
            keypair_path,
            rpc_url,
            program_id,
            commitment,
            group,
            account,
        ),
        ProfileCommand::Show => processor::show_profile(),
        ProfileCommand::List => processor::list_profiles(),
        ProfileCommand::Set { name } => processor::set_profile(name),
        ProfileCommand::Update {
            cluster,
            keypair_path,
            rpc_url,
            program_id,
            commitment,
            group,
            name,
            account,
        } => processor::configure_profile(
            name,
            cluster,
            keypair_path,
            rpc_url,
            program_id,
            commitment,
            group,
            account,
        ),
    }
}

fn group(subcmd: GroupCommand, global_options: &GlobalOptions) -> Result<()> {
    let profile = load_profile()?;
    let config = profile.get_config(Some(global_options))?;

    if !global_options.skip_confirmation {
        match subcmd {
            GroupCommand::Get { marginfi_group: _ } => (),
            GroupCommand::GetAll {} => (),
            _ => get_consent(&subcmd, &profile)?,
        }
    }

    match subcmd {
        GroupCommand::Get { marginfi_group } => {
            processor::group_get(config, marginfi_group.or(profile.marginfi_group))
        }
        GroupCommand::GetAll {} => processor::group_get_all(config),
        #[cfg(feature = "admin")]
        GroupCommand::Create {
            admin,
            override_existing_profile_group,
        } => processor::group_create(config, profile, admin, override_existing_profile_group),
        #[cfg(feature = "admin")]
        GroupCommand::Update { admin } => processor::group_configure(config, profile, admin),
        #[cfg(feature = "admin")]
        GroupCommand::AddBank {
            mint: bank_mint,
            asset_weight_init,
            asset_weight_maint,
            liability_weight_init,
            liability_weight_maint,
            pyth_oracle,
            optimal_utilization_rate,
            plateau_interest_rate,
            max_interest_rate,
            insurance_fee_fixed_apr,
            insurance_ir_fee,
            protocol_fixed_fee_apr,
            protocol_ir_fee,
            deposit_limit,
            borrow_limit,
        } => processor::group_add_bank(
            config,
            profile,
            bank_mint,
            pyth_oracle,
            asset_weight_init,
            asset_weight_maint,
            liability_weight_init,
            liability_weight_maint,
            deposit_limit,
            borrow_limit,
            optimal_utilization_rate,
            plateau_interest_rate,
            max_interest_rate,
            insurance_fee_fixed_apr,
            insurance_ir_fee,
            protocol_fixed_fee_apr,
            protocol_ir_fee,
        ),
    }
}

fn bank(subcmd: BankCommand, global_options: &GlobalOptions) -> Result<()> {
    let profile = load_profile()?;
    let config = profile.get_config(Some(global_options))?;

    if !global_options.skip_confirmation {
        match subcmd {
            BankCommand::Get { bank: _ } => (),
            BankCommand::GetAll { marginfi_group: _ } => (),
            _ => get_consent(&subcmd, &profile)?,
        }
    }

    match subcmd {
        BankCommand::Get { bank } => processor::bank_get(config, bank),
        BankCommand::GetAll { marginfi_group } => processor::bank_get_all(config, marginfi_group),
        #[cfg(feature = "admin")]
        BankCommand::Update {
            asset_weight_init,
            asset_weight_maint,
            liability_weight_init,
            liability_weight_maint,
            deposit_limit,
            borrow_limit,
            operational_state,
            bank_pk,
        } => processor::bank_configure(
            config,
            profile, //
            bank_pk,
            BankConfigOpt {
                asset_weight_init: asset_weight_init.map(|x| I80F48::from_num(x).into()),
                asset_weight_maint: asset_weight_maint.map(|x| I80F48::from_num(x).into()),
                liability_weight_init: liability_weight_init.map(|x| I80F48::from_num(x).into()),
                liability_weight_maint: liability_weight_maint.map(|x| I80F48::from_num(x).into()),
                deposit_limit,
                borrow_limit,
                operational_state: operational_state.map(|x| x.into()),
                oracle: None,
            },
        ),
    }
}

#[cfg(feature = "dev")]
fn inspect_padding() -> Result<()> {
    println!("MarginfiGroup: {}", MarginfiGroup::type_layout());
    println!("GroupConfig: {}", GroupConfig::type_layout());
    println!("InterestRateConfig: {}", InterestRateConfig::type_layout());
    println!("Bank: {}", Bank::type_layout());
    println!("BankConfig: {}", BankConfig::type_layout());
    println!("OracleConfig: {}", OracleConfig::type_layout());
    println!("BankConfigOpt: {}", BankConfigOpt::type_layout());
    println!("WrappedI80F48: {}", WrappedI80F48::type_layout());

    println!("MarginfiAccount: {}", MarginfiAccount::type_layout());
    println!("LendingAccount: {}", LendingAccount::type_layout());
    println!("Balance: {}", Balance::type_layout());

    Ok(())
}

fn process_account_subcmd(subcmd: AccountCommand, global_options: &GlobalOptions) -> Result<()> {
    let profile = load_profile()?;
    let config = profile.get_config(Some(global_options))?;

    if !global_options.skip_confirmation {
        match subcmd {
            AccountCommand::Get { .. } | AccountCommand::List => (),
            _ => get_consent(&subcmd, &profile)?,
        }
    }

    match subcmd {
        AccountCommand::List => processor::marginfi_account_list(profile, &config),
        AccountCommand::Use { account } => {
            processor::marginfi_account_use(profile, &config, account)
        }
        AccountCommand::Get { account } => {
            processor::marginfi_account_get(profile, &config, account)
        }
        AccountCommand::Deposit { bank, ui_amount } => {
            processor::marginfi_account_deposit(&profile, &config, bank, ui_amount)
        }
        AccountCommand::Withdraw {
            bank,
            ui_amount,
            withdraw_all,
        } => processor::marginfi_account_withdraw(&profile, &config, bank, ui_amount, withdraw_all),
        AccountCommand::Borrow { bank, ui_amount } => {
            processor::marginfi_account_borrow(&profile, &config, bank, ui_amount)
        }
        AccountCommand::Create => processor::marginfi_account_create(&profile, &config),
    }?;

    Ok(())
}

fn process_lip_subcmd(
    subcmd: LipCommand,
    cfg_override: &GlobalOptions,
) -> Result<(), anyhow::Error> {
    let profile = load_profile()?;
    let config = profile.get_config(Some(cfg_override))?;

    match subcmd {
        LipCommand::ListCampaigns => process_list_lip_campaigns(&config),
        LipCommand::ListDeposits => process_list_deposits(&config),
    }

    Ok(())
}

fn get_consent<T: std::fmt::Debug>(cmd: T, profile: &Profile) -> Result<()> {
    let mut input = String::new();
    println!("Command: {:#?}", cmd);
    println!("{:#?}", profile);
    println!(
        "Type the name of the profile [{}] to continue",
        profile.name.clone()
    );
    std::io::stdin().read_line(&mut input)?;
    if input.trim() != profile.name {
        println!("Aborting");
        std::process::exit(1);
    }

    Ok(())
}
