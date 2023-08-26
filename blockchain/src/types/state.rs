use std::collections::HashMap;
use std::hash::Hash;

use crate::types::account::Account;
use crate::types::address::Address;

#[derive(Clone)]
pub struct State {
    pub account_state_map: HashMap<Address, Account>,
}

impl State {
    pub fn new() -> Self {
        let acc_map: HashMap<Address, Account> = HashMap::new();

        Self {
            account_state_map: acc_map,
        }
    }
}
