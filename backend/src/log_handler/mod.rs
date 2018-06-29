extern crate byteorder;
extern crate futures;
extern crate hyper;
extern crate tokio_core;
extern crate web3;
extern crate tar;

use self::byteorder::{BigEndian, ByteOrder};
use self::hyper::Chunk;
use futures::{Stream, IntoFuture};
use futures::future::{join_all, ok};
use ipfs_api;
use ipfs_api::IpfsClient;
use std;
use std::collections::HashMap;
use std::fs::{create_dir_all, remove_dir_all, File};
use std::io::Write;
use std::path::Path;
use std::str;
use web3::Web3;
use web3::contract::{Contract, Options};
use web3::futures::Future;
use web3::transports::WebSocket;
use web3::types::{Address, H256};
use log_handler::data_loader::data_loader::{DataLoader, CSVLoader};
use self::tar::Archive;

mod data_loader;
mod knn_shapley;

macro_rules! hashmap {
    ($( $key: expr => $val: expr ),*) => {{
         let mut map = ::std::collections::HashMap::new();
         $( map.insert($key, $val); )*
         map
    }}
}

pub fn handle_log<'a>(
    log: &web3::types::Log,
    topics: &HashMap<(&str, String), H256>,
    dba_contract: &Contract<WebSocket>,
    ipfs_client: &'a IpfsClient,
    web3: &'a Web3<WebSocket>,
    tmp_folder_location: &'a str,
    event_loop: &mut tokio_core::reactor::Core,
    cv_num_splits: usize
 ) {
    info!("Handling log: {:?}", log.topics[0]);

    if log.topics[0] == *topics.get(&("DatabaseAssociation", "ProposalAdded".into())).unwrap()
    {
        // Initialize event deserializer
        let order = vec![
            "proposalID",
            "recipient",
            "amount",
            "description",
            "argument",
            "argument2",
            "curator",
            "state",
        ];
        let fields: HashMap<&str, &str> = hashmap!["proposalID" => "uint",
                                                                     "recipient" => "address",
                                                                     "amount" => "uint",
                                                                     "description" => "string",
                                                                     "argument" => "string",
                                                                     "argument2" => "uint",
                                                                     "curator" => "address",
                                                                     "state" => "uint"];
        let propadded_deserializer = LogdataDeserializer::new(&log.data.0, fields, order);
        let state = propadded_deserializer.get_u64("state");
        debug!("Proposal has state {}", state);
        
        if state == 2 {
            // State == 2: This is a shard add proposal

            // The ProposalAdded log contains the proposalID. We use this to load the proposal from Ethereum.
            let propID = propadded_deserializer.get_u64("proposalID");
            let proposal_future =
                dba_contract.query::<(Address, u64, String, u64, bool, bool, u64, Vec<u8>, String, u64, Address, u64,), _, _, _>
                                    ("proposals", (propID,), None, Options::default(), None);
            let proposal = event_loop.run(proposal_future).unwrap();
            
            // The first field in the proposal struct is the address of the SimpleDatabase contract affected by the proposal
            let contract_address = proposal.0;

            let db_contract = Contract::from_json(
                web3.eth(),
                contract_address,
                include_bytes!("../../../marketplaces/build/SimpleDatabase.abi"),
            ).unwrap();

            let database_local_folder =
                Path::new(tmp_folder_location).join(format!("{:?}", contract_address));

            // The proposed shard will be fetched into a special folder
            let new_shard_folder = database_local_folder.join("new_shard");
            remove_dir_all(&new_shard_folder); 
            create_dir_all(new_shard_folder);   
            // To get all files from IPFS, we first need to fetch all shards from Ethereum
            // To loop through all shards in the array, the array length is needed.
            let arrlen_future = db_contract.query::<u64, _, _, _>(
                "getShardArrayLength",
                (),
                None,
                Options::default(),
                None,
            );
            let arrlen = event_loop.run(arrlen_future).unwrap();
            
            let mut all_shard_futures = Vec::new();

            for i in 0..arrlen {
                let shard_local_folder = database_local_folder.join(format!("shard_{}", i));
                if !shard_local_folder.exists() {
                    // The shard folder doesn't exist, let's create it
                    create_dir_all(shard_local_folder);
                    let shard_future = db_contract.query::<(Address, String, u64, u64), _, _, _>(
                                        "shards", (i,), None, Options::default(), None,);
                    all_shard_futures.push(shard_future);
                }
            }
            debug!("Fetching shards from database contract.");
            let shards = event_loop.run(join_all(all_shard_futures)).unwrap();
            debug!("Fetched {} shards from database contract.", shards.len());

            let shards_valid: Vec<bool> = shards.iter().map(|shard| shard.0 != Address::zero()).collect();
            let mut all_ls_futures = Vec::new();
            for (id, shard) in shards.iter().enumerate() {
                if shards_valid[id] {
                    debug!("Listing directory content for shard {} at IPFS address {}", id, &shard.1);
                    let ls_future = ipfs_client.ls(Some(&format!("/ipfs/{}", &shard.1)));
                    all_ls_futures.push(ls_future.join(ok(Some(id))));
                }
            }
            
            let new_shard_ipfs_hash = propadded_deserializer.get_str("argument");
            debug!("Manually adding proposed shard '{}' for retrieval from IPFS", new_shard_ipfs_hash);
            let new_shard_ls_future = ipfs_client.ls(Some(&format!("/ipfs/{}", new_shard_ipfs_hash)));
            all_ls_futures.push(new_shard_ls_future.join(ok(None)));

            debug!("Fetching directory listings from IPFS");
            let ls_results = event_loop.run(join_all(all_ls_futures)).unwrap();
            debug!("Fetched {} directory listings from IPFS", ls_results.len());

            let mut all_download_futures = Vec::new();
            for (ls_res, id_opt) in ls_results.iter() {
                let shard_local_folder = match id_opt {
                                            None => database_local_folder.join("new_shard"),
                                            Some(id) => database_local_folder.join(format!("shard_{}", id))
                                        };

                for link in ls_res.objects[0].links.iter() {
                    debug!("Getting IPFS file {} from {}", &link.name, &link.hash);
                    let get_fut = ipfs_client.get(&link.hash).concat2();
                    all_download_futures.push(get_fut.join(ok((shard_local_folder.join(&link.name), id_opt))));
                }
            }
            debug!("Fetching files from IPFS");
            let all_file_dls = event_loop.run(join_all(all_download_futures)).unwrap();
            debug!("Fetched {} files from IPFS", all_file_dls.len());

            for (file_res, (file_path, _)) in all_file_dls.iter() {
                debug!("Writing file {:?}", file_path);
                let file_content = file_res;
                let mut tar_archive = Archive::new(file_content.as_ref());
                for archive_file in tar_archive.entries().unwrap() {
                    archive_file.unwrap().unpack(file_path);
                    break;
                }
                
            }
            debug!("Written shards to disk. Starting knn-shapley approximation...");

            let shard_attributed_file_paths = all_file_dls.iter().map(|(_, (path, shard_id_opt))| (path.to_str().unwrap(), **shard_id_opt)).collect();
            
            //TODO Load real file from smart contract and ipfs
            let dataformat_json_path = Path::new("./lorem_ipsum.json");
            let ldr = self::data_loader::data_loader::new(dataformat_json_path);
            let csv_ldr = ldr as CSVLoader;
            let (features, predictors, shard_attributions) = csv_ldr.load_all_samples(shard_attributed_file_paths);
            let X = csv_ldr.vecs_as_matrix(features);
            let y = csv_ldr.vec_as_vector(predictors);

            let cv_shapleys = knn_shapley::knn_shapley::run_shapley_cv(&X, &y, cv_num_splits);
            debug!("Cross-validation shapleys on sample-level{:?}", cv_shapleys);
            // TODO Sum up shapleys of shards (to get one shapley value per shard)
            println!("{:?}", shard_attributions);
            // TODO Write it back to the blockchain
        }
    }
 }

pub struct LogdataDeserializer<'a> {
    order: Vec<&'a str>,
    fields: HashMap<&'a str, &'a str>, // field name -> data type
    data: &'a Vec<u8>,
    offsets: HashMap<&'a str, (usize, usize)>, // field name -> starting offset
}

impl<'a> LogdataDeserializer<'a> {
    pub fn new(
        data: &'a Vec<u8>,
        fields: HashMap<&'a str, &'a str>,
        order: Vec<&'a str>,
    ) -> LogdataDeserializer<'a> {
        let mut lds = LogdataDeserializer {
            data: data,
            fields: fields,
            order: order,
            offsets: HashMap::new(),
        };

        lds.findAndStoreOffsets();
        lds
    }

    fn findAndStoreOffsets(&mut self) {
        //loop through fields
        let mut currentBlockOffset: usize = 0;
        for name in &self.order {
            let datatype = self.fields.get(name).unwrap();
            let startOffset = currentBlockOffset;

            let offsetTuple = match *datatype {
                "uint" => (currentBlockOffset + 24, currentBlockOffset + 32), // = usize256 = 32 Bytes
                "address" => (currentBlockOffset + 12, currentBlockOffset + 32), // = usually only 160b, but padded to 256b = 32 bytes
                "string" => self.getStringOffsets(currentBlockOffset),
                _ => {
                    error!("Unsupported datatype in LogdataDeserializer");
                    (0, 0)
                }
            };
            currentBlockOffset += 32;
            self.offsets.insert(name, offsetTuple);
        }
    }

    fn getStringOffsets(&self, stringPointerBlockOffset: usize) -> (usize, usize) {
        let slice = &self.data[(stringPointerBlockOffset + 24)..(stringPointerBlockOffset + 32)]; // Only take the least significant 8 Bytes into account (see description above)
        let stringLengthBlockOffset = BigEndian::read_u64(slice) as usize;

        let slice = &self.data[(stringLengthBlockOffset + 24)..(stringLengthBlockOffset + 32)];
        let stringLength = BigEndian::read_u64(slice) as usize;

        let stringStartOffset = stringLengthBlockOffset + 32;
        let stringEndOffset = stringStartOffset + stringLength;

        (stringStartOffset, stringEndOffset)
    }

    pub fn get_u64(&self, field_name: &str) -> u64 {
        //get field offset
        let (start, end) = self.offsets.get(field_name).unwrap();
        //get data type
        let datatype = self.fields.get(field_name).unwrap();

        match *datatype {
            "uint" => {
                /* The default uint contains 256b = 32 Bytes
                 * Byteorder supports a max of 64b, hence we hackily only take the least significant 64b = 8 Bytes
                 * into account when converting to u64.
                 *
                 * Usage of uint should be discouraged, as it probably is too large for our usecases anyways
                 */

                let slice = &self.data[*start..*end];
                BigEndian::read_u64(slice)
            }
            _ => {
                error!("Couldn't read u64 in LogdataDeserializer");
                0
            }
        }
    }

    pub fn get_address(&self, field_name: &str) -> Address {
        //get field offset
        let (start, end) = self.offsets.get(field_name).unwrap();
        //get data type
        let datatype = self.fields.get(field_name).unwrap();

        match *datatype {
            "address" => {
                /* The default address contains 160b, but its padded to 256b = 32 bytes
                 * Hence we only take the least significant 160b = 20 Bytes
                 */

                let slice = &self.data[*start..*end];
                Address::from_slice(slice)
            }
            _ => {
                error!("Couldn't read address in LogdataDeserializer");
                Address::default()
            }
        }
    }

    pub fn get_str(&self, field_name: &str) -> &str {
        //get field offset
        let (start, end) = self.offsets.get(field_name).unwrap();
        //get data type
        let datatype = self.fields.get(field_name).unwrap();

        match *datatype {
            "string" => {
                let slice = &self.data[*start..*end];
                str::from_utf8(slice).unwrap()
            }
            _ => {
                error!("Couldn't read str in LogdataDeserializer");
                ""
            }
        }
    }
}
