use log::info;

use crate::network::server::Handle as ServerHandle;
use crate::types::address::Address;

use crate::types::transaction;
use crate::types::transaction::signed_tx_verify;
use crate::types::transaction::verify;
use crate::types::transaction::Transaction;
use crossbeam::channel::{unbounded, Receiver, Sender, TryRecvError};
use rand::distributions::uniform::UniformSampler;
use ring::aead::BoundKey;
use ring::signature::Ed25519KeyPair;
use ring::signature::KeyPair;
use ring::signature::Signature;
use std::collections::HashMap;
use std::mem;
use std::mem::size_of;
use std::net;
use std::net::SocketAddr;
// use core::num::flt2dec::Sign;
use std::time;

use std::thread;

use crate::api::Server;
use crate::blockchain::Blockchain;
use crate::network::message::Message;

// use crate::network::server::Handle;
use crate::types::block;
use crate::types::block::{Block, Content, Header};
use crate::types::hash::Hashable;
use crate::types::hash::H256;
use crate::types::key_pair;
use crate::types::merkle::MerkleTree;
use crate::types::transaction::generate_random_transaction;
use crate::types::transaction::sign;
use crate::types::transaction::SignedTransaction;
use ::rand::{rngs::StdRng, Rng, SeedableRng};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use tiny_http::Server as HTTPServer;

enum ControlSignal {
    Start(u64), // the number controls the lambda of interval between block generation
    Update,     // update the block in mining, it may due to new blockchain tip or new transaction
    Exit,
}

enum OperatingState {
    Paused,
    Run(u64),
    ShutDown,
}

// size of block in bytes
const BLOCK_SIZE: usize = 1000;

pub struct Generator {
    /// Channel for receiving control signal
    control_chan: Receiver<ControlSignal>,
    operating_state: OperatingState,
    finished_block_chan: Sender<Block>,
    // chain for retrieving tip to use as parent of mined block
    blockchain: Arc<Mutex<Blockchain>>,
    mempool: Arc<Mutex<HashMap<H256, SignedTransaction>>>,
    server: ServerHandle,
    p2paddr: SocketAddr,
}

#[derive(Clone)]
pub struct Handle {
    /// Channel for sending signal to the miner thread
    control_chan: Sender<ControlSignal>,
}

// pub fn new(
//     // blockchain: &Arc<Mutex<Blockchain>>,
//     // pool: &Arc<Mutex<HashMap<H256, SignedTransaction>>>,
//     server: &Server,
// ) -> (Generator, Handle, Receiver<Block>) {
//     let (signal_chan_sender, signal_chan_receiver) = unbounded();
//     let (finished_block_sender, finished_block_receiver) = unbounded();

//     let ctx = Generator {
//         control_chan: signal_chan_receiver,
//         operating_state: OperatingState::Paused,
//         finished_block_chan: finished_block_sender,
//         blockchain: Arc::clone(&server.blockchain),
//         mempool: Arc::clone(&server.mempool),
//         server: *server,
//     };

//     let handle = Handle {
//         control_chan: signal_chan_sender.clone(),
//     };

//     (ctx, handle, finished_block_receiver)
// }

pub fn new(
    blockchain: &Arc<Mutex<Blockchain>>,
    pool: &Arc<Mutex<HashMap<H256, SignedTransaction>>>,
    server: &ServerHandle,
    p2paddr: SocketAddr,
) -> (Generator, Handle, Receiver<Block>) {
    let (signal_chan_sender, signal_chan_receiver) = unbounded();
    let (finished_block_sender, finished_block_receiver) = unbounded();

    let handle = Handle {
        control_chan: signal_chan_sender,
    };

    let ctx = Generator {
        control_chan: signal_chan_receiver,
        operating_state: OperatingState::Paused,
        finished_block_chan: finished_block_sender,
        blockchain: Arc::clone(blockchain),
        mempool: Arc::clone(pool),
        server: server.clone(),
        p2paddr: p2paddr,
    };

    (ctx, handle, finished_block_receiver)
}

// pub fn new(
//     blockchain: &Arc<Mutex<Blockchain>>,
//     pool: &Arc<Mutex<HashMap<H256, SignedTransaction>>>,
// ) -> (Context, Handle, Receiver<Block>) {
//     let (signal_chan_sender, signal_chan_receiver) = unbounded();
//     let (finished_block_sender, finished_block_receiver) = unbounded();

//     let ctx = Context {
//         control_chan: signal_chan_receiver,
//         operating_state: OperatingState::Paused,
//         finished_block_chan: finished_block_sender,
//         blockchain: Arc::clone(blockchain),
//         mempool: Arc::clone(pool),
//     };

//     let handle = Handle {
//         control_chan: signal_chan_sender,
//     };

//     (ctx, handle, finished_block_receiver)
// }

// #[cfg(any(test, test_utilities))]
// fn test_new() -> (Generator, Handle, Receiver<Block>) {
//     use std::collections::HashMap;

//     let mut blockchain: Blockchain = Blockchain::new();
//     let mut mempool: HashMap<H256, SignedTransaction> = HashMap::new();
//     let arc_mempool: Arc<Mutex<HashMap<H256, SignedTransaction>>> = Arc::new(Mutex::new(mempool));
//     let arc_chain: Arc<Mutex<Blockchain>> = Arc::new(Mutex::new(blockchain));
//     println!("set up new empty blockchain\n");
//     new(&arc_chain, &arc_mempool)
// }

impl Handle {
    pub fn exit(&self) {
        self.control_chan.send(ControlSignal::Exit).unwrap();
    }

    pub fn start(&self, theta: u64) {
        self.control_chan.send(ControlSignal::Start(theta)).unwrap();
    }

    pub fn update(&self) {
        self.control_chan.send(ControlSignal::Update).unwrap();
    }
}

impl Generator {
    pub fn start(mut self) {
        thread::Builder::new()
            .name("miner".to_string())
            .spawn(move || {
                println!("starting generator loop");
                self.generator_loop();
            })
            .unwrap();
        info!("Miner initialized into paused mode");
    }

    fn generator_loop(&mut self) {
        // main mining loop

        // let mut parent: H256 = init_chain.tip();
        // let mut diff: H256 = init_chain.block_map.get(&parent).unwrap().header.difficulty;
        // drop(init_chain);

        let mut key_pair_seed_list: Vec<&[u8]> = Vec::new();

        let seed_slice_0: &[u8] = &[0; 1];
        let seed_slice_1: &[u8] = &[1; 1];
        let seed_slice_2: &[u8] = &[2; 1];
        let seed_slice_0: &[u8] = &[0; 32];
        let seed_slice_1: &[u8] = &[1; 32];
        let seed_slice_2: &[u8] = &[2; 32];

        // put set of initial key pairs to choose from
        key_pair_seed_list.push(seed_slice_0);
        key_pair_seed_list.push(seed_slice_1);
        key_pair_seed_list.push(seed_slice_2);

        loop {
            // check and react to control signals
            match self.operating_state {
                OperatingState::Paused => {
                    let signal = self.control_chan.recv().unwrap();
                    match signal {
                        ControlSignal::Exit => {
                            info!("Miner shutting down");
                            self.operating_state = OperatingState::ShutDown;
                        }
                        ControlSignal::Start(i) => {
                            info!("Miner starting in continuous mode with lambda {}", i);
                            self.operating_state = OperatingState::Run(i);
                        }
                        ControlSignal::Update => {
                            // in paused state, don't need to update
                        }
                    };
                    continue;
                }
                OperatingState::ShutDown => {
                    return;
                }
                _ => match self.control_chan.try_recv() {
                    Ok(signal) => {
                        match signal {
                            ControlSignal::Exit => {
                                info!("Miner shutting down");
                                self.operating_state = OperatingState::ShutDown;
                            }
                            ControlSignal::Start(i) => {
                                info!("Generator starting in continuous mode with lambda {}", i);
                                self.operating_state = OperatingState::Run(i);
                            }
                            ControlSignal::Update => {
                                // let mut chain = self.blockchain.lock().unwrap();
                                // parent = chain.tip();
                                // diff = chain.block_map.get(&parent).unwrap().header.difficulty;
                                // drop(chain);
                            }
                        };
                    }
                    Err(TryRecvError::Empty) => {}
                    Err(TryRecvError::Disconnected) => panic!("Miner control channel detached"),
                },
            }
            if let OperatingState::ShutDown = self.operating_state {
                return;
            }

            //
            println!("****************** IN TX GENERATOR ******************");

            let mut blockchain = self.blockchain.lock().unwrap();
            let mut seeded_rng = StdRng::from_entropy();

            // create transaction w random content
            // let tx = generate_random_transaction();

            // println!("GENERATED RANDOM TRANSACTION");

            // HAVE SENDER BE CONFIGURED BASED ON IP ADDRESS
            let mut key_seed = 69;

            if self.p2paddr == "127.0.0.1:6000".parse::<net::SocketAddr>().unwrap() {
                key_seed = 0;
            } else if self.p2paddr == "127.0.0.1:6001".parse::<net::SocketAddr>().unwrap() {
                key_seed = 1;
            } else {
                key_seed = 2;
            }

            let key: Ed25519KeyPair =
                Ed25519KeyPair::from_seed_unchecked(key_pair_seed_list[key_seed]).unwrap();
            // have sender be hard coded
            // sender is chosen based on address of their label  --> chipotle porder
            let sender_address: Address = Address::from_public_key_bytes(key.public_key().as_ref());

            let mut receiver_address = sender_address.clone();

            let tip_hash = blockchain.tip();

            let state_addresses_map = blockchain
                .state_map
                .get(&tip_hash)
                .unwrap()
                .account_state_map
                .clone();
            drop(blockchain);

            let mut eligible_addr_in_state: Vec<Address> = Vec::new();

            for addr in state_addresses_map.keys() {
                eligible_addr_in_state.push(addr.clone());
            }

            receiver_address =
                eligible_addr_in_state[seeded_rng.gen_range(0..eligible_addr_in_state.len())];

            // receiver_address = sender_address.clone();

            let prev_sender_nonce = state_addresses_map
                .get(&sender_address)
                .unwrap()
                .account_nonce;

            let prev_sender_balance = state_addresses_map.get(&sender_address).unwrap().balance;

            let tx: Transaction = Transaction {
                sender: sender_address,
                receiver: receiver_address,
                value: seeded_rng.gen_range(
                    0..(if prev_sender_balance > 0 {
                        prev_sender_balance
                    } else {
                        1
                    }),
                ),
                account_nonce: prev_sender_nonce + 1,
            };

            let signature: Signature = sign(&tx, &key);

            let signed_tx: SignedTransaction = SignedTransaction {
                transaction: tx.clone(),
                signature_vector: signature.as_ref().to_vec(),
                key_vector: key.public_key().as_ref().to_vec(),
            };

            let verify_result =
                transaction::verify(&tx, key.public_key().as_ref(), signature.clone().as_ref());

            let address_check = signed_tx.transaction.sender
                == Address::from_public_key_bytes(signed_tx.key_vector.as_ref());

            let nonce_check = tx.account_nonce > prev_sender_nonce;

            // println!("RANDO TRX HAS HASH {}", signed_tx.hash());

            // possible validation steps

            // add into mempool

            let mut pool = self.mempool.lock().unwrap();

            let s_tx_hash = signed_tx.hash();

            if verify_result && address_check && nonce_check {
                println!("INSERTING TRANSACTION INTO MEMPOOL W HASH: {}", s_tx_hash);
                println!(">--- TRANSACTION INTO MEMPOOL ---<");
                println!(
                    "<---- TRANSACTION FROM {} TO {} FOR ${}",
                    signed_tx.transaction.sender,
                    signed_tx.transaction.receiver,
                    signed_tx.transaction.value
                );
                pool.insert(signed_tx.hash(), signed_tx.clone());

                //     // broacast new hashes
                println!("SENDING NEW TRANSACTION HASHES MESSAGE");
                self.server
                    .broadcast(Message::NewTransactionHashes(vec![signed_tx.hash()]));
            } else {
                println!("NOT VERIFYING");
            }

            drop(pool);

            if let OperatingState::Run(i) = self.operating_state {
                if i != 0 {
                    let interval = time::Duration::from_micros(i * 4000 as u64);
                    thread::sleep(interval);
                }
            }
        }
    }
}

// DO NOT CHANGE THIS COMMENT, IT IS FOR AUTOGRADER. BEFORE TEST

#[cfg(test)]
mod test {
    use crate::types::hash::Hashable;
    use ntest::timeout;

    // #[test]
    // #[timeout(60000)]
    // fn miner_three_block() {
    //     let (miner_ctx, miner_handle, finished_block_chan) = super::test_new();
    //     miner_ctx.start();
    //     miner_handle.start(0);
    //     let mut block_prev = finished_block_chan.recv().unwrap();
    //     for _ in 0..2 {
    //         let block_next = finished_block_chan.recv().unwrap();
    //         assert_eq!(block_prev.hash(), block_next.get_parent());
    //         block_prev = block_next;
    //     }
    // }

    // #[test]
    // #[timeout(60000)]
    // fn miner_ten_block() {
    //     let (miner_ctx, miner_handle, finished_block_chan) = super::test_new();
    //     miner_ctx.start();
    //     miner_handle.start(0);
    //     let mut block_prev = finished_block_chan.recv().unwrap();
    //     for _ in 0..10 {
    //         let block_next = finished_block_chan.recv().unwrap();
    //         assert_eq!(block_prev.hash(), block_next.get_parent());
    //         block_prev = block_next;
    //     }
    // }
}

// DO NOT CHANGE THIS COMMENT, IT IS FOR AUTOGRADER. AFTER TEST
