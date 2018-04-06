extern crate web3;
use web3::contract::{Contract, Options};
use web3::types::{Address, U256, Filter, FilterBuilder};
use web3::futures::{Future, Stream};

fn main() {
    let (_eloop, transp) = web3::transports::WebSocket::new("ws://127.0.0.1:8545").unwrap();
    let web3 = web3::Web3::new(transp);
    println!("a");
    let accounts = web3.eth().accounts().wait().unwrap();
	println!("b");
    let my_acc = accounts[0];
    let balance = web3.eth().balance(my_acc, None).wait().unwrap();
    println!("Balance of {}: {}", my_acc, balance);
    // Accessing existing contract
    let contract_address: Address = "370917ed5fdf51702e6d5d742c5b3434112e84ce".parse().unwrap(); // Address of the deployed SimpleDatabase contract
    let contract = Contract::from_json(
        web3.eth(),
        contract_address,
        include_bytes!("../../data_market/contracts/SimpleDatabase.abi"),
    ).unwrap();

    let mut options = Options::default();
    options.gas = Some(200000.into());
    let result = contract.call("addShard", (my_acc, "test".to_string(),), my_acc, options);
    let unwrapped_res = result.wait().unwrap();
    println!("{}", unwrapped_res);
    
    let filt = FilterBuilder::default().address(vec![contract_address]).build();
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
