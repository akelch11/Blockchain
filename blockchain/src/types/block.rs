use crate::types::hash::{Hashable, H256};
use crate::types::merkle::MerkleTree;
use crate::types::transaction::SignedTransaction;
use ::rand::{rngs::StdRng, Rng, SeedableRng};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Header {
    pub parent: H256,
    pub nonce: u32,
    pub difficulty: H256,
    pub timestamp: u128,
    pub merkle_root: H256,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Content {
    pub content: Vec<SignedTransaction>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Block {
    pub header: Header,
    pub content: Content,
}

impl Hashable for Header {
    fn hash(&self) -> H256 {
        let bytes: Vec<u8> = bincode::serialize(&self).unwrap();
        let hash: H256 = ring::digest::digest(&ring::digest::SHA256, &bytes).into();
        // println!("header hash is {}", hash);
        return hash;
    }
}

impl Hashable for Block {
    fn hash(&self) -> H256 {
        return (self.header).hash();
    }
}

impl Block {
    pub fn get_parent(&self) -> H256 {
        return self.header.parent;
    }

    pub fn get_difficulty(&self) -> H256 {
        return self.header.difficulty;
    }
}

#[cfg(any(test, test_utilities))]
pub fn generate_random_block(parent: &H256) -> Block {
    let mut seeded_rng = StdRng::from_entropy();
    let nonce: u32 = seeded_rng.gen_range(0..1000000);
    let diff: H256 =
        hex!("0a0b0c0d0e0f0e0d0a0b0c0d0e0f0e0d0a0b0c0d0e0f0e0d0a0b0c0d0e0f0e0d").into();
    let time: u128 = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis();

    let empty_data: Vec<H256> = Vec::new();
    let merkle: MerkleTree = MerkleTree::new(&empty_data);
    let root: H256 = merkle.root();

    let header: Header = Header {
        parent: *parent,
        nonce: nonce,
        difficulty: diff,
        timestamp: time,
        merkle_root: root,
    };

    let content: Content = Content {
        content: Vec::new().into(),
    };

    Block {
        header: header,
        content: content,
    }
}
