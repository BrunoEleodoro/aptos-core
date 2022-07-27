// Copyright (c) Aptos
// SPDX-License-Identifier: Apache-2.0

use crate::smoke_test_environment::new_local_swarm_with_aptos_and_config;
use crate::{
    smoke_test_environment::new_local_swarm_with_aptos,
    test_utils::{
        assert_balance, create_and_fund_account, transfer_coins, transfer_coins_non_blocking,
    },
};
use anyhow::{anyhow, bail, Context, Result};
use aptos_config::{
    config::{DiscoveryMethod, NodeConfig, Peer, PeerRole, HANDSHAKE_VERSION},
    network_id::NetworkId,
};
use aptos_types::network_address::{NetworkAddress, Protocol};
use forge::{NodeExt, Swarm, SwarmExt};
use std::sync::Arc;
use std::{
    collections::HashSet,
    net::Ipv4Addr,
    time::{Duration, Instant},
};

#[tokio::test]
async fn test_txn_broadcast() {
    let mut swarm = new_local_swarm_with_aptos(4).await;
    let transaction_factory = swarm.chain_info().transaction_factory();
    let version = swarm.versions().max().unwrap();
    let validator_peer_ids = swarm.validators().map(|v| v.peer_id()).collect::<Vec<_>>();

    let validator = validator_peer_ids[1];
    let vfn = swarm
        .add_validator_fullnode(
            &version,
            NodeConfig::default_for_validator_full_node(),
            validator,
        )
        .await
        .unwrap();

    for validator in swarm.validators_mut() {
        validator
            .wait_until_healthy(Instant::now() + Duration::from_secs(10))
            .await
            .unwrap();
    }
    for fullnode in swarm.full_nodes_mut() {
        fullnode
            .wait_until_healthy(Instant::now() + Duration::from_secs(10))
            .await
            .unwrap();
        fullnode
            .wait_for_connectivity(Instant::now() + Duration::from_secs(60))
            .await
            .unwrap();
    }

    // Setup accounts
    let mut account_0 = create_and_fund_account(&mut swarm, 10).await;
    let account_1 = create_and_fund_account(&mut swarm, 10).await;

    swarm
        .wait_for_all_nodes_to_catchup(Instant::now() + Duration::from_secs(10))
        .await
        .unwrap();

    // set up vfn_client
    let vfn_client = swarm.full_node(vfn).unwrap().rest_client();

    // set up validator_client. proposals not sent from this validator. txn should still go through.
    let validator_client = swarm.validator(validator).unwrap().rest_client();
    validator_client
        .set_failpoint("consensus::send_proposal".to_string(), "return".to_string())
        .await
        .unwrap();

    // send to validator_client
    let txn = transfer_coins(
        &validator_client,
        &transaction_factory,
        &mut account_0,
        &account_1,
        1,
    )
    .await;

    assert_balance(&validator_client, &account_0, 9).await;
    assert_balance(&validator_client, &account_1, 11).await;
    vfn_client.wait_for_signed_transaction(&txn).await.unwrap();
    assert_balance(&vfn_client, &account_0, 9).await;
    assert_balance(&vfn_client, &account_1, 11).await;

    transfer_coins(
        &vfn_client,
        &transaction_factory,
        &mut account_0,
        &account_1,
        1,
    )
    .await;

    assert_balance(&validator_client, &account_0, 8).await;
    assert_balance(&validator_client, &account_1, 12).await;
    assert_balance(&vfn_client, &account_0, 8).await;
    assert_balance(&vfn_client, &account_1, 12).await;
}
