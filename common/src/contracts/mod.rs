mod abis;
mod config;
use crate::prelude::*;
use eip_712_derive::U256;
use web3::types::Address;
pub mod dry_run;

pub use abis::*;

macro_rules! contracts {
    ($($field:ident: $T:ident),+) => {
        /// An interface for our contracts.
        pub struct Contracts<Context, Provider> {
            context: Context,
            chain_id: U256,
            $($field: $T<Provider>,)+
        }

        impl<Context: Clone, Provider> Clone for Contracts<Context, Provider> {
            fn clone(&self) -> Self {
                Self {
                    context: self.context.clone(),
                    chain_id: self.chain_id.clone(),
                    $($field: self.$field.clone(),)+
                }
            }
        }

        pub struct ContractConfig {
            pub url: String,
            pub chain_id: U256,
            $($field: Address,)+
        }

        impl<Context, Provider> Contracts<Context, Provider> {
            pub fn new(opts: ContractConfig, context: Context) -> Self where Context: solidity_bindgen::Context<Provider=Provider> {
                Self {
                    $($field: <$T<Provider>>::new(opts.$field, &context),)+
                    context,
                    chain_id: opts.chain_id,
                }
            }

            $(
                pub fn $field(&self) -> &$T<Provider> {
                    &self.$field
                }
            )+

            pub fn chain_id(&self) -> &U256 {
                &self.chain_id
            }
        }
    }
}

contracts! {
    gns: GNS,
    dispute_manager: DisputeManager,
    epoch_manager: EpochManager,
    rewards_manager: RewardsManager,
    curation: Curation,
    graph_token: GraphToken,
    service_registry: ServiceRegistry,
    staking: Staking
}
