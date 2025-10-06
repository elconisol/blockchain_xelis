use xelis_vm::tid;

use crate::{
    account::CiphertextCache,
    asset::AssetData,
    block::TopoHeight,
    crypto::{Hash, PublicKey},
};

use super::ContractStorage;

/// A trait providing high-level read access to contract-related data.
/// Implementations must wrap or extend a lower-level [`ContractStorage`].
pub trait ContractProvider: ContractStorage + 'static {
    /// Returns the balance of a contract for a specific asset.
    fn get_contract_balance_for_asset(
        &self,
        contract: &Hash,
        asset: &Hash,
        topoheight: TopoHeight,
    ) -> Result<Option<(TopoHeight, u64)>, anyhow::Error>;

    /// Returns the account balance for an asset at a given topological height.
    fn get_account_balance_for_asset(
        &self,
        key: &PublicKey,
        asset: &Hash,
        topoheight: TopoHeight,
    ) -> Result<Option<(TopoHeight, CiphertextCache)>, anyhow::Error>;

    /// Checks whether an asset exists in storage at a given height.
    fn asset_exists(
        &self,
        asset: &Hash,
        topoheight: TopoHeight,
    ) -> Result<bool, anyhow::Error>;

    /// Loads full asset data (metadata, rules, etc.) from storage.
    fn load_asset_data(
        &self,
        asset: &Hash,
        topoheight: TopoHeight,
    ) -> Result<Option<(TopoHeight, AssetData)>, anyhow::Error>;

    /// Loads the total supply of a given asset.
    fn load_asset_supply(
        &self,
        asset: &Hash,
        topoheight: TopoHeight,
    ) -> Result<Option<(TopoHeight, u64)>, anyhow::Error>;

    /// Verifies if an account (public key) exists in the ledger.
    fn account_exists(
        &self,
        key: &PublicKey,
        topoheight: TopoHeight,
    ) -> Result<bool, anyhow::Error>;
}

/// A wrapper allowing a mutable [`ContractProvider`] to be passed within a [`Context`].
pub struct ContractProviderWrapper<'a, S: ContractProvider>(pub &'a mut S);

tid! {
    impl<'a, S: 'static> TidAble<'a> for ContractProviderWrapper<'a, S>
    where
        S: ContractProvider
}
