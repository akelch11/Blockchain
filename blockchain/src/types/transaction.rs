use super::hash::{Hashable, H256};
use crate::types::address::Address;
use ::rand::{rngs::StdRng, Rng, SeedableRng};
use ring::signature::{Ed25519KeyPair, EdDSAParameters, KeyPair, Signature, VerificationAlgorithm};
use ring::{
    digest, rand,
    signature::{self},
};
use serde::{Deserialize, Serialize};
use std::convert::TryInto;
use std::fmt::Display;

#[derive(Serialize, Deserialize, Debug, Default, Clone)]
pub struct Transaction {
    pub sender: Address,
    pub receiver: Address,
    pub value: i32,
    pub account_nonce: u32,
}

#[derive(Serialize, Deserialize, Debug, Default, Clone)]
pub struct SignedTransaction {
    pub transaction: Transaction,
    pub signature_vector: Vec<u8>,
    pub key_vector: Vec<u8>,
}

impl Hashable for SignedTransaction {
    fn hash(&self) -> H256 {
        let bytes: Vec<u8> = bincode::serialize(&self).unwrap();
        let hash: H256 = ring::digest::digest(&ring::digest::SHA256, &bytes).into();
        // println!("signed transaction hash is {}", hash);
        return hash;
    }
}

/// Create digital signature of a transaction
pub fn sign(t: &Transaction, key: &Ed25519KeyPair) -> Signature {
    // binary string that is hash of transaction value
    // let message: &[u8] = digest::digest(&digest::SHA256, t.value.as_ref()).as_ref();

    let encoded: Vec<u8> = bincode::serialize(&t).unwrap();
    let sign = key.sign(encoded.as_ref());
    return sign;
}

/// Verify digital signature of a transaction, using public key instead of secret key
pub fn verify(t: &Transaction, public_key: &[u8], signature: &[u8]) -> bool {
    // let message: &[u8] = &digest::digest(&digest::SHA256, t.value.as_ref()).as_ref();
    let message: &[u8] = &bincode::serialize(&t).unwrap();
    let peer_public_key = signature::UnparsedPublicKey::new(&signature::ED25519, public_key);
    let res = peer_public_key.verify(message, signature.as_ref());

    return res.is_ok();
}

pub fn signed_tx_verify(s_tx: &SignedTransaction, public_key: &[u8]) -> bool {
    let message: &[u8] = &bincode::serialize(&s_tx).unwrap();
    let peer_public_key = signature::UnparsedPublicKey::new(&signature::ED25519, public_key);
    let res = peer_public_key.verify(message, s_tx.signature_vector.as_ref());

    return res.is_ok();
}
// #[cfg(any(test, test_utilities))]
pub fn generate_random_transaction() -> Transaction {
    // unimplemented!()
    // random key pair
    // let num: i32 = rand::thread_rng().gen_range(0..100);
    // let rng = rand::SystemRandom::new();
    let rng = rand::SystemRandom::new();

    // // Normally the application would store the PKCS#8 file persistently. Later
    // // it would read the PKCS#8 file from persistent storage to use it.

    // let key_pair = signature::Ed25519KeyPair::from_pkcs8(pkcs8_bytes.as_ref())?;

    let mut seeded_rng = StdRng::from_entropy();

    let mut senderAddr: [u8; 20] = [0; 20];
    let mut receiverAddr: [u8; 20] = [0; 20];

    for i in 0..20 {
        let num = seeded_rng.gen_range(0..2);
        let num2 = seeded_rng.gen_range(0..2);
        senderAddr[i] = num;
        receiverAddr[i] = num2;
    }
    let trans: Transaction = Transaction {
        sender: Address::from(senderAddr),
        value: seeded_rng.gen_range(0..20),
        receiver: Address::from(receiverAddr),
        account_nonce: seeded_rng.gen_range(0..1000000),
    };
    // trans.value  = rand::generate::<i32>(&rng);

    return trans;
}

// DO NOT CHANGE THIS COMMENT, IT IS FOR AUTOGRADER. BEFORE TEST

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::key_pair;
    use ring::signature::KeyPair;

    #[test]
    fn sign_verify() {
        let t = generate_random_transaction();
        let key = key_pair::random();
        let signature = sign(&t, &key);
        assert!(verify(&t, key.public_key().as_ref(), signature.as_ref()));
    }
    #[test]
    fn sign_verify_two() {
        let t = generate_random_transaction();
        let key = key_pair::random();
        let signature = sign(&t, &key);
        let key_2 = key_pair::random();
        let t_2 = generate_random_transaction();
        assert!(!verify(&t_2, key.public_key().as_ref(), signature.as_ref()));
        assert!(!verify(&t, key_2.public_key().as_ref(), signature.as_ref()));
    }
}

// DO NOT CHANGE THIS COMMENT, IT IS FOR AUTOGRADER. AFTER TEST
