extern crate web3;
use web3::contract::{Contract, Options};
use web3::types::{Address, U256};
use web3::futures::Future;

fn main() {
    let (_eloop, http) = web3::transports::Http::new("http://localhost:8545").unwrap();
    let web3 = web3::Web3::new(http);
    
    let accounts = web3.eth().accounts().wait().unwrap();

    let my_acc = accounts[0];
    let balance = web3.eth().balance(my_acc, None).wait().unwrap();
    println!("Balance of {}: {}", my_acc, balance);
    // Accessing existing contract
    let contract_address: Address = "f71a469e9a68b955687c8bd04062d00e5e888441".parse().unwrap(); // Address of the deployed SimpleDatabase contract
    let contract = Contract::from_json(
        web3.eth(),
        contract_address,
        include_bytes!("../../data_market/contracts/SimpleDatabase.abi"),
    ).unwrap();

    let mut options = Options::default();
    options.gas = Some(200000.into());
    let result = contract.call("addShard", (my_acc, "test".to_string(),), my_acc, options);
    result.wait();
    
    let get_result = contract.query("getShard", (0), None, Options::default(), None);
    let unwrapped_get_result: std::vec::Vec<Address> = get_result.wait().unwrap();
    println!("{:?}", unwrapped_get_result);
}
