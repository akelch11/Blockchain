use super::hash::{Hashable, H256};
use hex_literal::hex;
use ring::digest;
use ring::digest::Context;
use std::cell::RefCell;
use std::rc::Rc;

/// A Merkle tree.
#[derive(Debug, Default)]
pub struct MerkleTree {
    pub root_hash: H256,
    pub tree_hash_vector: Vec<Vec<H256>>,
}

impl MerkleTree {
    pub fn new<T>(data: &[T]) -> Self
    where
        T: Hashable,
    {
        let num_blocks = data.len();

        let mut tree_vector: Vec<Vec<H256>> = Vec::new();

        if num_blocks == 0 {
            let ret: MerkleTree = MerkleTree {
                root_hash: (hex!(
                    "0000000000000000000000000000000000000000000000000000000000000000"
                ))
                .into(),
                tree_hash_vector: tree_vector,
            };
            return ret;
        }

        let mut cur_level: Vec<H256> = Vec::new();

        for i in 0..num_blocks {
            (cur_level).push(data[i].hash());
        }

        tree_vector.push(cur_level.clone());

        let mut cur_level_len = cur_level.len();
        let mut parent_level: Vec<H256> = Vec::new();

        while cur_level_len > 1 {
            let mut ind = 0;
            let mut len = cur_level_len;
            while ind < len {
                // println!("index is {}", ind);
                let mut pair_hash: H256;
                let mut ctx = Context::new(&digest::SHA256);

                // hash first node
                ctx.update((cur_level[ind]).as_ref());

                if ind + 1 < len {
                    //hash second node
                    ctx.update((cur_level[ind + 1]).as_ref());
                } else {
                    // hash first node again, not a second node for pair
                    // println!("Odd instance, node {} has hash {}", cur_level[ind].clone(),  cur_level[ind].clone().hash());
                    ctx.update((cur_level[ind]).as_ref());
                }

                pair_hash = ctx.finish().into();
                // println!("Pair hash is {}", pair_hash);

                parent_level.push(pair_hash);

                ind += 2;
            }
            // update length of next level
            cur_level_len = parent_level.len();
            // next level to traverse is the parent level relative to current
            cur_level = parent_level.clone();
            // add parent level to tree
            tree_vector.push(parent_level.clone());
            parent_level = Vec::new();
        }

        // prints tree nodes in level order
        // println!("THIS IS TREE VEC");
        // for a in 0..tree_vector.len() {
        //     for b in 0..(tree_vector[a].len()) {
        //         println!("{}", tree_vector[a][b])
        //     }
        //     println!("NEW LEV");
        // }

        let ret: MerkleTree = MerkleTree {
            root_hash: tree_vector.last().unwrap()[0],
            tree_hash_vector: tree_vector,
        };
        return ret;
    }

    pub fn root(&self) -> H256 {
        return self.root_hash;
    }

    /// Returns the Merkle Proof of data at index i
    pub fn proof(&self, index: usize) -> Vec<H256> {
        let mut num_tree_levels = self.tree_hash_vector.len();
        let mut proof_vec: Vec<H256> = Vec::new();
        let mut i = index;

        if self.tree_hash_vector.len() == 0 || index >= self.tree_hash_vector[0].len() {
            return Vec::new();
        }

        for l in 0..(num_tree_levels - 1) {
            if i % 2 == 1 {
                proof_vec.push(self.tree_hash_vector[l][i - 1].clone());
            } else {
                if i + 1 < self.tree_hash_vector[l].len() {
                    proof_vec.push(self.tree_hash_vector[l][i + 1].clone());
                } else {
                    proof_vec.push(self.tree_hash_vector[l][i].clone());
                }
            }
            i = i / 2;
        }

        return proof_vec;
    }
}

/// Verify that the datum hash with a vector of proofs will produce the Merkle root. Also need the
/// index of datum and `leaf_size`, the total number of leaves.
pub fn verify(root: &H256, datum: &H256, proof: &[H256], index: usize, leaf_size: usize) -> bool {
    let mut i = index;
    let mut prev_hash: H256 = *datum;
    let mut size = leaf_size;

    if leaf_size == 0 || index >= leaf_size {
        return false;
    }

    // println!("Proof vec is:");
    // for i in 0..((proof).len())
    // {
    //     println!("entry {} is: {}", i, proof[i]);
    // }

    for l in 0..proof.len() {
        let mut ctx = Context::new(&digest::SHA256);
        // current node pointed to by index is on the right, so hash proof sibling first
        if i % 2 == 1 {
            // println!("concatenate {} with {}", proof[l].clone(), prev_hash.clone());
            ctx.update(proof[l].as_ref());
            ctx.update(prev_hash.as_ref());
        } else
        // i % 2 == 0; current node is on left, hash it first then proof sibling
        {
            ctx.update(prev_hash.as_ref());
            ctx.update(proof[l].as_ref());
        }

        prev_hash = ctx.finish().into();
        // println!("result is {}", prev_hash.clone());

        size = size / 2;
        i = i / 2;
    }

    // println!("root is {}, prev is {}", (*root).clone(), prev_hash.clone());
    let ans: bool = *root == prev_hash;
    return ans;
}
// DO NOT CHANGE THIS COMMENT, IT IS FOR AUTOGRADER. BEFORE TEST

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::hash::H256;

    macro_rules! gen_merkle_tree_data {
        () => {{
            vec![
                (hex!("0a0b0c0d0e0f0e0d0a0b0c0d0e0f0e0d0a0b0c0d0e0f0e0d0a0b0c0d0e0f0e0d")).into(),
                (hex!("0101010101010101010101010101010101010101010101010101010101010202")).into(),
            ]
        }};
    }

    #[test]
    fn merkle_root() {
        let input_data: Vec<H256> = gen_merkle_tree_data!();
        let merkle_tree = MerkleTree::new(&input_data);
        let root = merkle_tree.root();
        assert_eq!(
            root,
            (hex!("6b787718210e0b3b608814e04e61fde06d0df794319a12162f287412df3ec920")).into()
        );
        // "b69566be6e1720872f73651d1851a0eae0060a132cf0f64a0ffaea248de6cba0" is the hash of
        // "0a0b0c0d0e0f0e0d0a0b0c0d0e0f0e0d0a0b0c0d0e0f0e0d0a0b0c0d0e0f0e0d"
        // "965b093a75a75895a351786dd7a188515173f6928a8af8c9baa4dcff268a4f0f" is the hash of
        // "0101010101010101010101010101010101010101010101010101010101010202"
        // "6b787718210e0b3b608814e04e61fde06d0df794319a12162f287412df3ec920" is the hash of
        // the concatenation of these two hashes "b69..." and "965..."
        // notice that the order of these two matters
    }

    #[test]
    fn merkle_proof() {
        let input_data: Vec<H256> = gen_merkle_tree_data!();
        let merkle_tree = MerkleTree::new(&input_data);
        let proof = merkle_tree.proof(0);
        assert_eq!(
            proof,
            vec![hex!("965b093a75a75895a351786dd7a188515173f6928a8af8c9baa4dcff268a4f0f").into()]
        );
        // "965b093a75a75895a351786dd7a188515173f6928a8af8c9baa4dcff268a4f0f" is the hash of
        // "0101010101010101010101010101010101010101010101010101010101010202"
    }

    #[test]
    fn merkle_verifying() {
        let input_data: Vec<H256> = gen_merkle_tree_data!();
        let merkle_tree = MerkleTree::new(&input_data);
        let proof = merkle_tree.proof(0);
        assert!(verify(
            &merkle_tree.root(),
            &input_data[0].hash(),
            &proof,
            0,
            input_data.len()
        ));
    }
}

// DO NOT CHANGE THIS COMMENT, IT IS FOR AUTOGRADER. AFTER TEST
