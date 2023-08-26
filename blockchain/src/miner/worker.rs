use crate::blockchain::Blockchain;
use crate::network::message;
use crate::network::server::Handle as ServerHandle;
use crate::types::account::Account;
use crate::types::block::{self, Block};
use crate::types::hash::{Hashable, H256};
use crate::types::state::State;
use crate::types::transaction::{verify, SignedTransaction};
use crossbeam::channel::{unbounded, Receiver, Sender, TryRecvError};
use log::{debug, info};
use std::sync::{Arc, Mutex};
use std::thread;

#[derive(Clone)]
pub struct Worker {
    server: ServerHandle,
    finished_block_chan: Receiver<Block>,
    // chain for retrieving tip to use as parent of mined block
    blockchain: Arc<Mutex<Blockchain>>,
}

impl Worker {
    pub fn new(
        server: &ServerHandle,
        finished_block_chan: Receiver<Block>,
        blockchain: &Arc<Mutex<Blockchain>>,
    ) -> Self {
        Self {
            server: server.clone(),
            finished_block_chan,
            blockchain: Arc::clone(blockchain),
        }
    }

    pub fn start(self) {
        thread::Builder::new()
            .name("miner-worker".to_string())
            .spawn(move || {
                self.worker_loop();
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

        if new_sender_account.balance < 0 {
            return;
        }

        state.account_state_map.insert(sender, new_sender_account);
        state
            .account_state_map
            .insert(receiver, new_receiver_account);
    }

    fn worker_loop(&self) {
        loop {
            let _block = self
                .finished_block_chan
                .recv()
                .expect("Receive finished block error");

            println!("<<<<< MINER HAS MINED BLOCK {} >>>>>>>>>", _block.hash());
            // TODO for student: insert this finished block to blockchain, and broadcast this block hash

            let inner_txs: Vec<String> = _block
                .content
                .content
                .clone()
                .into_iter()
                .map(|h| h.hash().to_string())
                .collect();
            // for i in 0..inner_txs.len() {
            //     println!("TRANSACTION {} INSIDE BLOCK W/ HASH: {}", i, inner_txs[i]);
            // }
            // let v_string: Vec<String> =
            // v.into_iter().map(|h| h.to_string()).collect();

            let blockchain_clone = (Arc::clone(&self.blockchain).clone());
            let mut blockchain = (blockchain_clone).lock().unwrap();

            let tip_state_map = blockchain.state_map.get(&blockchain.tip()).unwrap().clone();
            let mut new_state_map = tip_state_map.clone();

            let mut discard_block = false;
            for tx in _block.content.content.clone() {
                let verify_result: bool = verify(
                    &tx.transaction.clone(),
                    tx.key_vector.clone().as_ref(),
                    tx.signature_vector.clone().as_ref(),
                );
                let spend_check = Self::spend_check(&tip_state_map, tx.clone());
                // let temp_spend_check = Self::spend_check(&new_state_map, tx.clone());

                if !verify_result || !spend_check {
                    println!("CHECKS NOT PASSED FOUND INVALID TRANSACTION");
                    discard_block = true;
                    break;
                } else {
                    // MAYBE REMOVE FROM MEMPOOL HERE?? Look into it
                    Self::update_state(&mut new_state_map, tx.clone())
                }
            }

            for (addr, account) in new_state_map.account_state_map.clone().into_iter() {
                if account.balance < 0 {
                    discard_block = true;
                    break;
                }
            }

            // println!("before insertion, tip is {}", blockchain.tip());

            // remove MAYBE
            if !discard_block {
                blockchain.insert(&_block);
                blockchain.state_map.insert(_block.hash(), new_state_map);

                // blockchain
                //     .state_map
                //     .insert(_block.clone().hash(), parent_state.clone());
                println!("=======================================");
                for (addr, acc) in blockchain
                    .state_map
                    .get(&blockchain.tip())
                    .unwrap()
                    .account_state_map
                    .clone()
                    .into_iter()
                {
                    println!("ADDRESS {} has balance: {}", addr, acc.balance);
                }
                println!("=======================================");
            }
            drop(blockchain);

            println!(
                "$$$$$$$$$ NEW BLOCK TO SEND WITH {} TRANSACTIONS INSIDE $$$$$$$$$",
                _block.content.content.len()
            );

            // broadcast out hash of new block
            let mut broadcast_hash: Vec<H256> = Vec::new();
            broadcast_hash.push(_block.hash());
            // println!(
            //     "BROADCAST NEW BLOCK HASH - inserting and broadcasting block {}",
            //     _block.hash()
            // );
            if !discard_block {
                self.server
                    .broadcast(message::Message::NewBlockHashes(broadcast_hash));
            }
        }
    }
}
