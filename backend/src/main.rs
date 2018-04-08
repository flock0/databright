extern crate web3;
extern crate ipfsapi;
use web3::contract::{Contract, Options};
use web3::types::{Address, FilterBuilder, BlockNumber};
use web3::futures::{Future, Stream};
use ipfsapi::IpfsApi;

fn main() {
	// WEB3 WEBSOCKET
    let (_eloop, transp) = web3::transports::WebSocket::new("ws://127.0.0.1:8546").unwrap();
    let web3 = web3::Web3::new(transp);
    println!("a");
    let accounts = web3.eth().accounts().wait().unwrap();
	println!("b");
    let my_acc = accounts[0];
    let balance = web3.eth().balance(my_acc, None).wait().unwrap();
    println!("Balance of {}: {}", my_acc, balance);
    // Accessing existing contract
    let contract_address: Address = "18c06846a71256d3af75f4b2948ba475e714b45b".parse().unwrap(); // Address of the deployed SimpleDatabase contract
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

	// SEND TRANSACTION
    let mut options = Options::default();
    options.gas = Some(200000.into());
    let result = contract.call("addShard", (my_acc, "test".to_string(),), my_acc, options);
    let unwrapped_res = result.wait().unwrap();
    println!("{}", unwrapped_res);
    
    // FILTER LOGS
    let filt = FilterBuilder::default().from_block(BlockNumber::Earliest).to_block(BlockNumber::Latest).address(vec![contract_address]).build();
    let mut sub = web3.eth_subscribe().subscribe_logs(filt).wait().unwrap();
	println!("Got subscription id: {:?}", sub.id());

	(&mut sub)
        .take(5)
        .for_each(|x| {
            println!("Got: {:?}", x);
            Ok(())
        })
        .wait()
        .unwrap();

	sub.unsubscribe();
}
