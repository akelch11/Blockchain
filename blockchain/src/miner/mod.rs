pub mod worker;

use log::debug;
use log::info;

use crossbeam::channel::{unbounded, Receiver, Sender, TryRecvError};
use std::collections::HashMap;
use std::mem;
use std::mem::size_of;
// use core::num::flt2dec::Sign;
use std::time;

use std::thread;

use crate::blockchain::Blockchain;
use crate::types::account::Account;
use crate::types::address;
use crate::types::address::Address;
use crate::types::block;
use crate::types::block::{Block, Content, Header};
use crate::types::hash::Hashable;
use crate::types::hash::H256;
use crate::types::merkle::MerkleTree;
use crate::types::state::State;
use crate::types::transaction::verify;
use crate::types::transaction::SignedTransaction;
use ::rand::{rngs::StdRng, Rng, SeedableRng};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

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
const BLOCK_SIZE: usize = 5;

pub struct Context {
    /// Channel for receiving control signal
    control_chan: Receiver<ControlSignal>,
    operating_state: OperatingState,
    finished_block_chan: Sender<Block>,
    // chain for retrieving tip to use as parent of mined block
    blockchain: Arc<Mutex<Blockchain>>,
    mempool: Arc<Mutex<HashMap<H256, SignedTransaction>>>,
}

#[derive(Clone)]
pub struct Handle {
    /// Channel for sending signal to the miner thread
    control_chan: Sender<ControlSignal>,
}

pub fn new(
    blockchain: &Arc<Mutex<Blockchain>>,
    pool: &Arc<Mutex<HashMap<H256, SignedTransaction>>>,
) -> (Context, Handle, Receiver<Block>) {
    let (signal_chan_sender, signal_chan_receiver) = unbounded();
    let (finished_block_sender, finished_block_receiver) = unbounded();

    let ctx = Context {
        control_chan: signal_chan_receiver,
        operating_state: OperatingState::Paused,
        finished_block_chan: finished_block_sender,
        blockchain: Arc::clone(blockchain),
        mempool: Arc::clone(pool),
    };

    let handle = Handle {
        control_chan: signal_chan_sender,
    };

    (ctx, handle, finished_block_receiver)
}

#[cfg(any(test, test_utilities))]
fn test_new() -> (Context, Handle, Receiver<Block>) {
    use std::collections::HashMap;

    let mut blockchain: Blockchain = Blockchain::new();
    let mut mempool: HashMap<H256, SignedTransaction> = HashMap::new();
    let arc_mempool: Arc<Mutex<HashMap<H256, SignedTransaction>>> = Arc::new(Mutex::new(mempool));
    let arc_chain: Arc<Mutex<Blockchain>> = Arc::new(Mutex::new(blockchain));
    //println!("set up new empty blockchain\n");
    new(&arc_chain, &arc_mempool)
}

impl Handle {
    pub fn exit(&self) {
        self.control_chan.send(ControlSignal::Exit).unwrap();
    }

    pub fn start(&self, lambda: u64) {
        //println!("sending control signal start");
        self.control_chan
            .send(ControlSignal::Start(lambda))
            .unwrap();
    }

    pub fn update(&self) {
        self.control_chan.send(ControlSignal::Update).unwrap();
    }
}

impl Context {
    pub fn start(mut self) {
        thread::Builder::new()
            .name("miner".to_string())
            .spawn(move || {
                self.miner_loop();
            })
            .unwrap();
        info!("Miner initialized into paused mode");
    }

    fn spend_check(state_map: &State, tx: SignedTransaction) -> bool {
        let account_state_map = &state_map.account_state_map;

        let sender_account = account_state_map.get(&tx.transaction.sender).unwrap();
        let sender_balance = sender_account.balance;
        let prev_sender_nonce = sender_account.account_nonce;

        let spend_check = sender_balance >= tx.transaction.value
            && tx.transaction.account_nonce > prev_sender_nonce;

        return spend_check;
    }

    fn update_state(state: &mut State, tx: SignedTransaction) {
        let sender = tx.transaction.sender;
        let receiver = tx.transaction.receiver;
        let value = tx.transaction.value;

        let sender_account = state.account_state_map.get(&sender).unwrap();

        if sender == receiver {
            println!("SENDER SAME AS RECV IN TX");
            let new_sender_account = Account {
                balance: sender_account.balance,
                account_nonce: sender_account.account_nonce + 1,
            };

            state.account_state_map.insert(sender, new_sender_account);
            return;
        }

        let receiver_account = state.account_state_map.get(&receiver).unwrap();

        let new_sender_account = Account {
            balance: sender_account.balance - value,
            account_nonce: sender_account.account_nonce + 1,
        };
        let new_receiver_account = Account {
            balance: receiver_account.balance + value,
            account_nonce: receiver_account.account_nonce,
        };

        state.account_state_map.insert(sender, new_sender_account);
        state
            .account_state_map
            .insert(receiver, new_receiver_account);
    }

    fn miner_loop(&mut self) {
        // main mining loop
        //println!("ENTERING MINER LOOPy");
        let init_chain = self.blockchain.lock().unwrap();
        let mut parent: H256 = init_chain.tip();
        let mut diff: H256 = init_chain.block_map.get(&parent).unwrap().header.difficulty;
        drop(init_chain);

        loop {
            // check and react to control signals

            match self.operating_state {
                OperatingState::Paused => {
                    let signal = self.control_chan.recv().unwrap();
                    match signal {
                        ControlSignal::Exit => {
                            info!("Miner shutting down");
                            //println!("Miner shutting down ");
                            self.operating_state = OperatingState::ShutDown;
                        }
                        ControlSignal::Start(i) => {
                            info!("Miner starting in continuous mode with lambda {}", i);
                            //println!("Miner starting in continuous mode with lambda {}", i);
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
                                info!("Miner starting in continuous mode with lambda {}", i);
                                self.operating_state = OperatingState::Run(i);
                            }
                            ControlSignal::Update => {
                                let mut chain = self.blockchain.lock().unwrap();
                                parent = chain.tip();
                                diff = chain.block_map.get(&parent).unwrap().header.difficulty;
                                drop(chain);
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

            // TODO for student: actual mining, create a block
            // TODO for student: if block mining finished, you can have something like: self.finished_block_chan.send(block.clone()).expect("Send finished block error");
            // println!("****************** IN MINER ******************");
            // figure out how to unwrap blockchain from arc<mutex<>>

            let mut seeded_rng = StdRng::from_entropy();
            // //println!("try to gen nonce");
            let nonce: u32 = seeded_rng.gen_range(1..1000000);
            // //println!("random nonce tried: {}", nonce);
            let time: u128 = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_millis();

            // create empty merkle tree with dummy input, later will be transactions
            let dummy_input: Vec<SignedTransaction> = Vec::new();
            let merkle: H256 = MerkleTree::new(&dummy_input).root();

            let new_block_header: Header = Header {
                parent: parent,
                nonce: nonce,
                difficulty: diff,
                timestamp: time,
                merkle_root: merkle,
            };

            // add transactions as content

            let mut new_block_content: Content = Content {
                content: Vec::new(),
            };

            // add transactions from mempool map

            // println!("!!!!!!!! MEMPOOL HAS {} LENGTH", pool.len());

            let mut blockchain = self.blockchain.lock().unwrap();
            let mut pool = self.mempool.lock().unwrap();
            let tip_block_state = blockchain.state_map.get(&blockchain.tip()).unwrap();
            let mut temp_block_state = tip_block_state.clone();

            let mut tx_in_block = 0;

            for (key, tx) in pool.clone().into_iter() {
                // add transaction to block if there is enough size
                if tx_in_block < BLOCK_SIZE {
                    // println!("*********** INSERT Transaction w/ hash {} into block FROM MEMPOOL*********", tx.hash());

                    let address_check = tx.transaction.sender
                        == Address::from_public_key_bytes(tx.key_vector.as_ref());
                    let verify_check = verify(
                        &tx.transaction,
                        &tx.key_vector.as_ref(),
                        &tx.signature_vector.as_ref(),
                    );

                    let tip_spend_check = Self::spend_check(tip_block_state, tx.clone());
                    let temp_spend_check = Self::spend_check(&temp_block_state, tx.clone());

                    if !address_check || !tip_spend_check || !temp_spend_check || !verify_check {
                        // println!("TRANSACTION NOT ADDED TO BLOCK FOR NOT PASSING CHECKS");
                        continue;
                    } else {
                        // println!(">--- TRANSACTION WAS VALID, PUT INTO BLOCK ---<");
                        // println!(
                        //     "<---- TRANSACTION FROM {} TO {} FOR ${}",
                        //     tx.transaction.sender, tx.transaction.receiver, tx.transaction.value
                        // );
                        Self::update_state(&mut temp_block_state, tx.clone());
                        // println!("================== IN TEMP STATE =====================");
                        // for (addr, acc) in temp_block_state.account_state_map.clone().into_iter() {
                        //     println!("ADDRESS {} has balance: {}", addr, acc.balance);
                        // }
                        // println!("=======================================");
                    }

                    new_block_content.content.push(tx.clone());
                    tx_in_block += 1;
                } else {
                    // println!("did not add tx into block, BLOCK FULL OF TX");
                    break;
                }
            }
            drop(blockchain);

            //println!("DONE ADDING TX TO BLOCK")

            // if new_block_content.content.len() > 0 {
            //     println!("CREATIGN BLOCK W TRANSACTIONS");
            // } else {
            //     println!("THERE WERE NO TRANSACTIONS TO ADD TO BLOCK");
            // }

            let new_block: Block = Block {
                header: new_block_header,
                content: new_block_content.clone(),
            };

            let mining_success: bool = (new_block.hash() <= diff);

            // inside if remove transactions
            if mining_success {
                // TODO for student: if block mining finished, you can have something like: self.finished_block_chan.send(block.clone()).expect("Send finished block error");
                println!(
                    "%%%MINING SUCCESS, block has {} TXs %%%%%%%%%%",
                    new_block.content.content.len()
                );

                let block_transactions = new_block.content.content.clone();

                // remove transactions from mempool before  inserting block
                for i in 0..block_transactions.len() {
                    let tx: SignedTransaction = block_transactions[i].clone();

                    let verify_result: bool = verify(
                        &tx.transaction.clone(),
                        tx.key_vector.clone().as_ref(),
                        tx.signature_vector.clone().as_ref(),
                    );

                    // TODO: ADD VALIDATION/ VERIFICATION OF TRANSACTION
                    if !verify_result {
                        println!("VERIFY NOT PASSED FOUND INVALID BLOCK");

                        break;
                    } else {
                        // transcation is valid
                        // remove transaction from memory pool
                        let tx_hash = tx.hash();
                        println!("BLOCK IS VALID FOR NOW, TRANSACTION IS REMOVED");
                        pool.remove(&tx_hash);
                    }
                }

                // //println!(
                //     "sending block to channel with parent hash: {}",
                //     new_block.get_parent()
                // );

                self.finished_block_chan
                    .send(new_block.clone())
                    .expect("Send finished block error");

                parent = new_block.hash();

                // //println!("newest parent is {}", parent);
            } else {
                println!("MINING FAILED");
            }

            drop(pool);

            if let OperatingState::Run(i) = self.operating_state {
                if i != 0 {
                    let interval = time::Duration::from_micros(i as u64);
                    thread::sleep(interval);
                } else {
                    let interval = time::Duration::from_micros(10);
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

    #[test]
    #[timeout(60000)]
    fn miner_three_block() {
        let (miner_ctx, miner_handle, finished_block_chan) = super::test_new();
        miner_ctx.start();
        miner_handle.start(0);
        let mut block_prev = finished_block_chan.recv().unwrap();
        for _ in 0..2 {
            let block_next = finished_block_chan.recv().unwrap();
            assert_eq!(block_prev.hash(), block_next.get_parent());
            block_prev = block_next;
        }
    }

    #[test]
    #[timeout(60000)]
    fn miner_ten_block() {
        let (miner_ctx, miner_handle, finished_block_chan) = super::test_new();
        miner_ctx.start();
        miner_handle.start(0);
        let mut block_prev = finished_block_chan.recv().unwrap();
        for _ in 0..10 {
            let block_next = finished_block_chan.recv().unwrap();
            assert_eq!(block_prev.hash(), block_next.get_parent());
            block_prev = block_next;
        }
    }
}

// DO NOT CHANGE THIS COMMENT, IT IS FOR AUTOGRADER. AFTER TEST
