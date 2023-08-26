use super::message::Message;
use super::peer;
use super::peer::Handle;
use super::server::Handle as ServerHandle;
use crate::blockchain::Blockchain;
use crate::types::account::Account;
use crate::types::address::Address;
use crate::types::block::Block;
use crate::types::hash::Hashable;
use crate::types::hash::H256;
use crate::types::state::State;
use crate::types::transaction;
use crate::types::transaction::signed_tx_verify;
use crate::types::transaction::verify;
use crate::types::transaction::SignedTransaction;
// use core::num::flt2dec::Sign;
// use mempool::Pool;
use std::collections::HashMap;
use std::sync::MutexGuard;
use std::sync::{Arc, Mutex};

use log::{debug, error, warn};

use std::thread;

#[cfg(any(test, test_utilities))]
use super::peer::TestReceiver as PeerTestReceiver;
#[cfg(any(test, test_utilities))]
use super::server::TestReceiver as ServerTestReceiver;
#[derive(Clone)]
pub struct Worker {
    msg_chan: smol::channel::Receiver<(Vec<u8>, peer::Handle)>,
    num_worker: usize,
    server: ServerHandle,
    blockchain: Arc<Mutex<Blockchain>>,
    // mapping parent hashes to what block has that parent hash
    orphan_buffer: HashMap<H256, Block>,
    // memory pool of transactions mapped to by their hashes
    mempool: Arc<Mutex<HashMap<H256, SignedTransaction>>>,
}

impl Worker {
    pub fn new(
        num_worker: usize,
        msg_src: smol::channel::Receiver<(Vec<u8>, peer::Handle)>,
        server: &ServerHandle,
        blockchain: &Arc<Mutex<Blockchain>>,
        pool: &Arc<Mutex<HashMap<H256, SignedTransaction>>>,
    ) -> Self {
        Self {
            msg_chan: msg_src,
            num_worker,
            server: server.clone(),
            blockchain: Arc::clone(blockchain),
            orphan_buffer: HashMap::new(),
            mempool: Arc::clone(pool),
        }
    }

    pub fn start(&mut self) {
        let num_worker = self.num_worker;
        for i in 0..num_worker {
            let mut cloned = self.clone();
            thread::spawn(move || {
                cloned.worker_loop();
                warn!("Worker thread {} exited", i);
            });
        }
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

    fn spend_check(state_map: &State, tx: SignedTransaction) -> bool {
        let account_state_map = &state_map.account_state_map;

        let sender_account = account_state_map.get(&tx.transaction.sender).unwrap();
        let sender_balance = sender_account.balance;
        let prev_sender_nonce = sender_account.account_nonce;

        let spend_check = sender_balance >= tx.transaction.value
            && tx.transaction.account_nonce > prev_sender_nonce;

        return spend_check;
    }

    fn worker_loop(&mut self) {
        loop {
            let result = smol::block_on(self.msg_chan.recv());
            if let Err(e) = result {
                error!("network worker terminated {}", e);
                break;
            }
            let msg = result.unwrap();
            let (msg, mut peer) = msg;
            let msg: Message = bincode::deserialize(&msg).unwrap();
            match msg {
                Message::Ping(nonce) => {
                    debug!("Ping: {}", nonce);
                    peer.write(Message::Pong(nonce.to_string()));
                }
                Message::Pong(nonce) => {
                    debug!("Pong: {}", nonce);
                }
                Message::NewBlockHashes(hashes) => {
                    println!("NEW BLOCK HASH MSG RECEIVED");

                    let chain = self.blockchain.lock().unwrap();
                    // accumulate new hashes not yet in blockchain
                    let mut not_found_hashes: Vec<H256> = Vec::new();
                    for i in 0..hashes.len() {
                        let hash: H256 = hashes[i];
                        // println!("hash {}: {}", i, hash);
                        if !chain.block_map.contains_key(&hash) {
                            not_found_hashes.push(hash);
                        }
                    }
                    // send getBlocks with these hashes
                    // write or broadcast??
                    drop(chain);

                    if not_found_hashes.len() > 0 {
                        println!("Sending query to Get Blocks from peer");
                        peer.write(Message::GetBlocks(not_found_hashes));
                    }
                }
                Message::GetBlocks(hashes) => {
                    println!("GET BLOCKS Recieved");

                    let mut blocks: Vec<Block> = Vec::new();
                    let chain = self.blockchain.lock().unwrap();
                    for i in 0..hashes.len() {
                        let hash: H256 = hashes[i];
                        // add corresponding block to list of blocks to send
                        if chain.block_map.contains_key(&hash) {
                            let b: Block = chain.block_map.get(&hash).unwrap().clone();
                            // println!("providing block {} w/ hash back to peer {}", i, hash);
                            blocks.push(b);
                        }
                    }

                    drop(chain);

                    if blocks.len() > 0 {
                        println!("Sending Message with Blocks to Peer");

                        peer.write(Message::Blocks(blocks));
                    } else {
                        println!("call to getblocks w no blocks found");
                    }
                }
                Message::Blocks(blocks) => {
                    // hashes not already in chain that need to be broadcasted to network
                    println!("accepting blocks from a peer");
                    let mut new_hashes: Vec<H256> = Vec::new();
                    let mut chain = self.blockchain.lock().unwrap();
                    let mut pool = self.mempool.lock().unwrap();
                    for i in 0..blocks.len() {
                        let block: Block = blocks[i].clone();
                        let block_hash = block.hash();

                        // println!("recieved block {} w/ hash {}", i, block_hash);
                        // safety check
                        if block_hash > block.get_difficulty()
                            || chain.block_map.contains_key(&block_hash)
                        {
                            continue;
                        }

                        // // iterate through buffer,
                        let mut parent: H256 = block.get_parent();

                        // if chain has the parent --> not orphan
                        if chain.block_map.contains_key(&parent) {
                            // insert block into chain
                            let parent_block: Block = chain.block_map.get(&parent).unwrap().clone();
                            let parent_diff = parent_block.get_difficulty();
                            // block state that is updated with each transaction to be inserted into state map if block is OK
                            let mut new_block_state = chain.state_map.get(&parent).unwrap().clone();
                            // verifying incoming TX with parent block state
                            let mut parent_block_state =
                                chain.state_map.get(&parent).unwrap().clone();

                            if block.get_difficulty() != parent_diff {
                                continue;
                            }

                            // remove transactions from mempool if valid, if not valid, throw away block
                            let block_transactions = block.content.content.clone();
                            let mut discard_block: bool = false;
                            for i in 0..block_transactions.len() {
                                let tx: SignedTransaction = block_transactions[i].clone();

                                let verify_result: bool = verify(
                                    &tx.transaction.clone(),
                                    tx.key_vector.clone().as_ref(),
                                    tx.signature_vector.clone().as_ref(),
                                );

                                let spend_check =
                                    Self::spend_check(&parent_block_state, tx.clone());

                                // let temp_spend_check =
                                //     Self::spend_check(&new_block_state, tx.clone());

                                if !verify_result && !spend_check {
                                    println!("VERIFY NOT PASSED FOUND INVALID BLOCK");
                                    discard_block = true;
                                    break;
                                } else {
                                    // transcation is valid
                                    // remove transaction from memory pool
                                    let tx_hash = tx.hash();
                                    println!("<------- BLOCK IS VALID FOR NOW, TRANSACTION IS REMOVED ------->");
                                    // println!(
                                    //     "<---- TRANSACTION FROM {} TO {} FOR ${}",
                                    //     tx.transaction.sender,
                                    //     tx.transaction.receiver,
                                    //     tx.transaction.value
                                    // );
                                    // PERFORM STATE UPDATE
                                    Self::update_state(&mut new_block_state, tx.clone());

                                    pool.remove(&tx_hash);
                                }
                            }

                            for (addr, account) in
                                new_block_state.account_state_map.clone().into_iter()
                            {
                                if account.balance < 0 {
                                    discard_block = true;
                                    break;
                                }
                            }

                            // block was invalidated for containing invalid transaction, process next block
                            if (discard_block) {
                                continue;
                            }

                            // insert if PoW passes (finds nonce that makes hash(content, nonce) < diff)
                            // check block to be inserted has same diff as parent

                            chain.insert(&block);
                            new_hashes.push(block_hash);
                            // update ledger state for new inserted block
                            chain.state_map.insert(block_hash, new_block_state);
                            println!("================== AT TIP BLOCK =====================");
                            for (addr, acc) in chain
                                .state_map
                                .get(&parent)
                                .unwrap()
                                .account_state_map
                                .clone()
                                .into_iter()
                            {
                                println!("ADDRESS {} has balance: {}", addr, acc.balance);
                            }
                            println!("=======================================");

                            while self.orphan_buffer.contains_key(&parent) {
                                println!("DOING ORPHAN STUFF");
                                // block whose parent we found
                                let parent_block_state =
                                    chain.state_map.get(&parent).unwrap().clone();
                                let mut new_block_state = parent_block_state.clone();

                                let block_to_deorphan: Block =
                                    self.orphan_buffer.get(&parent).unwrap().clone();

                                let block_transactions = block_to_deorphan.content.content.clone();
                                let mut discard_block: bool = false;
                                for i in 0..block_transactions.len() {
                                    let tx: SignedTransaction = block_transactions[i].clone();

                                    let verify_result: bool = verify(
                                        &tx.transaction.clone(),
                                        tx.key_vector.clone().as_ref(),
                                        tx.signature_vector.clone().as_ref(),
                                    );

                                    let spend_check =
                                        Self::spend_check(&parent_block_state, tx.clone());
                                    let temp_spend_check =
                                        Self::spend_check(&new_block_state, tx.clone());

                                    if !verify_result && !spend_check {
                                        println!("VERIFY NOT PASSED FOUND INVALID BLOCK");
                                        discard_block = true;
                                        break;
                                    } else {
                                        // transcation is valid
                                        // remove transaction from memory pool
                                        let tx_hash = tx.hash();
                                        println!("<------- BLOCK IS VALID FOR NOW, TRANSACTION IS REMOVED ------->");
                                        // println!(
                                        //     "<---- TRANSACTION FROM {} TO {} FOR ${}",
                                        //     tx.transaction.sender,
                                        //     tx.transaction.receiver,
                                        //     tx.transaction.value
                                        // );
                                        // PERFORM STATE UPDATE
                                        Self::update_state(&mut new_block_state, tx.clone());

                                        pool.remove(&tx_hash);
                                    }
                                }

                                for (addr, account) in
                                    new_block_state.account_state_map.clone().into_iter()
                                {
                                    if account.balance < 0 {
                                        discard_block = true;
                                        break;
                                    }
                                }

                                if !discard_block {
                                    chain.insert(&block_to_deorphan);
                                    chain
                                        .state_map
                                        .insert(block_to_deorphan.hash(), new_block_state);

                                    new_hashes.push(block_to_deorphan.hash());
                                }

                                new_hashes.push(block_to_deorphan.hash());
                                self.orphan_buffer.remove(&block_to_deorphan.hash());

                                parent = block_to_deorphan.hash();
                            }

                            // remove this block from orphan buffer ^^
                        } else {
                            self.orphan_buffer.insert(parent, block.clone());
                            peer.write(Message::GetBlocks(vec![parent.clone()]));
                        }
                    }
                    drop(chain);
                    drop(pool);
                    if new_hashes.len() > 0 {
                        println!("Broadcasting new hashes to network");
                        // broadcast new block hashes to server
                        self.server.broadcast(Message::NewBlockHashes(new_hashes));
                    } else {
                        println!("blocks called w no new hashes");
                    }
                }
                Message::NewTransactionHashes(new_tx_hashes) => {
                    println!("NEW TRANSACTION HASHES MESSAGE RECEIVED - tx hashes incoming");
                    // println!("new hashes of transactions are incoming");

                    let chain = self.blockchain.lock().unwrap();
                    let pool = self.mempool.lock().unwrap();
                    // accumulate new transaction hashes not yet in mempool
                    let mut tx_hashes: Vec<H256> = Vec::new();
                    for i in 0..new_tx_hashes.len() {
                        let hash: H256 = new_tx_hashes[i];
                        // println!("hash {}: {}", i, hash);
                        if !pool.contains_key(&hash) && !tx_hashes.contains(&hash) {
                            // println!("ADDING HASH OF TRANACTION: {}", hash);
                            tx_hashes.push(hash);
                        }
                    }
                    // send getBlocks with these hashes
                    // write or broadcast??
                    drop(chain);
                    drop(pool);

                    if tx_hashes.len() > 0 {
                        println!("Sending query to Get Transactions from peer");
                        peer.write(Message::GetTransactions(tx_hashes));
                    }
                }
                // reciever gets vector of hashes of transaction tx_hashes, which they use
                // to get actual transaction from map and send back over
                Message::GetTransactions(tx_hashes) => {
                    println!("GET TRANSACTIONS MESSAGE RECEIVED BY PEER");

                    let pool = self.mempool.lock().unwrap();
                    let mut transactions: Vec<SignedTransaction> = Vec::new();
                    for i in 0..tx_hashes.len() {
                        let hash: H256 = tx_hashes[i];

                        if pool.contains_key(&hash) {
                            let tx: SignedTransaction = pool.get(&hash).unwrap().clone();
                            // println!(
                            //     "providing transaction {} w/ hash back to peer. TX_HASH: {}",
                            //     i, hash
                            // );
                            transactions.push(tx.clone());
                        }
                    }
                    if transactions.len() > 0 {
                        println!("Sending Message with TX to Peer");

                        peer.write(Message::Transactions(transactions));
                    } else {
                        println!("call to getTX w no TX found");
                    }
                    drop(pool);
                }
                Message::Transactions(transactions) => {
                    println!("MESSAGE TO PROVIDE TRANSACTIONS RECIEVED");

                    let mut pool = self.mempool.lock().unwrap();
                    // vector of tranasction hashes to be broadcasted because thery are not in chain
                    let mut new_tx_hashes: Vec<H256> = Vec::new();

                    for i in 0..transactions.len() {
                        let transaction: SignedTransaction = transactions[i].clone();
                        let tx_hash: H256 = transaction.hash();

                        // if !pool.contains_key(&tx_hash) {
                        // add transaction validation check

                        // if !signed_tx_verify(
                        //     &transaction,
                        //     transaction.key_vector.as_ref(),
                        //     transaction.signature_vector.as_ref(),
                        // ) {
                        //     continue;
                        // }

                        let address_check: bool = transaction.transaction.sender
                            == Address::from_public_key_bytes(transaction.key_vector.as_ref());

                        if !address_check {
                            println!(
                                ">>>>> TRANSACTION DIDNT PASS ADDRESS CHECK IN NET WORKER <<<<<<<"
                            );
                            continue;
                        }

                        pool.insert(tx_hash, transaction.clone());
                        new_tx_hashes.push(tx_hash);
                        // }
                    }

                    if new_tx_hashes.len() > 0 {
                        println!("SENDING NEW TX HASHES MESSAGE");
                        self.server
                            .broadcast(Message::NewTransactionHashes(new_tx_hashes));
                    }

                    drop(pool);
                }
                // catch all for now
                _ => {} // remember to implement "optional" part of A5
                        // _ => unimplemented!(),
                        // implement other Message types,  NewTransactionHashes(Vec<H256>), GetTransactions(Vec<H256>), Transactions(Vec<SignedTransaction>
            }
        }
    }
}

#[cfg(any(test, test_utilities))]
struct TestMsgSender {
    s: smol::channel::Sender<(Vec<u8>, peer::Handle)>,
}
#[cfg(any(test, test_utilities))]
impl TestMsgSender {
    fn new() -> (
        TestMsgSender,
        smol::channel::Receiver<(Vec<u8>, peer::Handle)>,
    ) {
        let (s, r) = smol::channel::unbounded();
        (TestMsgSender { s }, r)
    }

    fn send(&self, msg: Message) -> PeerTestReceiver {
        let bytes = bincode::serialize(&msg).unwrap();
        let (handle, r) = peer::Handle::test_handle();
        smol::block_on(self.s.send((bytes, handle))).unwrap();
        r
    }
}
#[cfg(any(test, test_utilities))]
/// returns two structs used by tests, and an ordered vector of hashes of all blocks in the blockchain
fn generate_test_worker_and_start() -> (TestMsgSender, ServerTestReceiver, Vec<H256>) {
    use crate::types::hash;

    let (server, server_receiver) = ServerHandle::new_for_test();
    let (test_msg_sender, msg_chan) = TestMsgSender::new();

    let chain: Arc<Mutex<Blockchain>> = Arc::new(Mutex::new(Blockchain::new()));
    let pool: Arc<Mutex<HashMap<H256, SignedTransaction>>> = Arc::new(Mutex::new(HashMap::new()));
    let mut worker = Worker::new(1, msg_chan, &server, &chain, &pool);

    worker.start();

    let mut hash_list: Vec<H256> = Vec::new();
    let chain = chain.lock().unwrap();
    // let cur_hash = chain.tip();
    // let dummy_parent_hash: H256 =
    // (hex!("0000000000000000000000000000000000000000000000000000000000000000")).into();

    hash_list = chain.all_blocks_in_longest_chain();
    drop(chain);

    (test_msg_sender, server_receiver, hash_list)
}

// DO NOT CHANGE THIS COMMENT, IT IS FOR AUTOGRADER. BEFORE TEST

#[cfg(test)]
mod test {
    use crate::types::block::generate_random_block;
    use crate::types::hash::Hashable;
    use ntest::timeout;

    use super::super::message::Message;
    use super::generate_test_worker_and_start;

    #[test]
    #[timeout(60000)]
    fn reply_new_block_hashes() {
        let (test_msg_sender, _server_receiver, v) = generate_test_worker_and_start();
        let random_block = generate_random_block(v.last().unwrap());
        let mut peer_receiver =
            test_msg_sender.send(Message::NewBlockHashes(vec![random_block.hash()]));
        let reply = peer_receiver.recv();
        if let Message::GetBlocks(v) = reply {
            assert_eq!(v, vec![random_block.hash()]);
        } else {
            panic!();
        }
    }
    #[test]
    #[timeout(60000)]
    fn reply_get_blocks() {
        println!("starting reply_get_blocks");
        let (test_msg_sender, _server_receiver, v) = generate_test_worker_and_start();
        let h = v.last().unwrap().clone();
        let mut peer_receiver = test_msg_sender.send(Message::GetBlocks(vec![h.clone()]));
        let reply = peer_receiver.recv();
        if let Message::Blocks(v) = reply {
            println!("inside if");
            assert_eq!(1, v.len());
            assert_eq!(h, v[0].hash())
        } else {
            panic!();
        }
    }
    #[test]
    #[timeout(60000)]
    fn reply_blocks() {
        let (test_msg_sender, server_receiver, v) = generate_test_worker_and_start();
        let random_block = generate_random_block(v.last().unwrap());
        let mut _peer_receiver = test_msg_sender.send(Message::Blocks(vec![random_block.clone()]));
        let reply = server_receiver.recv().unwrap();
        if let Message::NewBlockHashes(v) = reply {
            assert_eq!(v, vec![random_block.hash()]);
        } else {
            panic!();
        }
    }
}

// DO NOT CHANGE THIS COMMENT, IT IS FOR AUTOGRADER. AFTER TEST
