use std::{
    panic, process,
    str::FromStr,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread::available_parallelism,
    time::Duration,
    vec,
};

use bytemuck::Contiguous;
use concurrent_queue::ConcurrentQueue;
use crossbeam::channel::TryRecvError;
use dotenv::dotenv;
use envconfig::Envconfig;
use event_indexer::{
    backfiller::{
        crawl_signatures_for_range, generate_ranges, get_default_before_signature, Range,
        TransactionData, MARGINFI_PROGRAM_GENESIS_SIG,
    },
    error::IndexingError,
    parser::{MarginfiEventParser, MarginfiEventWithMeta, MARGINFI_GROUP_ADDRESS},
};
use rpc_utils::conversion::convert_encoded_ui_transaction;
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::{
    commitment_config::{CommitmentConfig, CommitmentLevel},
    signature::Signature,
};
use tokio::time::interval;
use tracing::{debug, error, info, warn};
use tracing_subscriber::{layer::SubscriberExt, EnvFilter};

#[derive(Envconfig, Debug, Clone)]
pub struct Config {
    #[envconfig(from = "RPC_HOST")]
    pub rpc_host: String,
    #[envconfig(from = "RPC_TOKEN")]
    pub rpc_token: String,
    #[envconfig(from = "BEFORE_SIGNATURE")]
    pub before: Option<String>,
    #[envconfig(from = "UNTIL_SIGNATURE")]
    pub until: Option<String>,
    #[envconfig(from = "DATABASE_URL")]
    pub database_url: String,
    #[envconfig(from = "GOOGLE_APPLICATION_CREDENTIALS_JSON")]
    pub gcp_sa_key: String,
    #[envconfig(from = "PRETTY_LOGS")]
    pub pretty_logs: Option<bool>,
}

#[tokio::main]
pub async fn main() {
    dotenv().ok();

    let orig_hook = panic::take_hook();
    panic::set_hook(Box::new(move |panic_info| {
        orig_hook(panic_info);
        process::exit(1);
    }));

    let config = Config::init_from_env().unwrap();

    let pretty_logs = config.pretty_logs.unwrap_or(false);

    let filter = EnvFilter::from_default_env();
    let stackdriver = tracing_stackdriver::layer(); // writes to std::io::Stdout
    let subscriber = tracing_subscriber::registry().with(filter);
    if pretty_logs {
        let subscriber = subscriber.with(tracing_subscriber::fmt::layer().compact());
        tracing::subscriber::set_global_default(subscriber).unwrap();
    } else {
        let subscriber = subscriber.with(stackdriver);
        tracing::subscriber::set_global_default(subscriber).unwrap();
    };

    let rpc_endpoint = format!("{}/{}", config.rpc_host, config.rpc_token).to_string();
    let rpc_client = RpcClient::new_with_commitment(
        rpc_endpoint.clone(),
        CommitmentConfig {
            commitment: CommitmentLevel::Confirmed,
        },
    );

    let default_before_sig = get_default_before_signature(&rpc_client).await;
    let before_sig = Signature::from_str(&config.before.unwrap_or(default_before_sig)).unwrap();
    let until_sig = Signature::from_str(
        &config
            .until
            .unwrap_or(MARGINFI_PROGRAM_GENESIS_SIG.to_string()),
    )
    .unwrap();

    let threads = available_parallelism()
        .map(|c| c.into_integer() as usize)
        .unwrap_or(1);

    let fetcher_threads = 5;
    let processor_threads = threads - fetcher_threads;

    let mut fetcher_tasks: Vec<std::thread::JoinHandle<Result<(), IndexingError>>> = vec![];

    let range_queue = Arc::new(ConcurrentQueue::<Range>::bounded(threads * 2));
    let is_range_complete = Arc::new(AtomicBool::new(false));

    let range_queue_clone = range_queue.clone();
    let rpc_endpoint_clone = rpc_endpoint.clone();
    let is_range_complete_clone = is_range_complete.clone();
    fetcher_tasks.push(std::thread::spawn(move || {
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let rpc_client = RpcClient::new_with_commitment(
                rpc_endpoint_clone,
                CommitmentConfig {
                    commitment: CommitmentLevel::Confirmed,
                },
            );
            generate_ranges(
                &marginfi::ID,
                rpc_client,
                before_sig,
                until_sig,
                &range_queue_clone,
                is_range_complete_clone,
            )
            .await
            .map_err(|e| IndexingError::FailedToGenerateRange(e.to_string()))?;

            info!("Finished generating ranges");

            Ok(())
        })
    }));

    let (transaction_tx, transaction_rx) = crossbeam::channel::unbounded::<TransactionData>();

    let is_fetching_complete = Arc::new(AtomicBool::new(false));
    for i in 0..fetcher_threads {
        info!("Spawning fetcher thread: {:?}", i);

        let local_transaction_tx = transaction_tx.clone();
        let rpc_client = RpcClient::new_with_commitment(
            rpc_endpoint.clone(),
            CommitmentConfig {
                commitment: CommitmentLevel::Confirmed,
            },
        );
        let range_queue = range_queue.clone();
        let is_range_complete = is_range_complete.clone();

        fetcher_tasks.push(std::thread::spawn(move || {
            tokio::runtime::Runtime::new().unwrap().block_on(async {
                let mut timer = interval(Duration::from_millis(200));
                while range_queue.len() > 0 || !is_range_complete.load(Ordering::Relaxed) {
                    if local_transaction_tx.len() > 1000 {
                        warn!("[{:?}] Tx backlog too long. Waiting 30 seconds...", i);
                        tokio::time::sleep(Duration::from_secs(30)).await;
                        continue;
                    }

                    if let Ok(Range {
                        before_sig,
                        until_sig,
                        before_slot,
                        until_slot,
                        progress,
                    }) = range_queue.pop()
                    {
                        debug!(
                            "[{:?}] Processing {:?} -> {:?} ({:?} -> {:?})",
                            i, until_slot, before_slot, until_sig, before_sig
                        );
                        crawl_signatures_for_range(
                            i as u64,
                            &rpc_client,
                            marginfi::ID,
                            before_sig,
                            until_sig,
                            &local_transaction_tx,
                            None,
                        )
                        .await?;
                        debug!(
                            "[{:?}] {:.2?}% completed {:?} - {:?} ({:?} -> {:?})",
                            i, progress, until_slot, before_slot, until_sig, before_sig
                        );
                    }

                    timer.tick().await;
                }

                info!("Fetcher thread {:?} done", i);

                Ok(())
            })
        }));
    }
    let is_fetching_complete_clone = is_fetching_complete.clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(1)).await;
            if fetcher_tasks.iter().all(|task| task.is_finished()) {
                is_fetching_complete_clone.store(true, Ordering::Relaxed);
                break;
            }
        }
        Ok::<(), String>(())
    });

    let mut processor_tasks: Vec<std::thread::JoinHandle<Result<(), IndexingError>>> = vec![];

    let mut print_time = std::time::Instant::now();

    for i in 0..processor_threads {
        info!("Spawning processor thread: {:?}", i);

        let local_transaction_rx = transaction_rx.clone();
        let is_fetching_complete_clone = is_fetching_complete.clone();
        #[cfg(not(feature = "dry-run"))]
        let rpc_endpoint = rpc_endpoint.clone();
        #[cfg(not(feature = "dry-run"))]
        let database_url = config.database_url.clone();

        processor_tasks.push(std::thread::spawn(move || {
            tokio::runtime::Runtime::new().unwrap().block_on(async {
                let parser = MarginfiEventParser::new(marginfi::ID, MARGINFI_GROUP_ADDRESS);
                loop {
                    let elapsed = print_time.elapsed().as_secs();
                    if elapsed > 30 {
                        debug!("Queue size: {:?}", local_transaction_rx.len());
                        print_time = std::time::Instant::now();
                    }

                    let maybe_item = local_transaction_rx.try_recv();

                    let TransactionData {
                        transaction,
                        ..
                    } = match maybe_item {
                        Err(TryRecvError::Empty) => {
                            if is_fetching_complete_clone.load(Ordering::Relaxed) {
                                info!("Processor thread {} done", i);
                                break;
                            } else {
                                std::thread::sleep(Duration::from_millis(100));
                                continue;
                            }
                        }
                        Err(TryRecvError::Disconnected) => {
                            error!("Transaction channel disconnected! Exiting...");
                            break;
                        }
                        Ok(tx_data) => tx_data,
                    };

                    let timestamp = transaction.block_time.unwrap();
                    let slot = transaction.slot;
                    let versioned_tx_with_meta =
                        convert_encoded_ui_transaction(transaction.transaction).unwrap();

                    let events = parser.extract_events(timestamp, slot, versioned_tx_with_meta);

                    #[cfg(feature = "dry-run")]
                    {
                        for MarginfiEventWithMeta {
                            event,
                            tx_sig,
                            ..
                        } in events
                        {
                            info!("Event: {:?} ({:?})", event, tx_sig);
                        }
                    }


                    #[cfg(not(feature = "dry-run"))]
                    {
                        use backoff::{retry, ExponentialBackoffBuilder};
                        use chrono::DateTime;
                        use event_indexer::{parser::MarginfiEvent, entity_store::EntityStore, db::establish_connection};
                        
                        let mut entity_store = EntityStore::new(rpc_endpoint.clone(), database_url.clone());
                        let mut db_connection = establish_connection(database_url.clone());

                        for MarginfiEventWithMeta {
                            event,
                            timestamp,
                            slot,
                            in_flashloan,
                            call_stack,
                            tx_sig,
                            outer_ix_index,
                            inner_ix_index,
                        } in events
                        {
                            let timestamp = DateTime::from_timestamp(timestamp, 0).unwrap().naive_utc();
                            let tx_sig = tx_sig.to_string();
                            let call_stack = serde_json::to_string(
                                &call_stack
                                    .into_iter()
                                    .map(|cs| cs.to_string())
                                    .collect::<Vec<_>>(),
                            )
                            .unwrap_or_else(|_| "null".to_string());

                            let mut retries = 0;
                            retry(
                                ExponentialBackoffBuilder::new()
                                    .with_max_interval(Duration::from_secs(5))
                                    .build(),
                                || match event.db_insert(
                                    timestamp,
                                    slot,
                                    tx_sig.clone(),
                                    in_flashloan,
                                    call_stack.clone(),
                                    outer_ix_index,
                                    inner_ix_index,
                                    &mut db_connection,
                                    &mut entity_store,
                                ) {
                                    Ok(signatures) => Ok(signatures),
                                    Err(e) => {
                                        if retries > 5 {
                                            error!(
                                        "[{:?}] Failed to insert event after 5 retries: {:?} - {:?} ({:?})",
                                        i, event, e, tx_sig
                                    );
                                            Err(backoff::Error::permanent(e))
                                        } else {
                                            warn!(
                                            "[{:?}] Failed to insert event, retrying: {:?} - {:?} ({:?})",
                                            i, event, e, tx_sig
                                        );
                                            retries += 1;
                                            Err(backoff::Error::transient(e))
                                        }
                                    }
                                },
                            )
                            .unwrap();
                        }
                    }
                }
            });

            Ok(())
        }))
    }

    loop {
        tokio::time::sleep(Duration::from_secs(1)).await;
        if processor_tasks.iter().all(|task| task.is_finished()) {
            break;
        }
    }
}
