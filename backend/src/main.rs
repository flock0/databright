extern crate ini;
extern crate csv;
extern crate tokio_core;
extern crate web3;
#[macro_use] extern crate log;
extern crate env_logger;
extern crate ipfs_api;
extern crate futures;

use ini::Ini;
use std::collections::HashMap;
use web3::contract::Contract;
use web3::types::{Address, FilterBuilder, BlockNumber, H256};
use web3::futures::{Future, Stream};
use std::str::FromStr;
use std::fs::remove_dir_all;
use futures::future::{join_all, ok};
use ipfs_api::IpfsClient;

mod log_handler;

fn main() {

    env_logger::init();

    // Extract configuration from config.ini
    info!("Extracting configuration from config.ini..");
    let conf = Ini::load_from_file("config.ini").unwrap();
    let contracts_section = conf.section(Some("Contracts".to_owned())).unwrap();
    let contracts = contracts_section.get("contracts").unwrap();
    let contract_address: Address = contracts_section.get("DatabaseAssociation").unwrap().parse().unwrap();
    info!("Contracts to use: {}", contracts);
    info!("DatabaseAssociation to use: {}", contract_address);

    let web3_section = conf.section(Some("Web3".to_owned())).unwrap();
    let ws_url = web3_section.get("websocket_transport_url").unwrap();
    let replay_past_events = {
        let replay_string = web3_section.get("replay_past_events").unwrap();
        match replay_string.parse::<bool>() {
            Ok(b) => b,
            Err(_) => {error!("Couldn't parse replay_past_events from configuration. Skipping events from the past.."); false },
        }
    };

    let last_processed_block_id = web3_section.get("last_processed_block_id").unwrap();

    let ipfs_section = conf.section(Some("Ipfs".to_owned())).unwrap();
    let ipfs_node_ip = ipfs_section.get("node_ip").unwrap();
    let ipfs_node_port = ipfs_section.get("node_port").unwrap();
    let mut event_loop = tokio_core::reactor::Core::new().unwrap();
    let ipfs_client = IpfsClient::new(ipfs_node_ip, ipfs_node_port.parse::<u16>().unwrap()).unwrap();

    let loghandling_section = conf.section(Some("LogHandling".to_owned())).unwrap();
    let tmp_folder_location = loghandling_section.get("temp_data_storage_path").unwrap();
    let reset_local_data_storage = {
        let rest_string = loghandling_section.get("reset_local_data_storage").unwrap();
        match rest_string.parse::<bool>() {
            Ok(b) => b,
            Err(_) => {error!("Couldn't parse reset_local_data_storage from configuration. Not reseting local folder.."); false },
        }
    };
    let cv_num_splits = {
        let cv_num_splits_string = loghandling_section.get("cv_num_splits").unwrap();
        match cv_num_splits_string.parse::<usize>() {
            Ok(b) => b,
            Err(_) => {error!("Couldn't parse cv_num_splits from configuration. Using 10 splits..."); 10 },
        }
    };

    // Populate topic hashmap
    info!("Loading topic hashes from ../marketplaces/build/*.topic files..");
    let mut topics: HashMap<(&str, String), H256> = HashMap::new();
    for contract in contracts.split(",") {
        let mut rdr = csv::Reader::from_path(format!("../marketplaces/build/{}.topic", contract)).unwrap();
        
        for rec in rdr.records() {
            let rr = rec.unwrap();
            let event_name = rr.get(0).unwrap();
            let topic_hash = rr.get(1).unwrap();
            let topic_bytes = match H256::from_str(topic_hash) {
                Ok(hash) => hash,
                Err(err) => {error!("Couldn't convert hash of {} topic from CSV file: {}", event_name, err); H256::default()},
            };
            topics.insert((contract, event_name.to_owned()), topic_bytes);
        }
    }

    // Optionally we delete all previously downloaded dataset shards.
    // They will be re-downloaded when handling the logs
    if reset_local_data_storage {
        remove_dir_all(tmp_folder_location);
    }

    // Connect to ethereum node
    info!("Connecting to ethereum node at {}", ws_url);
    let transp = web3::transports::WebSocket::with_event_loop(ws_url, &event_loop.handle()).unwrap();
    let web3 = web3::Web3::new(transp);

    // Print accounts and balance to check if websocket connection works
    let accounts = web3.eth().accounts().map(|accounts| {
        debug!("Accounts on node: {:?}", accounts);
        accounts[0]
	});

	let accs = event_loop.run(accounts).unwrap();
	let bal = web3.eth().balance(accs, None).map(|balance| {
		debug!("Balance of {}: {}", accs, balance);
	});
    event_loop.run(bal).unwrap();

    let contract = Contract::from_json(
        web3.eth(),
        contract_address,
        include_bytes!("../../marketplaces/build/DatabaseAssociation.abi"),
    ).unwrap();

    // To filter for specific events:
    let desired_topics: std::option::Option<std::vec::Vec<web3::types::H256>> = Some(
        vec![*topics.get(&("DatabaseAssociation", "ProposalAdded".into())).unwrap()]);
    
    let num_events = match desired_topics {
        Some(ref vec) => vec.len(),
        None => 0
    };
    if num_events == 0 {
        info!("Listening for all events");
    } else {
        info!("Listening for {} events", num_events);
    }

    // Retrieve logs since last processed block
    if replay_past_events {
        info!("replay_past_events is true. Will replay events from the past..");
        let from_block = if last_processed_block_id.is_empty() {
                BlockNumber::Earliest
            } else {
                match last_processed_block_id.parse::<u64>() {
                    Ok(n) => BlockNumber::Number(n),
                    Err(_) => {warn!("Couldn't parse last_processed_block_id from configuration. Starting from earliest block..."); BlockNumber::Earliest },
                }
            };
        info!("Replaying events from {:?} to latest block", from_block);
        let filter = FilterBuilder::default()
            .address(vec![contract.address()])
            .from_block(from_block)
            .to_block(BlockNumber::Latest)
            .topics(
                desired_topics.clone(),
                None,
                None,
                None,
            )
            .build();

        let log_future = web3.eth_filter()
            .create_logs_filter(filter)
            .and_then(|filter| filter.logs());


        let logs = event_loop.run(log_future).unwrap();
        for log in logs {
            log_handler::handle_log(&log, &topics, &contract, &ipfs_client, &web3, &tmp_folder_location, &mut event_loop, cv_num_splits);
        }
        info!("Finished replay of events");
    }
}