use xelis_vm::tid;

use crate::{
    account::CiphertextCache,
    asset::AssetData,
    block::TopoHeight,
    crypto::{Hash, PublicKey}
};

use super::ContractStorage;

pub trait ContractProvider: ContractStorage + 'static {
    fn get_contract_balance_for_asset(
        &self,
        contract: &Hash,
        asset: &Hash,
        topoheight: TopoHeight,
    ) -> Result<Option<(TopoHeight, u64)>, ContractProviderError>;

    fn get_account_balance_for_asset(
        &self,
        key: &PublicKey,
        asset: &Hash,
        topoheight: TopoHeight,
    ) -> Result<Option<(TopoHeight, CiphertextCache)>, ContractProviderError>;

    fn asset_exists(
        &self,
        asset: &Hash,
        topoheight: TopoHeight,
    ) -> Result<Option<TopoHeight>, ContractProviderError>;

    fn load_asset_data(
        &self,
        asset: &Hash,
        topoheight: TopoHeight,
    ) -> Result<Option<(TopoHeight, AssetData)>, ContractProviderError>;

    fn load_asset_supply(
        &self,
        asset: &Hash,
        topoheight: TopoHeight,
    ) -> Result<Option<(TopoHeight, u64)>, ContractProviderError>;

    fn account_exists(
        &self,
        key: &PublicKey,
        topoheight: TopoHeight,
    ) -> Result<Option<TopoHeight>, ContractProviderError>;
}

#[derive(thiserror::Error, Debug)]
pub enum ContractProviderError {
    #[error("Storage backend error: {0}")]
    Storage(String),
    #[error("Invalid state: {0}")]
    InvalidState(String),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

pub struct ContractProviderWrapper<'a, S: ContractProvider>(pub &'a mut S);

tid! { impl<'a, S: 'static> TidAble<'a> for ContractProviderWrapper<'a, S> where S: ContractProvider }

