use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use secret_toolkit::storage::Item;
use cosmwasm_std::Addr;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    pub owner: Addr,
}

// Storage
pub const CONFIG: Item<Config> = Item::new(b"config");
