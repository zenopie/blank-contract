use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use secret_toolkit::storage::{Item, Keymap};
use cosmwasm_std::{Addr, Uint128, Deps, StdResult, StdError, to_binary, QueryRequest, WasmQuery};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    pub owner: Addr,
    pub backend_wallet: Addr,
    pub registry_contract: Addr,
    pub registry_hash: String,
    pub fee_basis_points: u16, // 50 = 0.5%
    pub fee_enabled: bool,
    pub bridge_enabled: bool,
    pub total_minted: Uint128,
    pub total_burned: Uint128,
    pub total_fees_collected: Uint128,
    pub withdrawal_counter: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ContractInfo {
    pub address: Addr,
    pub code_hash: String,
}

pub struct BridgeContracts {
    pub xmr_token: ContractInfo,
    pub exchange: ContractInfo,
}

// Storage
pub const CONFIG: Item<Config> = Item::new(b"config");
pub const PROCESSED_TXS: Keymap<String, ProcessedDeposit> = Keymap::new(b"processed_txs");
pub const PENDING_WITHDRAWALS: Keymap<Addr, Vec<Withdrawal>> = Keymap::new(b"pending_withdrawals");
pub const COMPLETED_WITHDRAWALS: Keymap<Addr, Vec<Withdrawal>> = Keymap::new(b"completed_withdrawals");
pub const DEPOSIT_INDEX_COUNTER: Item<u32> = Item::new(b"deposit_index_counter");
pub const DEPOSIT_INDICES: Keymap<Addr, u32> = Keymap::new(b"deposit_indices");

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ProcessedDeposit {
    pub recipient: Addr,
    pub amount: Uint128,
    pub fee: Uint128,
    pub timestamp: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Withdrawal {
    pub id: u64,
    pub from: Addr,
    pub monero_address: String,
    pub amount: Uint128,
    pub fee: Uint128,
    pub timestamp: u64,
}

// Registry query types
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RegistryQueryMsg {
    GetContracts { names: Vec<String> },
}

#[derive(Serialize, Deserialize)]
pub struct ContractResponse {
    pub name: String,
    pub info: ContractInfo,
}

#[derive(Serialize, Deserialize)]
pub struct AllContractsResponse {
    pub contracts: Vec<ContractResponse>,
}

pub fn load_bridge_contracts(deps: &Deps, config: &Config) -> StdResult<BridgeContracts> {
    let query_msg = RegistryQueryMsg::GetContracts {
        names: vec!["xmr_token".to_string(), "exchange".to_string()],
    };
    let response: AllContractsResponse = deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: config.registry_contract.to_string(),
        code_hash: config.registry_hash.to_string(),
        msg: to_binary(&query_msg)?,
    }))?;
    if response.contracts.len() != 2 {
        return Err(StdError::generic_err(
            format!("Registry returned {} contracts, expected 2", response.contracts.len())
        ));
    }
    let contracts: Vec<ContractInfo> = response.contracts.into_iter().map(|c| c.info).collect();
    Ok(BridgeContracts {
        xmr_token: contracts[0].clone(),
        exchange: contracts[1].clone(),
    })
}
