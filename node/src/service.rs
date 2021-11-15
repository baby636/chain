/*
 * This file is part of the Nodle Chain distributed at https://github.com/NodleCode/chain
 * Copyright (C) 2020  Nodle International
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation, either version 3 of the License, or
 * (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License
 * along with this program.  If not, see <http://www.gnu.org/licenses/>.
 */

//! Service and ServiceFactory implementation. Specialized wrapper over substrate service.

pub use crate::client::{
    AbstractClient, Client, ClientHandle, ExecuteWithClient, RuntimeApiCollection,
};
use crate::rpc as node_rpc;
use futures::prelude::*;
use primitives::Block;
use sc_client_api::{ExecutorProvider, RemoteBackend};
use sc_consensus_babe::{self, SlotProportion};
use sc_executor::{NativeElseWasmExecutor, NativeExecutionDispatch};
use sc_finality_grandpa::{self, FinalityProofProvider as GrandpaFinalityProofProvider};
use sc_network::{Event, NetworkService};
use sc_service::{
    config::Configuration, error::Error as ServiceError, ChainSpec, RpcHandlers, TaskManager,
};
use sc_telemetry::{Telemetry, TelemetryWorker};
use sp_api::ConstructRuntimeApi;
use sp_runtime::traits::{BlakeTwo256, Block as BlockT};
use sp_trie::PrefixedMemoryDB;
use std::sync::Arc;

mod main_executor {
    pub use main_runtime;

    pub struct MainExecutorDispatch;
    impl sc_executor::NativeExecutionDispatch for MainExecutorDispatch {
        type ExtendHostFunctions = frame_benchmarking::benchmarking::HostFunctions;

        fn dispatch(method: &str, data: &[u8]) -> Option<Vec<u8>> {
            main_runtime::api::dispatch(method, data)
        }

        fn native_version() -> sc_executor::NativeVersion {
            main_runtime::native_version()
        }
    }
}

mod staking_executor {
    pub use staking_runtime;

    pub struct StakingExecutorDispatch;
    impl sc_executor::NativeExecutionDispatch for StakingExecutorDispatch {
        type ExtendHostFunctions = frame_benchmarking::benchmarking::HostFunctions;

        fn dispatch(method: &str, data: &[u8]) -> Option<Vec<u8>> {
            staking_runtime::api::dispatch(method, data)
        }

        fn native_version() -> sc_executor::NativeVersion {
            staking_runtime::native_version()
        }
    }
}

pub use main_executor::*;
pub use staking_executor::*;

pub type FullBackend = sc_service::TFullBackend<Block>;
pub type LightBackend = sc_service::TLightBackendWithHash<Block, sp_runtime::traits::BlakeTwo256>;

pub type FullClient<RuntimeApi, ExecutorDispatch> =
    sc_service::TFullClient<Block, RuntimeApi, NativeElseWasmExecutor<ExecutorDispatch>>;
pub type LightClient<RuntimeApi, ExecutorDispatch> = sc_service::TLightClientWithBackend<
    Block,
    RuntimeApi,
    NativeElseWasmExecutor<ExecutorDispatch>,
    LightBackend,
>;

pub type FullSelectChain = sc_consensus::LongestChain<FullBackend, Block>;
pub type FullGrandpaBlockImport<RuntimeApi, ExecutorDispatch> =
    sc_finality_grandpa::GrandpaBlockImport<
        FullBackend,
        Block,
        FullClient<RuntimeApi, ExecutorDispatch>,
        FullSelectChain,
    >;

pub type TransactionPool<RuntimeApi, ExecutorDispatch> =
    sc_transaction_pool::FullPool<Block, FullClient<RuntimeApi, ExecutorDispatch>>;

/// Can be called for a `Configuration` to check what node it belongs to.
pub trait IdentifyVariant {
    /// Returns if this is a configuration for the `Main` node.
    fn is_main_runtime(&self) -> bool;

    /// Returns if this is a configuration for the `Staking` node.
    fn is_staking_runtime(&self) -> bool;
}

impl IdentifyVariant for Box<dyn ChainSpec> {
    fn is_main_runtime(&self) -> bool {
        self.name().to_lowercase().starts_with("main")
    }
    fn is_staking_runtime(&self) -> bool {
        self.name().to_lowercase().starts_with("staking")
    }
}

fn main_new_partial(
    config: &Configuration,
) -> Result<
    sc_service::PartialComponents<
        FullClient<main_runtime::RuntimeApi, MainExecutorDispatch>,
        FullBackend,
        FullSelectChain,
        sc_consensus::DefaultImportQueue<
            Block,
            FullClient<main_runtime::RuntimeApi, MainExecutorDispatch>,
        >,
        TransactionPool<main_runtime::RuntimeApi, MainExecutorDispatch>,
        (
            impl Fn(
                node_rpc::DenyUnsafe,
                sc_rpc::SubscriptionTaskExecutor,
            ) -> Result<node_rpc::IoHandler, sc_service::Error>,
            (
                sc_consensus_babe::BabeBlockImport<
                    Block,
                    FullClient<main_runtime::RuntimeApi, MainExecutorDispatch>,
                    FullGrandpaBlockImport<main_runtime::RuntimeApi, MainExecutorDispatch>,
                >,
                sc_finality_grandpa::LinkHalf<
                    Block,
                    FullClient<main_runtime::RuntimeApi, MainExecutorDispatch>,
                    FullSelectChain,
                >,
                sc_consensus_babe::BabeLink<Block>,
            ),
            sc_finality_grandpa::SharedVoterState,
            Option<Telemetry>,
        ),
    >,
    ServiceError,
> {
    let telemetry = config
        .telemetry_endpoints
        .clone()
        .filter(|x| !x.is_empty())
        .map(|endpoints| -> Result<_, sc_telemetry::Error> {
            let worker = TelemetryWorker::new(16)?;
            let telemetry = worker.handle().new_telemetry(endpoints);
            Ok((worker, telemetry))
        })
        .transpose()?;

    let executor = NativeElseWasmExecutor::<MainExecutorDispatch>::new(
        config.wasm_method,
        config.default_heap_pages,
        config.max_runtime_instances,
    );

    let (client, backend, keystore_container, task_manager) = sc_service::new_full_parts::<
        Block,
        main_runtime::RuntimeApi,
        NativeElseWasmExecutor<MainExecutorDispatch>,
    >(
        &config,
        telemetry.as_ref().map(|(_, telemetry)| telemetry.handle()),
        executor,
    )?;

    let client = Arc::new(client);

    let telemetry = telemetry.map(|(worker, telemetry)| {
        task_manager.spawn_handle().spawn("telemetry", worker.run());
        telemetry
    });

    let select_chain = sc_consensus::LongestChain::new(backend.clone());

    let transaction_pool = sc_transaction_pool::BasicPool::new_full(
        config.transaction_pool.clone(),
        config.role.is_authority().into(),
        config.prometheus_registry(),
        task_manager.spawn_essential_handle(),
        client.clone(),
    );

    let (grandpa_block_import, grandpa_link) = sc_finality_grandpa::block_import(
        client.clone(),
        &(client.clone() as Arc<_>),
        select_chain.clone(),
        telemetry.as_ref().map(|x| x.handle()),
    )?;

    let justification_import = grandpa_block_import.clone();

    let (block_import, babe_link) = sc_consensus_babe::block_import(
        sc_consensus_babe::Config::get_or_compute(&*client)?,
        grandpa_block_import,
        client.clone(),
    )?;

    let slot_duration = babe_link.config().slot_duration();
    let import_queue = sc_consensus_babe::import_queue(
        babe_link.clone(),
        block_import.clone(),
        Some(Box::new(justification_import)),
        client.clone(),
        select_chain.clone(),
        move |_, ()| async move {
            let timestamp = sp_timestamp::InherentDataProvider::from_system_time();

            let slot =
                sp_consensus_babe::inherents::InherentDataProvider::from_timestamp_and_duration(
                    *timestamp,
                    slot_duration,
                );

            let uncles =
                sp_authorship::InherentDataProvider::<<Block as BlockT>::Header>::check_inherents();

            Ok((timestamp, slot, uncles))
        },
        &task_manager.spawn_essential_handle(),
        config.prometheus_registry(),
        sp_consensus::CanAuthorWithNativeVersion::new(client.executor().clone()),
        telemetry.as_ref().map(|x| x.handle()),
    )?;

    let import_setup = (block_import, grandpa_link, babe_link);

    let (rpc_extensions_builder, rpc_setup) = {
        let (_, grandpa_link, babe_link) = &import_setup;

        let justification_stream = grandpa_link.justification_stream();
        let shared_authority_set = grandpa_link.shared_authority_set().clone();
        let shared_voter_state = sc_finality_grandpa::SharedVoterState::empty();
        let rpc_setup = shared_voter_state.clone();

        let finality_proof_provider = GrandpaFinalityProofProvider::new_for_service(
            backend.clone(),
            Some(shared_authority_set.clone()),
        );

        let babe_config = babe_link.config().clone();
        let shared_epoch_changes = babe_link.epoch_changes().clone();

        let client = client.clone();
        let pool = transaction_pool.clone();
        let select_chain = select_chain.clone();
        let keystore = keystore_container.sync_keystore();
        let chain_spec = config.chain_spec.cloned_box();

        let rpc_extensions_builder = move |deny_unsafe, subscription_executor| {
            let deps = node_rpc::MainFullDeps {
                client: client.clone(),
                pool: pool.clone(),
                select_chain: select_chain.clone(),
                chain_spec: chain_spec.cloned_box(),
                deny_unsafe,
                babe: node_rpc::BabeDeps {
                    babe_config: babe_config.clone(),
                    shared_epoch_changes: shared_epoch_changes.clone(),
                    keystore: keystore.clone(),
                },
                grandpa: node_rpc::GrandpaDeps {
                    shared_voter_state: shared_voter_state.clone(),
                    shared_authority_set: shared_authority_set.clone(),
                    justification_stream: justification_stream.clone(),
                    subscription_executor,
                    finality_provider: finality_proof_provider.clone(),
                },
            };

            node_rpc::main_create_full(deps).map_err(Into::into)
        };

        (rpc_extensions_builder, rpc_setup)
    };

    Ok(sc_service::PartialComponents {
        client,
        backend,
        task_manager,
        keystore_container,
        select_chain,
        import_queue,
        transaction_pool,
        other: (rpc_extensions_builder, import_setup, rpc_setup, telemetry),
    })
}

/// Creates a full service from the configuration.
fn main_new_full_base(
    mut config: Configuration,
) -> Result<
    NewFullBase<Arc<FullClient<main_runtime::RuntimeApi, MainExecutorDispatch>>>,
    ServiceError,
> {
    let sc_service::PartialComponents {
        client,
        backend,
        mut task_manager,
        import_queue,
        keystore_container,
        select_chain,
        transaction_pool,
        other: (rpc_extensions_builder, import_setup, rpc_setup, mut telemetry),
    } = main_new_partial(&config)?;

    let shared_voter_state = rpc_setup;
    let auth_disc_publish_non_global_ips = config.network.allow_non_globals_in_dht;

    config
        .network
        .extra_sets
        .push(sc_finality_grandpa::grandpa_peers_set_config());

    let warp_sync = Arc::new(sc_finality_grandpa::warp_proof::NetworkProvider::new(
        backend.clone(),
        import_setup.1.shared_authority_set().clone(),
        Vec::default(),
    ));

    let (network, system_rpc_tx, network_starter) =
        sc_service::build_network(sc_service::BuildNetworkParams {
            config: &config,
            client: client.clone(),
            transaction_pool: transaction_pool.clone(),
            spawn_handle: task_manager.spawn_handle(),
            import_queue,
            on_demand: None,
            block_announce_validator_builder: None,
            warp_sync: Some(warp_sync),
        })?;

    if config.offchain_worker.enabled {
        sc_service::build_offchain_workers(
            &config,
            task_manager.spawn_handle(),
            client.clone(),
            network.clone(),
        );
    }

    let role = config.role.clone();
    let force_authoring = config.force_authoring;
    let backoff_authoring_blocks =
        Some(sc_consensus_slots::BackoffAuthoringOnFinalizedHeadLagging::default());
    let name = config.network.node_name.clone();
    let enable_grandpa = !config.disable_grandpa;
    let prometheus_registry = config.prometheus_registry().cloned();

    let _rpc_handlers = sc_service::spawn_tasks(sc_service::SpawnTasksParams {
        config,
        backend: backend.clone(),
        client: client.clone(),
        keystore: keystore_container.sync_keystore(),
        network: network.clone(),
        rpc_extensions_builder: Box::new(rpc_extensions_builder),
        transaction_pool: transaction_pool.clone(),
        task_manager: &mut task_manager,
        on_demand: None,
        remote_blockchain: None,
        system_rpc_tx,
        telemetry: telemetry.as_mut(),
    })?;

    let (block_import, grandpa_link, babe_link) = import_setup;

    if let sc_service::config::Role::Authority { .. } = &role {
        let proposer = sc_basic_authorship::ProposerFactory::new(
            task_manager.spawn_handle(),
            client.clone(),
            transaction_pool.clone(),
            prometheus_registry.as_ref(),
            telemetry.as_ref().map(|x| x.handle()),
        );

        let can_author_with =
            sp_consensus::CanAuthorWithNativeVersion::new(client.executor().clone());

        let client_clone = client.clone();
        let slot_duration = babe_link.config().slot_duration();
        let babe_config = sc_consensus_babe::BabeParams {
            keystore: keystore_container.sync_keystore(),
            client: client.clone(),
            select_chain,
            env: proposer,
            block_import,
            sync_oracle: network.clone(),
            justification_sync_link: network.clone(),
            create_inherent_data_providers: move |parent, ()| {
                let client_clone = client_clone.clone();
                async move {
                    let uncles = sc_consensus_uncles::create_uncles_inherent_data_provider(
                        &*client_clone,
                        parent,
                    )?;

                    let timestamp = sp_timestamp::InherentDataProvider::from_system_time();

                    let slot =
						sp_consensus_babe::inherents::InherentDataProvider::from_timestamp_and_duration(
							*timestamp,
							slot_duration,
						);

                    let storage_proof =
                        sp_transaction_storage_proof::registration::new_data_provider(
                            &*client_clone,
                            &parent,
                        )?;

                    Ok((timestamp, slot, uncles, storage_proof))
                }
            },
            force_authoring,
            backoff_authoring_blocks,
            babe_link,
            can_author_with,
            block_proposal_slot_portion: SlotProportion::new(0.5),
            max_block_proposal_slot_portion: None,
            telemetry: telemetry.as_ref().map(|x| x.handle()),
        };

        let babe = sc_consensus_babe::start_babe(babe_config)?;
        task_manager
            .spawn_essential_handle()
            .spawn_blocking("babe-proposer", babe);
    }

    // Spawn authority discovery module.
    if role.is_authority() {
        let authority_discovery_role =
            sc_authority_discovery::Role::PublishAndDiscover(keystore_container.keystore());
        let dht_event_stream =
            network
                .event_stream("authority-discovery")
                .filter_map(|e| async move {
                    match e {
                        Event::Dht(e) => Some(e),
                        _ => None,
                    }
                });
        let (authority_discovery_worker, _service) =
            sc_authority_discovery::new_worker_and_service_with_config(
                sc_authority_discovery::WorkerConfig {
                    publish_non_global_ips: auth_disc_publish_non_global_ips,
                    ..Default::default()
                },
                client.clone(),
                network.clone(),
                Box::pin(dht_event_stream),
                authority_discovery_role,
                prometheus_registry.clone(),
            );

        task_manager.spawn_handle().spawn(
            "authority-discovery-worker",
            authority_discovery_worker.run(),
        );
    }

    // if the node isn't actively participating in consensus then it doesn't
    // need a keystore, regardless of which protocol we use below.
    let keystore = if role.is_authority() {
        Some(keystore_container.sync_keystore())
    } else {
        None
    };

    let config = sc_finality_grandpa::Config {
        // FIXME #1578 make this available through chainspec
        gossip_duration: std::time::Duration::from_millis(333),
        justification_period: 512,
        name: Some(name),
        observer_enabled: false,
        keystore,
        local_role: role,
        telemetry: telemetry.as_ref().map(|x| x.handle()),
    };

    if enable_grandpa {
        // start the full GRANDPA voter
        // NOTE: non-authorities could run the GRANDPA observer protocol, but at
        // this point the full voter should provide better guarantees of block
        // and vote data availability than the observer. The observer has not
        // been tested extensively yet and having most nodes in a network run it
        // could lead to finality stalls.
        let grandpa_config = sc_finality_grandpa::GrandpaParams {
            config,
            link: grandpa_link,
            network: network.clone(),
            telemetry: telemetry.as_ref().map(|x| x.handle()),
            voting_rule: sc_finality_grandpa::VotingRulesBuilder::default().build(),
            prometheus_registry,
            shared_voter_state,
        };

        // the GRANDPA voter task is considered infallible, i.e.
        // if it fails we take down the service with it.
        task_manager.spawn_essential_handle().spawn_blocking(
            "grandpa-voter",
            sc_finality_grandpa::run_grandpa_voter(grandpa_config)?,
        );
    }

    network_starter.start_network();

    Ok(NewFullBase {
        task_manager,
        client,
        network,
    })
}

fn new_partial<RuntimeApi, ExecutorDispatch>(
    config: &Configuration,
) -> Result<
    sc_service::PartialComponents<
        FullClient<RuntimeApi, ExecutorDispatch>,
        FullBackend,
        FullSelectChain,
        sc_consensus::DefaultImportQueue<Block, FullClient<RuntimeApi, ExecutorDispatch>>,
        TransactionPool<RuntimeApi, ExecutorDispatch>,
        (
            impl Fn(
                node_rpc::DenyUnsafe,
                sc_rpc::SubscriptionTaskExecutor,
            ) -> Result<node_rpc::IoHandler, sc_service::Error>,
            (
                sc_consensus_babe::BabeBlockImport<
                    Block,
                    FullClient<RuntimeApi, ExecutorDispatch>,
                    FullGrandpaBlockImport<RuntimeApi, ExecutorDispatch>,
                >,
                sc_finality_grandpa::LinkHalf<
                    Block,
                    FullClient<RuntimeApi, ExecutorDispatch>,
                    FullSelectChain,
                >,
                sc_consensus_babe::BabeLink<Block>,
            ),
            sc_finality_grandpa::SharedVoterState,
            Option<Telemetry>,
        ),
    >,
    ServiceError,
>
where
    RuntimeApi: ConstructRuntimeApi<Block, FullClient<RuntimeApi, ExecutorDispatch>>
        + Send
        + Sync
        + 'static,
    RuntimeApi::RuntimeApi:
        RuntimeApiCollection<StateBackend = sc_client_api::StateBackendFor<FullBackend, Block>>,
    ExecutorDispatch: NativeExecutionDispatch + 'static,
{
    let telemetry = config
        .telemetry_endpoints
        .clone()
        .filter(|x| !x.is_empty())
        .map(|endpoints| -> Result<_, sc_telemetry::Error> {
            let worker = TelemetryWorker::new(16)?;
            let telemetry = worker.handle().new_telemetry(endpoints);
            Ok((worker, telemetry))
        })
        .transpose()?;

    let executor = NativeElseWasmExecutor::<ExecutorDispatch>::new(
        config.wasm_method,
        config.default_heap_pages,
        config.max_runtime_instances,
    );

    let (client, backend, keystore_container, task_manager) =
        sc_service::new_full_parts::<Block, RuntimeApi, NativeElseWasmExecutor<ExecutorDispatch>>(
            &config,
            telemetry.as_ref().map(|(_, telemetry)| telemetry.handle()),
            executor,
        )?;

    let client = Arc::new(client);

    let telemetry = telemetry.map(|(worker, telemetry)| {
        task_manager.spawn_handle().spawn("telemetry", worker.run());
        telemetry
    });

    let select_chain = sc_consensus::LongestChain::new(backend.clone());

    let transaction_pool = sc_transaction_pool::BasicPool::new_full(
        config.transaction_pool.clone(),
        config.role.is_authority().into(),
        config.prometheus_registry(),
        task_manager.spawn_essential_handle(),
        client.clone(),
    );

    let (grandpa_block_import, grandpa_link) = sc_finality_grandpa::block_import(
        client.clone(),
        &(client.clone() as Arc<_>),
        select_chain.clone(),
        telemetry.as_ref().map(|x| x.handle()),
    )?;

    let justification_import = grandpa_block_import.clone();

    let (block_import, babe_link) = sc_consensus_babe::block_import(
        sc_consensus_babe::Config::get_or_compute(&*client)?,
        grandpa_block_import,
        client.clone(),
    )?;

    let slot_duration = babe_link.config().slot_duration();
    let import_queue = sc_consensus_babe::import_queue(
        babe_link.clone(),
        block_import.clone(),
        Some(Box::new(justification_import)),
        client.clone(),
        select_chain.clone(),
        move |_, ()| async move {
            let timestamp = sp_timestamp::InherentDataProvider::from_system_time();

            let slot =
                sp_consensus_babe::inherents::InherentDataProvider::from_timestamp_and_duration(
                    *timestamp,
                    slot_duration,
                );

            let uncles =
                sp_authorship::InherentDataProvider::<<Block as BlockT>::Header>::check_inherents();

            Ok((timestamp, slot, uncles))
        },
        &task_manager.spawn_essential_handle(),
        config.prometheus_registry(),
        sp_consensus::CanAuthorWithNativeVersion::new(client.executor().clone()),
        telemetry.as_ref().map(|x| x.handle()),
    )?;

    let import_setup = (block_import, grandpa_link, babe_link);

    let (rpc_extensions_builder, rpc_setup) = {
        let (_, grandpa_link, babe_link) = &import_setup;

        let justification_stream = grandpa_link.justification_stream();
        let shared_authority_set = grandpa_link.shared_authority_set().clone();
        let shared_voter_state = sc_finality_grandpa::SharedVoterState::empty();
        let rpc_setup = shared_voter_state.clone();

        let finality_proof_provider = GrandpaFinalityProofProvider::new_for_service(
            backend.clone(),
            Some(shared_authority_set.clone()),
        );

        let babe_config = babe_link.config().clone();
        let shared_epoch_changes = babe_link.epoch_changes().clone();

        let client = client.clone();
        let pool = transaction_pool.clone();
        let select_chain = select_chain.clone();
        let keystore = keystore_container.sync_keystore();
        let chain_spec = config.chain_spec.cloned_box();

        let rpc_extensions_builder = move |deny_unsafe, subscription_executor| {
            let deps = node_rpc::FullDeps {
                client: client.clone(),
                pool: pool.clone(),
                select_chain: select_chain.clone(),
                chain_spec: chain_spec.cloned_box(),
                deny_unsafe,
                babe: node_rpc::BabeDeps {
                    babe_config: babe_config.clone(),
                    shared_epoch_changes: shared_epoch_changes.clone(),
                    keystore: keystore.clone(),
                },
                grandpa: node_rpc::GrandpaDeps {
                    shared_voter_state: shared_voter_state.clone(),
                    shared_authority_set: shared_authority_set.clone(),
                    justification_stream: justification_stream.clone(),
                    subscription_executor,
                    finality_provider: finality_proof_provider.clone(),
                },
            };

            node_rpc::create_full(deps).map_err(Into::into)
        };

        (rpc_extensions_builder, rpc_setup)
    };

    Ok(sc_service::PartialComponents {
        client,
        backend,
        task_manager,
        keystore_container,
        select_chain,
        import_queue,
        transaction_pool,
        other: (rpc_extensions_builder, import_setup, rpc_setup, telemetry),
    })
}

/// Creates a full service from the configuration.
fn new_full_base<RuntimeApi, ExecutorDispatch>(
    mut config: Configuration,
) -> Result<NewFullBase<Arc<FullClient<RuntimeApi, ExecutorDispatch>>>, ServiceError>
where
    RuntimeApi: ConstructRuntimeApi<Block, FullClient<RuntimeApi, ExecutorDispatch>>
        + Send
        + Sync
        + 'static,
    RuntimeApi::RuntimeApi:
        RuntimeApiCollection<StateBackend = sc_client_api::StateBackendFor<FullBackend, Block>>,
    ExecutorDispatch: NativeExecutionDispatch + 'static,
{
    let sc_service::PartialComponents {
        client,
        backend,
        mut task_manager,
        import_queue,
        keystore_container,
        select_chain,
        transaction_pool,
        other: (rpc_extensions_builder, import_setup, rpc_setup, mut telemetry),
    } = new_partial::<RuntimeApi, ExecutorDispatch>(&config)?;

    let shared_voter_state = rpc_setup;
    let auth_disc_publish_non_global_ips = config.network.allow_non_globals_in_dht;

    config
        .network
        .extra_sets
        .push(sc_finality_grandpa::grandpa_peers_set_config());

    let warp_sync = Arc::new(sc_finality_grandpa::warp_proof::NetworkProvider::new(
        backend.clone(),
        import_setup.1.shared_authority_set().clone(),
        Vec::default(),
    ));

    let (network, system_rpc_tx, network_starter) =
        sc_service::build_network(sc_service::BuildNetworkParams {
            config: &config,
            client: client.clone(),
            transaction_pool: transaction_pool.clone(),
            spawn_handle: task_manager.spawn_handle(),
            import_queue,
            on_demand: None,
            block_announce_validator_builder: None,
            warp_sync: Some(warp_sync),
        })?;

    if config.offchain_worker.enabled {
        sc_service::build_offchain_workers(
            &config,
            task_manager.spawn_handle(),
            client.clone(),
            network.clone(),
        );
    }

    let role = config.role.clone();
    let force_authoring = config.force_authoring;
    let backoff_authoring_blocks =
        Some(sc_consensus_slots::BackoffAuthoringOnFinalizedHeadLagging::default());
    let name = config.network.node_name.clone();
    let enable_grandpa = !config.disable_grandpa;
    let prometheus_registry = config.prometheus_registry().cloned();

    let _rpc_handlers = sc_service::spawn_tasks(sc_service::SpawnTasksParams {
        config,
        backend: backend.clone(),
        client: client.clone(),
        keystore: keystore_container.sync_keystore(),
        network: network.clone(),
        rpc_extensions_builder: Box::new(rpc_extensions_builder),
        transaction_pool: transaction_pool.clone(),
        task_manager: &mut task_manager,
        on_demand: None,
        remote_blockchain: None,
        system_rpc_tx,
        telemetry: telemetry.as_mut(),
    })?;

    let (block_import, grandpa_link, babe_link) = import_setup;

    if let sc_service::config::Role::Authority { .. } = &role {
        let proposer = sc_basic_authorship::ProposerFactory::new(
            task_manager.spawn_handle(),
            client.clone(),
            transaction_pool.clone(),
            prometheus_registry.as_ref(),
            telemetry.as_ref().map(|x| x.handle()),
        );

        let can_author_with =
            sp_consensus::CanAuthorWithNativeVersion::new(client.executor().clone());

        let client_clone = client.clone();
        let slot_duration = babe_link.config().slot_duration();
        let babe_config = sc_consensus_babe::BabeParams {
            keystore: keystore_container.sync_keystore(),
            client: client.clone(),
            select_chain,
            env: proposer,
            block_import,
            sync_oracle: network.clone(),
            justification_sync_link: network.clone(),
            create_inherent_data_providers: move |parent, ()| {
                let client_clone = client_clone.clone();
                async move {
                    let uncles = sc_consensus_uncles::create_uncles_inherent_data_provider(
                        &*client_clone,
                        parent,
                    )?;

                    let timestamp = sp_timestamp::InherentDataProvider::from_system_time();

                    let slot =
						sp_consensus_babe::inherents::InherentDataProvider::from_timestamp_and_duration(
							*timestamp,
							slot_duration,
						);

                    let storage_proof =
                        sp_transaction_storage_proof::registration::new_data_provider(
                            &*client_clone,
                            &parent,
                        )?;

                    Ok((timestamp, slot, uncles, storage_proof))
                }
            },
            force_authoring,
            backoff_authoring_blocks,
            babe_link,
            can_author_with,
            block_proposal_slot_portion: SlotProportion::new(0.5),
            max_block_proposal_slot_portion: None,
            telemetry: telemetry.as_ref().map(|x| x.handle()),
        };

        let babe = sc_consensus_babe::start_babe(babe_config)?;
        task_manager
            .spawn_essential_handle()
            .spawn_blocking("babe-proposer", babe);
    }

    // Spawn authority discovery module.
    if role.is_authority() {
        let authority_discovery_role =
            sc_authority_discovery::Role::PublishAndDiscover(keystore_container.keystore());
        let dht_event_stream =
            network
                .event_stream("authority-discovery")
                .filter_map(|e| async move {
                    match e {
                        Event::Dht(e) => Some(e),
                        _ => None,
                    }
                });
        let (authority_discovery_worker, _service) =
            sc_authority_discovery::new_worker_and_service_with_config(
                sc_authority_discovery::WorkerConfig {
                    publish_non_global_ips: auth_disc_publish_non_global_ips,
                    ..Default::default()
                },
                client.clone(),
                network.clone(),
                Box::pin(dht_event_stream),
                authority_discovery_role,
                prometheus_registry.clone(),
            );

        task_manager.spawn_handle().spawn(
            "authority-discovery-worker",
            authority_discovery_worker.run(),
        );
    }

    // if the node isn't actively participating in consensus then it doesn't
    // need a keystore, regardless of which protocol we use below.
    let keystore = if role.is_authority() {
        Some(keystore_container.sync_keystore())
    } else {
        None
    };

    let config = sc_finality_grandpa::Config {
        // FIXME #1578 make this available through chainspec
        gossip_duration: std::time::Duration::from_millis(333),
        justification_period: 512,
        name: Some(name),
        observer_enabled: false,
        keystore,
        local_role: role,
        telemetry: telemetry.as_ref().map(|x| x.handle()),
    };

    if enable_grandpa {
        // start the full GRANDPA voter
        // NOTE: non-authorities could run the GRANDPA observer protocol, but at
        // this point the full voter should provide better guarantees of block
        // and vote data availability than the observer. The observer has not
        // been tested extensively yet and having most nodes in a network run it
        // could lead to finality stalls.
        let grandpa_config = sc_finality_grandpa::GrandpaParams {
            config,
            link: grandpa_link,
            network: network.clone(),
            telemetry: telemetry.as_ref().map(|x| x.handle()),
            voting_rule: sc_finality_grandpa::VotingRulesBuilder::default().build(),
            prometheus_registry,
            shared_voter_state,
        };

        // the GRANDPA voter task is considered infallible, i.e.
        // if it fails we take down the service with it.
        task_manager.spawn_essential_handle().spawn_blocking(
            "grandpa-voter",
            sc_finality_grandpa::run_grandpa_voter(grandpa_config)?,
        );
    }

    network_starter.start_network();

    Ok(NewFullBase {
        task_manager,
        client,
        network,
    })
}

pub struct NewFullBase<C> {
    pub task_manager: TaskManager,
    // pub inherent_data_providers: InherentDataProviders,
    pub client: C,
    pub network: Arc<NetworkService<Block, <Block as BlockT>::Hash>>,
    // pub network_status_sinks: sc_service::NetworkStatusSinks<Block>,
}

impl<C> NewFullBase<C> {
    /// Convert the client type using the given `func`.
    pub fn with_client<NC>(self, func: impl FnOnce(C) -> NC) -> NewFullBase<NC> {
        NewFullBase {
            task_manager: self.task_manager,
            client: func(self.client),
            network: self.network,
        }
    }
}

pub fn build_full(
    config: Configuration,
    run_staking_runtime: bool,
) -> Result<NewFullBase<Client>, ServiceError> {
    if run_staking_runtime {
        return new_full_base::<staking_runtime::RuntimeApi, StakingExecutorDispatch>(config)
            .map(|full| full.with_client(Client::StakingRT));
    } else {
        return main_new_full_base(config).map(|full| full.with_client(Client::MainRT));
    }
}

pub fn new_chain_ops(
    mut config: &mut Configuration,
) -> Result<
    (
        Arc<Client>,
        Arc<FullBackend>,
        sc_consensus::import_queue::BasicQueue<Block, PrefixedMemoryDB<BlakeTwo256>>,
        TaskManager,
    ),
    ServiceError,
> {
    config.keystore = sc_service::config::KeystoreConfig::InMemory;
    if config.chain_spec.is_staking_runtime() {
        let sc_service::PartialComponents {
            client,
            backend,
            import_queue,
            task_manager,
            ..
        } = new_partial::<staking_runtime::RuntimeApi, StakingExecutorDispatch>(config)?;
        Ok((
            Arc::new(Client::StakingRT(client)),
            backend,
            import_queue,
            task_manager,
        ))
    } else {
        let sc_service::PartialComponents {
            client,
            backend,
            import_queue,
            task_manager,
            ..
        } = new_partial::<main_runtime::RuntimeApi, MainExecutorDispatch>(config)?;
        Ok((
            Arc::new(Client::MainRT(client)),
            backend,
            import_queue,
            task_manager,
        ))
    }
}

pub fn new_light_base<RuntimeApi, ExecutorDispatch>(
    mut config: Configuration,
) -> Result<
    (
        TaskManager,
        RpcHandlers,
        Arc<LightClient<RuntimeApi, ExecutorDispatch>>,
        Arc<NetworkService<Block, <Block as BlockT>::Hash>>,
        Arc<
            sc_transaction_pool::LightPool<
				Block,
				LightClient<RuntimeApi, ExecutorDispatch>,
				sc_network::config::OnDemand<Block>,
			>,
        >,
    ),
    ServiceError,
>
where
	RuntimeApi: 'static + Send + Sync + ConstructRuntimeApi<Block, LightClient<RuntimeApi, ExecutorDispatch>>,
	<RuntimeApi as ConstructRuntimeApi<Block, LightClient<RuntimeApi, ExecutorDispatch>>>::RuntimeApi:
		RuntimeApiCollection<StateBackend = sc_client_api::StateBackendFor<LightBackend, Block>>,
	ExecutorDispatch: NativeExecutionDispatch + 'static,
{
    let telemetry = config
        .telemetry_endpoints
        .clone()
        .filter(|x| !x.is_empty())
        .map(|endpoints| -> Result<_, sc_telemetry::Error> {
            let worker = TelemetryWorker::new(16)?;
            let telemetry = worker.handle().new_telemetry(endpoints);
            Ok((worker, telemetry))
        })
        .transpose()?;

    let executor = NativeElseWasmExecutor::<ExecutorDispatch>::new(
        config.wasm_method,
        config.default_heap_pages,
        config.max_runtime_instances,
    );

    let (client, backend, keystore_container, mut task_manager, on_demand) =
        sc_service::new_light_parts::<Block, RuntimeApi, _>(
            &config,
            telemetry.as_ref().map(|(_, telemetry)| telemetry.handle()),
            executor,
        )?;

    let mut telemetry = telemetry.map(|(worker, telemetry)| {
        task_manager.spawn_handle().spawn("telemetry", worker.run());
        telemetry
    });

    config
        .network
        .extra_sets
        .push(sc_finality_grandpa::grandpa_peers_set_config());

    let select_chain = sc_consensus::LongestChain::new(backend.clone());

    let transaction_pool = Arc::new(sc_transaction_pool::BasicPool::new_light(
        config.transaction_pool.clone(),
        config.prometheus_registry(),
        task_manager.spawn_essential_handle(),
        client.clone(),
        on_demand.clone(),
    ));

    let (grandpa_block_import, grandpa_link) = sc_finality_grandpa::block_import(
        client.clone(),
        &(client.clone() as Arc<_>),
        select_chain.clone(),
        telemetry.as_ref().map(|x| x.handle()),
    )?;
    let justification_import = grandpa_block_import.clone();

    let (babe_block_import, babe_link) = sc_consensus_babe::block_import(
        sc_consensus_babe::Config::get_or_compute(&*client)?,
        grandpa_block_import,
        client.clone(),
    )?;

    let slot_duration = babe_link.config().slot_duration();
    let import_queue = sc_consensus_babe::import_queue(
        babe_link,
        babe_block_import,
        Some(Box::new(justification_import)),
        client.clone(),
        select_chain.clone(),
        move |_, ()| async move {
            let timestamp = sp_timestamp::InherentDataProvider::from_system_time();

            let slot =
                sp_consensus_babe::inherents::InherentDataProvider::from_timestamp_and_duration(
                    *timestamp,
                    slot_duration,
                );

            let uncles =
                sp_authorship::InherentDataProvider::<<Block as BlockT>::Header>::check_inherents();

            Ok((timestamp, slot, uncles))
        },
        &task_manager.spawn_essential_handle(),
        config.prometheus_registry(),
        sp_consensus::NeverCanAuthor,
        telemetry.as_ref().map(|x| x.handle()),
    )?;

    let warp_sync = Arc::new(sc_finality_grandpa::warp_proof::NetworkProvider::new(
        backend.clone(),
        grandpa_link.shared_authority_set().clone(),
        Vec::default(),
    ));

    let (network, system_rpc_tx, network_starter) =
        sc_service::build_network(sc_service::BuildNetworkParams {
            config: &config,
            client: client.clone(),
            transaction_pool: transaction_pool.clone(),
            spawn_handle: task_manager.spawn_handle(),
            import_queue,
            on_demand: Some(on_demand.clone()),
            block_announce_validator_builder: None,
            warp_sync: Some(warp_sync),
        })?;

    let enable_grandpa = !config.disable_grandpa;
    if enable_grandpa {
        let name = config.network.node_name.clone();

        let config = sc_finality_grandpa::Config {
            gossip_duration: std::time::Duration::from_millis(333),
            justification_period: 512,
            name: Some(name),
            observer_enabled: false,
            keystore: None,
            local_role: config.role.clone(),
            telemetry: telemetry.as_ref().map(|x| x.handle()),
        };

        task_manager.spawn_handle().spawn_blocking(
            "grandpa-observer",
            sc_finality_grandpa::run_grandpa_observer(config, grandpa_link, network.clone())?,
        );
    }

    if config.offchain_worker.enabled {
        sc_service::build_offchain_workers(
            &config,
            task_manager.spawn_handle(),
            client.clone(),
            network.clone(),
        );
    }

    let light_deps = node_rpc::LightDeps {
        remote_blockchain: backend.remote_blockchain(),
        fetcher: on_demand.clone(),
        client: client.clone(),
        pool: transaction_pool.clone(),
    };

    let rpc_extensions = node_rpc::create_light(light_deps);

    let rpc_handlers = sc_service::spawn_tasks(sc_service::SpawnTasksParams {
        on_demand: Some(on_demand),
        remote_blockchain: Some(backend.remote_blockchain()),
        rpc_extensions_builder: Box::new(sc_service::NoopRpcExtensionBuilder(rpc_extensions)),
        client: client.clone(),
        transaction_pool: transaction_pool.clone(),
        keystore: keystore_container.sync_keystore(),
        config,
        backend,
        system_rpc_tx,
        network: network.clone(),
        task_manager: &mut task_manager,
        telemetry: telemetry.as_mut(),
    })?;

    network_starter.start_network();

    Ok((
        task_manager,
        rpc_handlers,
        client,
        network,
        transaction_pool,
    ))
}

pub fn build_light(
    config: Configuration,
    is_staking_runtime: bool,
) -> Result<TaskManager, ServiceError> {
    if is_staking_runtime {
        new_light_base::<staking_runtime::RuntimeApi, StakingExecutorDispatch>(config)
            .map(|(task_manager, _, _, _, _)| task_manager)
    } else {
        new_light_base::<main_runtime::RuntimeApi, MainExecutorDispatch>(config)
            .map(|(task_manager, _, _, _, _)| task_manager)
    }
}
