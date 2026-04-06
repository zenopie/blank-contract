use cosmwasm_std::{
    entry_point, to_binary, from_binary, Addr, Binary, Deps, DepsMut, Env, MessageInfo,
    QueryResponse, Response, StdError, StdResult, Uint128, CosmosMsg, WasmMsg,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use secret_toolkit::storage::Item;
use crate::msg::{
    ExecuteMsg, InstantiateMsg, QueryMsg, ConfigResponse, IsProcessedResponse,
    DepositResponse, WithdrawalsResponse, Snip20ExecuteMsg, MigrateMsg, ReceiveMsg,
    ExchangeReceiveMsg, RegisterDepositAddressResponse, DepositIndexResponse,
    AllDepositIndicesResponse,
};
use crate::state::{
    Config, ContractInfo, ProcessedDeposit, Withdrawal, CONFIG, PROCESSED_TXS,
    PENDING_WITHDRAWALS, COMPLETED_WITHDRAWALS, DEPOSIT_INDEX_COUNTER, DEPOSIT_INDICES,
    load_bridge_contracts,
};

const FEE_DENOMINATOR: u128 = 10_000;

#[entry_point]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> StdResult<Response> {
    let owner = deps.api.addr_validate(&msg.owner)?;
    let backend_wallet = deps.api.addr_validate(&msg.backend_wallet)?;
    let registry_contract = deps.api.addr_validate(&msg.registry_contract)?;

    if msg.fee_basis_points > 10_000 {
        return Err(StdError::generic_err("Fee cannot exceed 100%"));
    }

    let config = Config {
        owner,
        backend_wallet,
        registry_contract,
        registry_hash: msg.registry_hash,
        fee_basis_points: msg.fee_basis_points,
        fee_enabled: msg.fee_enabled,
        bridge_enabled: msg.bridge_enabled,
        total_minted: Uint128::zero(),
        total_burned: Uint128::zero(),
        total_fees_collected: Uint128::zero(),
        withdrawal_counter: 0,
    };
    CONFIG.save(deps.storage, &config)?;

    DEPOSIT_INDEX_COUNTER.save(deps.storage, &1u32)?;

    Ok(Response::new()
        .add_attribute("action", "instantiate")
        .add_attribute("owner", msg.owner)
        .add_attribute("backend_wallet", msg.backend_wallet)
        .add_attribute("fee_basis_points", msg.fee_basis_points.to_string()))
}

#[entry_point]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> StdResult<Response> {
    match msg {
        ExecuteMsg::ProcessDeposit { recipient, amount, monero_tx_id } =>
            execute_process_deposit(deps, env, info, recipient, amount, monero_tx_id),
        ExecuteMsg::Receive { sender, from, amount, memo: _, msg } =>
            execute_receive(deps, env, info, sender, from, amount, msg),
        ExecuteMsg::UpdateConfig { config } => execute_update_config(deps, info, config),
        ExecuteMsg::CompleteWithdrawal { withdrawal_id } => execute_complete_withdrawal(deps, info, withdrawal_id),
        ExecuteMsg::RegisterDepositAddress {} => execute_register_deposit_address(deps, info),
    }
}

fn execute_process_deposit(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    recipient: String,
    amount: Uint128,
    monero_tx_id: String,
) -> StdResult<Response> {
    let mut config = CONFIG.load(deps.storage)?;

    if !config.bridge_enabled {
        return Err(StdError::generic_err("Bridge is currently disabled"));
    }
    if info.sender != config.backend_wallet {
        return Err(StdError::generic_err("Unauthorized: only backend wallet can process deposits"));
    }
    if PROCESSED_TXS.get(deps.storage, &monero_tx_id).is_some() {
        return Err(StdError::generic_err("Monero transaction already processed"));
    }

    let recipient_addr = deps.api.addr_validate(&recipient)?;

    let fee_amount = if config.fee_enabled {
        amount.checked_mul(Uint128::from(config.fee_basis_points as u128))?
              .checked_div(Uint128::from(FEE_DENOMINATOR))?
    } else {
        Uint128::zero()
    };
    let mint_amount = amount.checked_sub(fee_amount)?;

    let deps_ref = deps.as_ref();
    let addrs = load_bridge_contracts(&deps_ref, &config)?;

    let mut messages: Vec<CosmosMsg> = vec![];

    // Mint to recipient
    messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: addrs.xmr_token.address.to_string(),
        code_hash: addrs.xmr_token.code_hash.clone(),
        msg: to_binary(&Snip20ExecuteMsg::Mint {
            recipient: recipient.clone(),
            amount: mint_amount,
            memo: Some(format!("XMR Bridge deposit: {}", monero_tx_id)),
            padding: None,
        })?,
        funds: vec![],
    }));

    // Mint fee to contract, send to exchange for burn
    if !fee_amount.is_zero() {
        messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: addrs.xmr_token.address.to_string(),
            code_hash: addrs.xmr_token.code_hash.clone(),
            msg: to_binary(&Snip20ExecuteMsg::Mint {
                recipient: env.contract.address.to_string(),
                amount: fee_amount,
                memo: Some(format!("XMR Bridge fee: {}", monero_tx_id)),
                padding: None,
            })?,
            funds: vec![],
        }));

        messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: addrs.xmr_token.address.to_string(),
            code_hash: addrs.xmr_token.code_hash.clone(),
            msg: to_binary(&Snip20ExecuteMsg::Send {
                recipient: addrs.exchange.address.to_string(),
                recipient_code_hash: addrs.exchange.code_hash.clone(),
                amount: fee_amount,
                msg: Some(to_binary(&ExchangeReceiveMsg::SwapToErthAndBurn {})?),
                memo: Some("XMR Bridge fee swap".to_string()),
                padding: None,
            })?,
            funds: vec![],
        }));
    }

    let processed_deposit = ProcessedDeposit {
        recipient: recipient_addr,
        amount: mint_amount,
        fee: fee_amount,
        timestamp: env.block.time.seconds(),
    };
    PROCESSED_TXS.insert(deps.storage, &monero_tx_id, &processed_deposit)?;

    config.total_minted = config.total_minted.checked_add(mint_amount)?;
    config.total_fees_collected = config.total_fees_collected.checked_add(fee_amount)?;
    CONFIG.save(deps.storage, &config)?;

    Ok(Response::new()
        .add_messages(messages)
        .add_attribute("action", "process_deposit")
        .add_attribute("monero_tx_id", monero_tx_id)
        .add_attribute("recipient", recipient)
        .add_attribute("amount", mint_amount.to_string())
        .add_attribute("fee", fee_amount.to_string()))
}

#[allow(clippy::too_many_arguments)]
fn execute_receive(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    _sender: String,
    from: String,
    amount: Uint128,
    msg: Binary,
) -> StdResult<Response> {
    let mut config = CONFIG.load(deps.storage)?;

    if !config.bridge_enabled {
        return Err(StdError::generic_err("Bridge is currently disabled"));
    }

    let deps_ref = deps.as_ref();
    let addrs = load_bridge_contracts(&deps_ref, &config)?;

    if info.sender != addrs.xmr_token.address {
        return Err(StdError::generic_err("Only XMR tokens can be sent to this contract"));
    }

    let receive_msg: ReceiveMsg = from_binary(&msg)?;
    let monero_address = match receive_msg {
        ReceiveMsg::Withdraw { monero_address } => monero_address,
    };

    if !monero_address.starts_with('4') && !monero_address.starts_with('8') {
        return Err(StdError::generic_err("Invalid Monero address format"));
    }

    let fee_amount = if config.fee_enabled {
        amount.checked_mul(Uint128::from(config.fee_basis_points as u128))?
              .checked_div(Uint128::from(FEE_DENOMINATOR))?
    } else {
        Uint128::zero()
    };
    let burn_amount = amount.checked_sub(fee_amount)?;

    config.withdrawal_counter += 1;
    let withdrawal_id = config.withdrawal_counter;

    let mut messages: Vec<CosmosMsg> = vec![];

    // Burn tokens
    messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: addrs.xmr_token.address.to_string(),
        code_hash: addrs.xmr_token.code_hash.clone(),
        msg: to_binary(&Snip20ExecuteMsg::Burn {
            amount: burn_amount,
            memo: Some(format!("XMR Bridge withdrawal #{}", withdrawal_id)),
            padding: None,
        })?,
        funds: vec![],
    }));

    // Send fee to exchange
    if !fee_amount.is_zero() {
        messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: addrs.xmr_token.address.to_string(),
            code_hash: addrs.xmr_token.code_hash.clone(),
            msg: to_binary(&Snip20ExecuteMsg::Send {
                recipient: addrs.exchange.address.to_string(),
                recipient_code_hash: addrs.exchange.code_hash.clone(),
                amount: fee_amount,
                msg: Some(to_binary(&ExchangeReceiveMsg::SwapToErthAndBurn {})?),
                memo: Some("XMR Bridge fee swap".to_string()),
                padding: None,
            })?,
            funds: vec![],
        }));
    }

    let from_addr = deps.api.addr_validate(&from)?;
    let withdrawal = Withdrawal {
        id: withdrawal_id,
        from: from_addr.clone(),
        monero_address,
        amount: burn_amount,
        fee: fee_amount,
        timestamp: env.block.time.seconds(),
    };

    let mut pending = PENDING_WITHDRAWALS.get(deps.storage, &from_addr).unwrap_or_default();
    pending.push(withdrawal);
    PENDING_WITHDRAWALS.insert(deps.storage, &from_addr, &pending)?;

    config.total_burned = config.total_burned.checked_add(burn_amount)?;
    config.total_fees_collected = config.total_fees_collected.checked_add(fee_amount)?;
    CONFIG.save(deps.storage, &config)?;

    Ok(Response::new()
        .add_messages(messages)
        .add_attribute("action", "withdrawal")
        .add_attribute("withdrawal_id", withdrawal_id.to_string()))
}

fn execute_update_config(
    deps: DepsMut,
    info: MessageInfo,
    new_config: Config,
) -> StdResult<Response> {
    let current_config = CONFIG.load(deps.storage)?;

    if info.sender != current_config.owner {
        return Err(StdError::generic_err("Unauthorized: only owner can update config"));
    }
    if new_config.fee_basis_points > 10_000 {
        return Err(StdError::generic_err("Fee cannot exceed 100%"));
    }

    CONFIG.save(deps.storage, &new_config)?;

    Ok(Response::new().add_attribute("action", "update_config"))
}

fn execute_complete_withdrawal(
    deps: DepsMut,
    info: MessageInfo,
    withdrawal_id: u64,
) -> StdResult<Response> {
    let config = CONFIG.load(deps.storage)?;

    if info.sender != config.backend_wallet {
        return Err(StdError::generic_err("Unauthorized: only backend wallet can complete withdrawals"));
    }

    let mut found_withdrawal: Option<Withdrawal> = None;
    let mut found_addr: Option<Addr> = None;

    let keys: Vec<_> = PENDING_WITHDRAWALS.iter_keys(deps.storage)?.collect();
    for key_result in keys {
        let addr = key_result?;
        if let Some(mut pending) = PENDING_WITHDRAWALS.get(deps.storage, &addr) {
            if let Some(pos) = pending.iter().position(|w| w.id == withdrawal_id) {
                found_withdrawal = Some(pending.remove(pos));
                found_addr = Some(addr.clone());

                if pending.is_empty() {
                    PENDING_WITHDRAWALS.remove(deps.storage, &addr)?;
                } else {
                    PENDING_WITHDRAWALS.insert(deps.storage, &addr, &pending)?;
                }
                break;
            }
        }
    }

    let withdrawal = found_withdrawal.ok_or_else(|| StdError::generic_err("Withdrawal not found"))?;
    let addr = found_addr.unwrap();

    let mut completed = COMPLETED_WITHDRAWALS.get(deps.storage, &addr).unwrap_or_default();
    completed.push(withdrawal);
    COMPLETED_WITHDRAWALS.insert(deps.storage, &addr, &completed)?;

    Ok(Response::new()
        .add_attribute("action", "complete_withdrawal")
        .add_attribute("withdrawal_id", withdrawal_id.to_string()))
}

fn execute_register_deposit_address(
    deps: DepsMut,
    info: MessageInfo,
) -> StdResult<Response> {
    if let Some(existing_index) = DEPOSIT_INDICES.get(deps.storage, &info.sender) {
        return Ok(Response::new()
            .add_attribute("action", "register_deposit_address")
            .add_attribute("index", existing_index.to_string())
            .set_data(to_binary(&RegisterDepositAddressResponse { index: existing_index })?));
    }

    let next_index = DEPOSIT_INDEX_COUNTER.load(deps.storage)?;
    DEPOSIT_INDICES.insert(deps.storage, &info.sender, &next_index)?;
    DEPOSIT_INDEX_COUNTER.save(deps.storage, &(next_index + 1))?;

    Ok(Response::new()
        .add_attribute("action", "register_deposit_address")
        .add_attribute("index", next_index.to_string())
        .set_data(to_binary(&RegisterDepositAddressResponse { index: next_index })?))
}

// Old config for migration
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct OldConfig {
    pub owner: Addr,
    pub backend_wallet: Addr,
    pub xmr_token: ContractInfo,
    pub exchange_contract: ContractInfo,
    pub fee_basis_points: u16,
    pub fee_enabled: bool,
    pub bridge_enabled: bool,
    pub total_minted: Uint128,
    pub total_burned: Uint128,
    pub total_fees_collected: Uint128,
    pub withdrawal_counter: u64,
}

#[entry_point]
pub fn migrate(deps: DepsMut, _env: Env, msg: MigrateMsg) -> StdResult<Response> {
    match msg {
        MigrateMsg::Migrate { registry_contract, registry_hash } => {
            let old_config_storage: Item<OldConfig> = Item::new(b"config");
            let old_config = old_config_storage.load(deps.storage)?;

            let registry_addr = deps.api.addr_validate(&registry_contract)?;

            let new_config = Config {
                owner: old_config.owner,
                backend_wallet: old_config.backend_wallet,
                registry_contract: registry_addr,
                registry_hash,
                fee_basis_points: old_config.fee_basis_points,
                fee_enabled: old_config.fee_enabled,
                bridge_enabled: old_config.bridge_enabled,
                total_minted: old_config.total_minted,
                total_burned: old_config.total_burned,
                total_fees_collected: old_config.total_fees_collected,
                withdrawal_counter: old_config.withdrawal_counter,
            };
            CONFIG.save(deps.storage, &new_config)?;

            Ok(Response::new()
                .add_attribute("action", "migrate")
                .add_attribute("status", "success"))
        }
        MigrateMsg::Upgrade {} => {
            Ok(Response::new().add_attribute("action", "upgrade"))
        }
    }
}

#[entry_point]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<QueryResponse> {
    match msg {
        QueryMsg::GetConfig {} => to_binary(&query_config(deps)?),
        QueryMsg::IsProcessed { monero_tx_id } => to_binary(&query_is_processed(deps, monero_tx_id)?),
        QueryMsg::GetDeposit { monero_tx_id } => to_binary(&query_deposit(deps, monero_tx_id)?),
        QueryMsg::GetPendingWithdrawals { address } => to_binary(&query_pending_withdrawals(deps, address)?),
        QueryMsg::GetCompletedWithdrawals { address } => to_binary(&query_completed_withdrawals(deps, address)?),
        QueryMsg::GetDepositIndex { address } => to_binary(&query_deposit_index(deps, address)?),
        QueryMsg::GetAllDepositIndices { start_after, limit } => {
            to_binary(&query_all_deposit_indices(deps, start_after, limit)?)
        }
    }
}

fn query_config(deps: Deps) -> StdResult<ConfigResponse> {
    let config = CONFIG.load(deps.storage)?;
    Ok(ConfigResponse { config })
}

fn query_is_processed(deps: Deps, monero_tx_id: String) -> StdResult<IsProcessedResponse> {
    let is_processed = PROCESSED_TXS.get(deps.storage, &monero_tx_id).is_some();
    Ok(IsProcessedResponse { is_processed })
}

fn query_deposit(deps: Deps, monero_tx_id: String) -> StdResult<DepositResponse> {
    let deposit = PROCESSED_TXS.get(deps.storage, &monero_tx_id);
    Ok(DepositResponse { deposit })
}

fn query_pending_withdrawals(deps: Deps, address: String) -> StdResult<WithdrawalsResponse> {
    let addr = deps.api.addr_validate(&address)?;
    let withdrawals = PENDING_WITHDRAWALS.get(deps.storage, &addr).unwrap_or_default();
    Ok(WithdrawalsResponse { withdrawals })
}

fn query_completed_withdrawals(deps: Deps, address: String) -> StdResult<WithdrawalsResponse> {
    let addr = deps.api.addr_validate(&address)?;
    let withdrawals = COMPLETED_WITHDRAWALS.get(deps.storage, &addr).unwrap_or_default();
    Ok(WithdrawalsResponse { withdrawals })
}

fn query_deposit_index(deps: Deps, address: String) -> StdResult<DepositIndexResponse> {
    let addr = deps.api.addr_validate(&address)?;
    let index = DEPOSIT_INDICES.get(deps.storage, &addr);
    Ok(DepositIndexResponse { index })
}

fn query_all_deposit_indices(
    deps: Deps,
    start_after: Option<String>,
    limit: Option<u32>,
) -> StdResult<AllDepositIndicesResponse> {
    let limit = limit.unwrap_or(100).min(500) as usize;

    let start_addr = start_after
        .map(|s| deps.api.addr_validate(&s))
        .transpose()?;

    let mut indices: Vec<(String, u32)> = Vec::new();
    let mut found_start = start_addr.is_none();

    for key_result in DEPOSIT_INDICES.iter_keys(deps.storage)? {
        let addr = key_result?;

        if !found_start {
            if Some(&addr) == start_addr.as_ref() {
                found_start = true;
            }
            continue;
        }

        if let Some(index) = DEPOSIT_INDICES.get(deps.storage, &addr) {
            indices.push((addr.to_string(), index));
            if indices.len() >= limit {
                break;
            }
        }
    }

    Ok(AllDepositIndicesResponse { indices })
}
