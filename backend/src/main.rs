extern crate web3;
extern crate ipfsapi;
extern crate tokio_core;
use web3::contract::{Contract, Options};
use web3::types::{Address, FilterBuilder, BlockNumber};
use web3::futures::{Future, Stream};
use ipfsapi::IpfsApi;
use std::time::Duration;

fn main() {

	let mut event_loop = tokio_core::reactor::Core::new().unwrap();
    let handle = event_loop.handle();
	// WEB3 WEBSOCKET
    let transp = web3::transports::WebSocket::with_event_loop("ws://127.0.0.1:8546", &handle).unwrap();

    let web3 = web3::Web3::new(transp);
    println!("a");
    let accounts = web3.eth().accounts().map(|accounts| {
        println!("Accounts: {:?}", accounts);
        accounts[0]
	});
	
	let my_acc = event_loop.run(accounts).unwrap();
	let bal = web3.eth().balance(my_acc, None).map(|balance| {
		println!("Balance of {}: {}", my_acc, balance);
	});	
	event_loop.run(bal).unwrap();
    //println!("Balance of {}: {}", my_acc, balance);
    // Accessing existing contract
    let contract_address: Address = "504aeef21184dc59f01be167264c60bfa8560699".parse().unwrap(); // Address of the deployed SimpleDatabase contract
    let contract = Contract::from_json(
        web3.eth(),
        contract_address,
        include_bytes!("../../data_market/build/SimpleDatabase.abi"),
    ).unwrap();

    // IPFS
	let api = IpfsApi::new("127.0.0.1", 5001);

	let bytes = api.cat("QmWATWQ7fVPP2EFGu71UkfnqhYXDYH566qy47CnJDgvs8u").unwrap();
	let data = String::from_utf8(bytes.collect()).unwrap();

	println!("{}", data);

	/*
	// SEND TRANSACTION
    let mut options = Options::default();
    options.gas = Some(200000.into());
    let result = contract.call("addShard", (my_acc, "test".to_string(),), my_acc, options);
    let unwrapped_res = result.wait().unwrap();
    println!("{}", unwrapped_res);
    let mut options = Options::default();
    options.gas = Some(200000.into());
    let result = contract.call("addShard", (my_acc, "test2".to_string(),), my_acc, options);
    let unwrapped_res = result.wait().unwrap();
    println!("{}", unwrapped_res);
    */

    // FILTER PAST LOGS
    println!("c");
    //let filt = FilterBuilder::default().address(vec![contract_address]).limit(10).build();
    // .from_block(BlockNumber::Earliest).to_block(BlockNumber::Latest).
    let filt = FilterBuilder::default().build();
    println!("d");
    let logfilter = web3.eth_filter().create_logs_filter(filt);
    let filter_stream = event_loop.run(logfilter).unwrap();
    println!("e");
    let logg = filter_stream.poll();

    /*
    let print_logs = filter_stream
    	.for_each(|x| {
            println!("Got: {:?}", x);
            Ok(())
        });
    println!("f");
    */
    let returnval = event_loop.run(logg).unwrap();
    println!("{:?}", returnval);
    
    /*let mut sub = web3.eth_subscribe().subscribe_logs(filt).wait().unwrap();
	println!("Got subscription id: {:?}", sub.id());

	(&mut sub)
        .for_each(|x| {
            println!("Got: {:?}", x);
            Ok(())
        })
        .wait()
        .unwrap();

	sub.unsubscribe();
	*/
}
