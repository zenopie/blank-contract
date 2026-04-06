use cosmwasm_std::{
    entry_point, to_binary, Deps, DepsMut, Env, MessageInfo,
    QueryResponse, Response, StdError, StdResult,
};
use crate::msg::{ExecuteMsg, InstantiateMsg, QueryMsg};
use crate::state::{Config, CONFIG};

#[entry_point]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> StdResult<Response> {
    let owner = deps.api.addr_validate(&msg.owner)?;

    let config = Config { owner };
    CONFIG.save(deps.storage, &config)?;

    Ok(Response::new()
        .add_attribute("action", "instantiate")
        .add_attribute("owner", msg.owner))
}

#[entry_point]
pub fn execute(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> StdResult<Response> {
    match msg {
        ExecuteMsg::UpdateConfig { config } => execute_update_config(deps, info, config),
    }
}

fn execute_update_config(
    deps: DepsMut,
    info: MessageInfo,
    config: Config,
) -> StdResult<Response> {
    let old_config = CONFIG.load(deps.storage)?;

    if info.sender != old_config.owner {
        return Err(StdError::generic_err("Unauthorized"));
    }

    CONFIG.save(deps.storage, &config)?;

    Ok(Response::new().add_attribute("action", "update_config"))
}

#[entry_point]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<QueryResponse> {
    match msg {
        QueryMsg::GetConfig {} => to_binary(&query_config(deps)?),
    }
}

fn query_config(deps: Deps) -> StdResult<Config> {
    CONFIG.load(deps.storage)
}
