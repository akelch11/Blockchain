use std::collections::HashMap;

use crate::types::account::Account;
use crate::types::address::Address;
use crate::types::block::{Block, Content, Header};
use crate::types::hash::Hashable;
use crate::types::hash::H256;
use crate::types::merkle::MerkleTree;
use crate::types::state::State;
use ::rand::{rngs::StdRng, Rng, SeedableRng};
use clap::Error;
use hex_literal::hex;
use ring::signature::{Ed25519KeyPair, KeyPair};
use std::time::{SystemTime, UNIX_EPOCH};

pub struct Blockchain {
    pub block_map: HashMap<H256, Block>,
    pub height_map: HashMap<H256, u32>,
    pub tip: H256,
    pub state_map: HashMap<H256, State>,
}

impl Blockchain {
    /// Create a new blockchain, only containing the genesis block
    pub fn new() -> Self {
        // deprecatedblock
        let dummy_parent_hash: H256 =
            (hex!("0000000000000000000000000000000000000000000000000000000000000000")).into();

        let mut seeded_rng = StdRng::from_entropy();
        // let nonce: u32 = seeded_rng.gen_range(0..1000000);
        let nonce: u32 = 25;
        // let difficulty: H256 =
        //     hex!("0a0b0c0d0e0f0e0d0a0b0c0d0e0f0e0d0a0b0c0d0e0f0e0d0a0b0c0d0e0f0e0d").into();
        // let difficulty: H256 = [1u8; 32].into();
        // let difficulty: H256 =
        //     (hex!("0000020022000000000000001100000000000000000000000011111111111111")).into();
        let difficulty: H256 =
            (hex!("00000f0000000000000000000000000000000000000000000000000000000000")).into();

        let difficulty =
            (hex!("01ffff0000000000000000000000000000000000000000000000000000000000")).into();
        // let time: u128 = SystemTime::now()
        //     .duration_since(UNIX_EPOCH)
        //     .unwrap()
        //     .as_millis();
        let time: u128 = 123456789;

        let empty_data: Vec<H256> = Vec::new();
        let merkle: MerkleTree = MerkleTree::new(&empty_data);
        let root: H256 = merkle.root();

        let header: Header = Header {
            parent: dummy_parent_hash.clone(),
            nonce: nonce,
            difficulty: difficulty,
            timestamp: time,
            merkle_root: root,
        };

        let content: Content = Content {
            content: Vec::new().into(),
        };

        let genesis_block: Block = Block {
            header: header,
            content: content,
        };

        let mut block_map: HashMap<H256, Block> = HashMap::new();
        let mut height_map: HashMap<H256, u32> = HashMap::new();
        let mut state_map: HashMap<H256, State> = HashMap::new();

        let genesis_seed_slice_0: &[u8] = &[0; 32];
        let genesis_seed_slice_1: &[u8] = &[1; 32];
        let genesis_seed_slice_2: &[u8] = &[2; 32];

        let genesis_key_pair_0 = Ed25519KeyPair::from_seed_unchecked(genesis_seed_slice_0).unwrap();
        let genesis_key_pair_1 = Ed25519KeyPair::from_seed_unchecked(genesis_seed_slice_1).unwrap();
        let genesis_key_pair_2 = Ed25519KeyPair::from_seed_unchecked(genesis_seed_slice_2).unwrap();
        let genesis_address_0 =
            Address::from_public_key_bytes(genesis_key_pair_0.public_key().as_ref());
        let genesis_address_1 =
            Address::from_public_key_bytes(genesis_key_pair_1.public_key().as_ref());
        let genesis_address_2 =
            Address::from_public_key_bytes(genesis_key_pair_2.public_key().as_ref());
        // initial account nonce is 0, balance is 1 mil
        let genesis_account_0: Account = Account {
            account_nonce: 0,
            balance: 100,
        };
        let genesis_account_1: Account = Account {
            account_nonce: 0,
            balance: 0,
        };
        let genesis_account_2: Account = Account {
            account_nonce: 0,
            balance: 0,
        };

        // ledger state at the genesis block
        let mut genesis_state: State = State::new();
        // insert initial accounts for 3 nodes into initial state
        genesis_state
            .account_state_map
            .insert(genesis_address_0, genesis_account_0);
        genesis_state
            .account_state_map
            .insert(genesis_address_1, genesis_account_1);
        genesis_state
            .account_state_map
            .insert(genesis_address_2, genesis_account_2);

        let genesis_hash: H256 = genesis_block.hash();
        println!("genesis hash is {}", genesis_hash);

        block_map.insert(genesis_hash, genesis_block.clone());
        height_map.insert(genesis_hash, 0);
        state_map.insert(genesis_hash, genesis_state);

        Self {
            block_map: block_map,
            height_map: height_map,
            tip: genesis_hash,
            state_map: state_map,
        }
    }

    /// Insert a block into blockchain
    pub fn insert(&mut self, block: &Block) -> Result<bool, bool> {
        let parent: H256 = (*block).get_parent();

        if !self.block_map.contains_key(&parent) {
            println!("throwing err");
            return Err(false);
        }

        let parent_height = self.height_map.get(&parent).unwrap().clone();
        // println!("height  of parent is {}", parent_height);

        self.block_map.insert((*block).hash(), (*block).clone());
        self.height_map.insert((*block).hash(), parent_height + 1);

        if parent_height + 1 > self.height_map.get(&self.tip()).unwrap().clone() {
            // println!("UPDATE THE TIP to {}", (*block).hash().clone());
            self.tip = (*block).hash();
        }

        Ok(true)
    }

    /// Get the last block's hash of the longest chain
    pub fn tip(&self) -> H256 {
        // println!("call to tip, tip hash is {} ", self.tip.clone());
        return self.tip;
    }

    /// Get all blocks' hashes of the longest chain, ordered from genesis to the tip
    pub fn all_blocks_in_longest_chain(&self) -> Vec<H256> {
        println!("getting Longest chain");
        let mut chain: Vec<H256> = Vec::new();

        let dummy_parent_hash: H256 =
            (hex!("0000000000000000000000000000000000000000000000000000000000000000")).into();

        let mut curHash: H256 = self.tip();

        while curHash != dummy_parent_hash {
            // println!("IN LC METHOD: cur hash {}", curHash);
            chain.push(curHash.clone());
            curHash = (self.block_map.get(&curHash)).unwrap().get_parent();
        }

        // chain.push(dummy_parent_hash.clone());

        return chain.into_iter().rev().collect();
    }
}

// DO NOT CHANGE THIS COMMENT, IT IS FOR AUTOGRADER. BEFORE TEST

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::block::generate_random_block;
    use crate::types::hash::Hashable;

    #[test]
    fn insert_one() {
        let mut blockchain = Blockchain::new();
        let genesis_hash = blockchain.tip();
        let block = generate_random_block(&genesis_hash);
        blockchain.insert(&block);
        assert_eq!(blockchain.tip(), block.hash());
    }

    // #[test]
    // fn insert_zero() {
    //     let mut chain: Blockchain = Blockchain::new();
    //     let genesis_hash: H256 = chain.tip();
    //     println!("{}", genesis_hash);
    //     let dummy_hash: H256 =
    //         (hex!("0000000000000000000000000000000000000000000000000000000000000001")).into();
    //     assert_eq!(genesis_hash, dummy_hash);
    // }

    #[test]
    fn insert_two() {
        let mut blockchain = Blockchain::new();
        let genesis_hash = blockchain.tip();
        let block = generate_random_block(&genesis_hash);
        blockchain.insert(&block);
        let tip_after_insertion: H256 = blockchain.tip();
        println!("new tip is {}", tip_after_insertion);
        let next_block: Block = generate_random_block(&tip_after_insertion);
        blockchain.insert(&next_block);

        assert_eq!(blockchain.tip(), next_block.hash());
    }

    #[test]
    fn insert_n() {
        let mut seeded_rng = StdRng::from_entropy();
        let n: u32 = seeded_rng.gen_range(1..1000);
        println!("running test for {} blocks", n);
        let mut blockchain = Blockchain::new();
        let genesis_hash = blockchain.tip();
        let mut block = generate_random_block(&genesis_hash);
        blockchain.insert(&block);
        for i in 0..n {
            let mut tip_after_insertion: H256 = blockchain.tip();
            let mut next_block: Block = generate_random_block(&tip_after_insertion);
            blockchain.insert(&next_block);
            assert_eq!(blockchain.tip(), next_block.hash());
        }
    }

    #[test]
    fn test_branching() {
        let mut blockchain = Blockchain::new();
        let genesis_hash = blockchain.tip();
        let block = generate_random_block(&genesis_hash);
        blockchain.insert(&block);
        let tip_to_branch_from: H256 = blockchain.tip();
        println!("new tip is {}", tip_to_branch_from);
        let next_block: Block = generate_random_block(&tip_to_branch_from);
        blockchain.insert(&next_block);
        let third_block = generate_random_block(&(blockchain.tip()));
        blockchain.insert(&third_block);
        let branching_block = generate_random_block(&tip_to_branch_from);
        blockchain.insert(&branching_block);

        assert_eq!(blockchain.tip(), third_block.hash());
    }

    #[test]
    fn branching_chain_switch() {
        let mut blockchain = Blockchain::new();
        let genesis_hash = blockchain.tip();
        let block = generate_random_block(&genesis_hash);
        blockchain.insert(&block);

        // G -- [1]
        let tip_to_branch_from: H256 = blockchain.tip();
        println!("new tip is {}", tip_to_branch_from);
        let next_block: Block = generate_random_block(&tip_to_branch_from);
        blockchain.insert(&next_block);
        // G -- [1] -- [2]
        let third_block = generate_random_block(&(blockchain.tip()));
        blockchain.insert(&third_block);
        // G -- [1] -- [2] -- [3]

        // insert first block on forked chain
        let branching_block = generate_random_block(&tip_to_branch_from);
        blockchain.insert(&branching_block);

        // G -- [1] -- [2] -- [3]
        //         \-- [4]

        // longest chain should be as normal, even with shorter diverging fork
        assert_eq!(blockchain.tip(), third_block.hash());

        // this insertion creates a tie, longest chain/ tip should stay the same according to specs
        // (only change tip when forked chain is strictly greater in height)
        let next_branch_block = generate_random_block(&branching_block.hash());
        blockchain.insert(&next_branch_block);
        // G -- [1] -- [2] -- [3]     // LONGEST CHAIN
        //         \-- [4] -- [5]
        assert_eq!(blockchain.tip(), third_block.hash());

        // this should cause the longest chain to switch
        let last_branch_block = generate_random_block(&next_branch_block.hash());
        blockchain.insert(&last_branch_block);
        // G <-- [1] <-- [2] <-- [3]
        //         \<--  [4] <-- [5] <-- [6]    // LONGEST CHAIN

        // check that tip returns the tip of the new longest chain
        assert_eq!(blockchain.tip(), last_branch_block.hash());
    }
}

// DO NOT CHANGE THIS COMMENT, IT IS FOR AUTOGRADER. AFTER TEST
