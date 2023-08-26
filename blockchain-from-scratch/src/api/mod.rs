use crate::blockchain::Blockchain;
use crate::miner::Handle as MinerHandle;
use crate::network::message::Message;
use crate::network::server::Handle as NetworkServerHandle;
use crate::txgenerator::Generator;
use crate::txgenerator::Handle as TxHandle;
use crate::types::block;
use crate::types::hash::{Hashable, H256};
use crate::types::key_pair;
use crate::types::transaction::generate_random_transaction;
use crate::types::transaction::{sign, SignedTransaction};
use crossbeam::channel::{unbounded, Receiver, Sender, TryRecvError};
use ring::signature::{Ed25519KeyPair, KeyPair, Signature};
use serde::Serialize;

// use core::num::flt2dec::Sign;
use log::info;
use std::collections::HashMap;
use std::convert::TryInto;
use std::sync::{Arc, Mutex};
use std::thread;
use tiny_http::Header;
use tiny_http::Response;
use tiny_http::Server as HTTPServer;
use url::Url;

pub struct Server {
    handle: HTTPServer,
    miner: MinerHandle,
    pub network: NetworkServerHandle,
    pub blockchain: Arc<Mutex<Blockchain>>,
    pub mempool: Arc<Mutex<HashMap<H256, SignedTransaction>>>,
    pub generator: TxHandle,
}

#[derive(Serialize)]
struct ApiResponse {
    success: bool,
    message: String,
}

macro_rules! respond_result {
    ( $req:expr, $success:expr, $message:expr ) => {{
        let content_type = "Content-Type: application/json".parse::<Header>().unwrap();
        let payload = ApiResponse {
            success: $success,
            message: $message.to_string(),
        };
        let resp = Response::from_string(serde_json::to_string_pretty(&payload).unwrap())
            .with_header(content_type);
        $req.respond(resp).unwrap();
    }};
}
macro_rules! respond_json {
    ( $req:expr, $message:expr ) => {{
        let content_type = "Content-Type: application/json".parse::<Header>().unwrap();
        let resp = Response::from_string(serde_json::to_string(&$message).unwrap())
            .with_header(content_type);
        $req.respond(resp).unwrap();
    }};
}

impl Server {
    pub fn start(
        addr: std::net::SocketAddr,
        miner: &MinerHandle,
        network: &NetworkServerHandle,
        blockchain: &Arc<Mutex<Blockchain>>,
        mempool: &Arc<Mutex<HashMap<H256, SignedTransaction>>>,
        generator: &TxHandle,
    ) {
        let handle = HTTPServer::http(&addr).unwrap();
        let mut server = Self {
            handle,
            miner: miner.clone(),
            network: network.clone(),
            blockchain: Arc::clone(blockchain),
            mempool: Arc::clone(mempool),
            generator: generator.clone(),
        };

        thread::spawn(move || {
            for req in server.handle.incoming_requests() {
                let miner = server.miner.clone();
                let network = server.network.clone();
                let blockchain = Arc::clone(&server.blockchain);

                let generator = server.generator.clone();
                // let pool = Arc::clone(&server.mempool);
                thread::spawn(move || {
                    // a valid url requires a base
                    let base_url = Url::parse(&format!("http://{}/", &addr)).unwrap();
                    let url = match base_url.join(req.url()) {
                        Ok(u) => u,
                        Err(e) => {
                            respond_result!(req, false, format!("error parsing url: {}", e));
                            return;
                        }
                    };
                    match url.path() {
                        "/miner/start" => {
                            let params = url.query_pairs();
                            let params: HashMap<_, _> = params.into_owned().collect();
                            let lambda = match params.get("lambda") {
                                Some(v) => v,
                                None => {
                                    respond_result!(req, false, "missing lambda");
                                    return;
                                }
                            };
                            let lambda = match lambda.parse::<u64>() {
                                Ok(v) => v,
                                Err(e) => {
                                    respond_result!(
                                        req,
                                        false,
                                        format!("error parsing lambda: {}", e)
                                    );
                                    return;
                                }
                            };
                            println!("STARTING MINER");
                            miner.start(lambda);
                            respond_result!(req, true, "ok");
                        }
                        "/tx-generator/start" => {
                            let params = url.query_pairs();
                            let params: HashMap<_, _> = params.into_owned().collect();
                            let theta = match params.get("theta") {
                                Some(v) => v,
                                None => {
                                    respond_result!(req, false, "missing theta");
                                    return;
                                }
                            };
                            let theta = match theta.parse::<u64>() {
                                Ok(v) => v,
                                Err(e) => {
                                    respond_result!(
                                        req,
                                        false,
                                        format!("error parsing lambda: {}", e)
                                    );
                                    return;
                                }
                            };

                            println!("GENERATOR STARTED");
                            generator.start(theta);
                            respond_result!(req, true, "ok");
                        }
                        "/network/ping" => {
                            network.broadcast(Message::Ping(String::from("Test ping")));
                            respond_result!(req, true, "ok");
                        }
                        "/blockchain/longest-chain" => {
                            println!("getting LC1");
                            let blockchain = blockchain.lock().unwrap();
                            println!("getting LC");
                            let v = blockchain.all_blocks_in_longest_chain();
                            drop(blockchain);
                            let v_string: Vec<String> =
                                v.into_iter().map(|h| h.to_string()).collect();
                            respond_json!(req, v_string);
                        }
                        "/blockchain/longest-chain-tx" => {
                            // unimplemented!()
                            let blockchain = blockchain.lock().unwrap();
                            let v = blockchain.all_blocks_in_longest_chain();
                            let mut strs: Vec<Vec<String>> = Vec::new();

                            for i in 0..v.len() {
                                if !blockchain.block_map.contains_key(&v[i]) {
                                    continue;
                                }
                                let b = blockchain.block_map.get(&v[i]).unwrap();
                                let txs: Vec<SignedTransaction> = b.content.content.clone();
                                let mut tx_strs: Vec<String> = Vec::new();

                                // tx_strs.push(format!("TRANSACTS FOR BLOCK {}", i));
                                println!("BLOCK {} HAS {} TX INSIDE", i, txs.len());
                                for j in 0..txs.len() {
                                    tx_strs.push(txs[j].clone().hash().to_string());
                                }
                                // tx_strs.push(format!("{} TOTAL TX IN BLOCK", txs.len()));
                                strs.push(tx_strs);
                            }

                            // let v_string: Vec<String> =
                            //     v.into_iter().map(|h| h.to_string()).collect();
                            drop(blockchain);
                            respond_json!(req, strs);
                        }
                        "/blockchain/state" => {
                            let params = url.query_pairs();
                            let params: HashMap<_, _> = params.into_owned().collect();
                            let block = match params.get("block") {
                                Some(v) => v,
                                None => {
                                    respond_result!(req, false, "missing theta");
                                    return;
                                }
                            };
                            let block = match block.parse::<u64>() {
                                Ok(v) => v,
                                Err(e) => {
                                    respond_result!(
                                        req,
                                        false,
                                        format!("error parsing lambda: {}", e)
                                    );
                                    return;
                                }
                            };

                            let block: usize = block.try_into().unwrap();

                            let blockchain = blockchain.lock().unwrap();
                            let LC = blockchain.all_blocks_in_longest_chain();
                            let mut err = false;

                            if block >= LC.len().try_into().unwrap() {
                                err = true;
                            }

                            // if !blockchain.state_map.contains_key(&ith_block.hash()) {
                            //     err = true;
                            // }

                            let mut output_vec: Vec<Vec<String>> = Vec::new();
                            if !err {
                                let ith_block_hash = LC[block];
                                // let ith_block_state =
                                //     blockchain.state_map.get(&blockchain.tip()).unwrap();
                                // for (address, account) in
                                //     ith_block_state.account_state_map.clone().into_iter()
                                // {
                                //     let entry: Vec<String> = vec![
                                //         format!("ADDRESS: {}", address),
                                //         format!("ACCOUNT NONCE: {}", account.account_nonce),
                                //         format!("BALANCE: {}", account.balance),
                                //     ];
                                //     output_vec.push((entry));
                                // }

                                let ith_block_state =
                                    blockchain.state_map.get(&ith_block_hash).unwrap();
                                for (address, account) in
                                    ith_block_state.account_state_map.clone().into_iter()
                                {
                                    let entry: Vec<String> = vec![
                                        format!("ADDRESS: {}", address),
                                        format!("ACCOUNT NONCE: {}", account.account_nonce),
                                        format!("BALANCE: {}", account.balance),
                                    ];
                                    output_vec.push((entry));
                                }
                            } else {
                                output_vec.push(vec![
                                    "ERROR, TRIED TO GET BLOCK OUTSIDE OF LC".to_string()
                                ]);
                            }

                            respond_json!(req, output_vec);
                        }
                        "/blockchain/show-state" => {
                            let blockchain = blockchain.lock().unwrap();
                            let LC = blockchain.all_blocks_in_longest_chain();
                            let length = LC.clone().len();
                            let states = blockchain.state_map.clone();
                            let mut err = false;

                            let mut output_vec: Vec<Vec<String>> = Vec::new();

                            // for (hash, state) in blockchain.state_map.clone().into_iter() {
                            //     let mut block_entry: Vec<Vec<String>> = Vec::new();
                            //     block_entry.push(vec![format!("STATE AT BLOCK {}", hash)]);
                            //     for (address, account) in state.account_state_map.into_iter() {
                            //         let entry: Vec<String> = vec![
                            //             format!("ADDRESS: {}", address),
                            //             format!("ACCOUNT NONCE: {}", account.account_nonce),
                            //             format!("BALANCE: {}", account.balance),
                            //         ];
                            //         block_entry.push(entry);
                            //     }
                            //     output_vec.push(block_entry);
                            // }
                            let state_length = 0;

                            for block_hash in LC.into_iter() {
                                let mut entry: Vec<String> = Vec::new();
                                let state = states.get(&block_hash).unwrap();

                                entry.push(format!("STATE FOR BLOCK {}", block_hash));
                                for (address, account) in
                                    state.account_state_map.clone().into_iter()
                                {
                                    entry.push(format!("ADDRESS: {}", address));
                                    entry.push(format!("ACCOUNT NONCE: {}", account.account_nonce));
                                    entry.push(format!("BALANCE: {}", account.balance));
                                }
                                output_vec.push(entry);
                            }
                            // for (address, account) in
                            //     ith_block_state.account_state_map.clone().into_iter()
                            // {
                            //     let entry: Vec<String> = vec![
                            //         format!("ADDRESS: {}", address),
                            //         format!("ACCOUNT NONCE: {}", account.account_nonce),
                            //         format!("BALANCE: {}", account.balance),
                            //     ];
                            //     output_vec.push((entry));
                            // }
                            output_vec
                                .push(vec![format!("LENGTH OF LC IS: {}", length).to_string()]);

                            respond_json!(req, output_vec);
                        }
                        "/blockchain/longest-chain-tx-count" => {
                            let blockchain = blockchain.lock().unwrap();
                            let v = blockchain.all_blocks_in_longest_chain();
                            // let mut strs: Vec<Vec<String>> = Vec::new();
                            let mut tx_count = 0;
                            let mut run_tpb_average = 0;
                            // transaction hashes (hash set)
                            let mut seen: Vec<H256> = Vec::new();

                            for i in 0..v.len() {
                                if !blockchain.block_map.contains_key(&v[i]) {
                                    continue;
                                }
                                let b = blockchain.block_map.get(&v[i]).unwrap();
                                let txs: Vec<SignedTransaction> = b.content.content.clone();
                                let mut tx_strs: Vec<String> = Vec::new();

                                tx_strs.push(format!("TRANSACTS FOR BLOCK {}", i));
                                for j in 0..txs.len() {
                                    tx_strs.push(txs[j].clone().hash().to_string());
                                    tx_count += 1;

                                    if !seen.contains(&txs[j].clone().hash()) {
                                        seen.push(txs[j].hash());
                                    }
                                }
                                // strs.push(tx_strs);
                            }

                            drop(blockchain);
                            let C = tx_count as f32;
                            let B: f32 = (v.len() - 1) as f32;
                            let S: f32 = seen.len() as f32;
                            let unique_ratio = S / C;
                            let tpb_average: f32 = C / (B);

                            let response: Vec<String> = vec![
                                format!("The length of the longest chain is {}", tx_count),
                                format!("The unique TX ratio is {}", unique_ratio),
                                format!("The TX/Block is {}", tpb_average),
                            ];

                            respond_json!(req, response);
                        }

                        _ => {
                            let content_type =
                                "Content-Type: application/json".parse::<Header>().unwrap();
                            let payload = ApiResponse {
                                success: false,
                                message: "endpoint not found".to_string(),
                            };
                            let resp = Response::from_string(
                                serde_json::to_string_pretty(&payload).unwrap(),
                            )
                            .with_header(content_type)
                            .with_status_code(404);
                            req.respond(resp).unwrap();
                        }
                    }
                });
            }
        });
        info!("API server listening at {}", &addr);
    }
}
