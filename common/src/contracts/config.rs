use super::ContractConfig;
use crate::prelude::*;
use anyhow::anyhow;
use eip_712_derive::{
    chain_id::{ARBITRUM_GOERLI, ARBITRUM_ONE, GOERLI, MAIN_NET},
    U256,
};
use serde::{Deserialize, Deserializer};
use serde_json;
use std::{collections::BTreeMap, fs::File, path::Path, str::FromStr};
use web3::types::H160;

/// This impl contains the addresses for contracts belonging to the Network
/// as deployed on various networks.
impl ContractConfig {
    pub fn from_file(path: &Path, chain_id: u64, url: &str) -> Result<Self, Error> {
        let address_book: AddressBook = serde_json::from_reader(File::open(path)?)?;
        let addresses = address_book
            .0
            .get(&chain_id.to_string())
            .ok_or_else(|| anyhow!("{} not found in address book", chain_id))?;
        let mut chain_id_bytes = [0u8; 32];
        chain_id_bytes[24..].clone_from_slice(&chain_id.to_be_bytes());
        Ok(Self {
            url: url.into(),
            chain_id: U256(chain_id_bytes),
            graph_token: addresses
                .graph_token
                .as_ref()
                .ok_or_else(|| anyhow!("GraphToken contract missing from address book"))?
                .address,
            epoch_manager: addresses.epoch_manager.address,
            dispute_manager: addresses.dispute_manager.address,
            staking: addresses.staking.address,
            curation: addresses.curation.address,
            rewards_manager: addresses.rewards_manager.address,
            service_registry: addresses.service_registry.address,
            gns: addresses.gns.address,
        })
    }

    pub fn mainnet(url: &str) -> Self {
        Self {
            url: url.into(),
            graph_token: "c944E90C64B2c07662A292be6244BDf05Cda44a7".parse().unwrap(),
            epoch_manager: "64F990Bf16552A693dCB043BB7bf3866c5E05DdB".parse().unwrap(),
            dispute_manager: "97307b963662cCA2f7eD50e38dCC555dfFc4FB0b".parse().unwrap(),
            staking: "F55041E37E12cD407ad00CE2910B8269B01263b9".parse().unwrap(),
            curation: "8FE00a685Bcb3B2cc296ff6FfEaB10acA4CE1538".parse().unwrap(),
            rewards_manager: "9Ac758AB77733b4150A901ebd659cbF8cB93ED66".parse().unwrap(),
            service_registry: "aD0C9DaCf1e515615b0581c8D7E295E296Ec26E6".parse().unwrap(),
            gns: "aDcA0dd4729c8BA3aCf3E99F3A9f471EF37b6825".parse().unwrap(),
            chain_id: MAIN_NET,
        }
    }

    pub fn ganache(chain_id: U256) -> Self {
        Self {
            url: "http://127.0.0.1:8545".into(),
            graph_token: "CfEB869F69431e42cdB54A4F4f105C19C080A601".parse().unwrap(),
            epoch_manager: "254dffcd3277C0b1660F6d42EFbB754edaBAbC2B".parse().unwrap(),
            dispute_manager: "0290FB167208Af455bB137780163b7B7a9a10C16".parse().unwrap(),
            staking: "e982E462b094850F12AF94d21D470e21bE9D0E9C".parse().unwrap(),
            curation: "C89Ce4735882C9F0f0FE26686c53074E09B0D550".parse().unwrap(),
            rewards_manager: "59d3631c86BbE35EF041872d502F218A39FBa150".parse().unwrap(),
            service_registry: "9b1f7F645351AF3631a656421eD2e40f2802E6c0".parse().unwrap(),
            gns: "67B5656d60a809915323Bf2C40A8bEF15A152e3e".parse().unwrap(),
            chain_id,
        }
    }

    pub fn goerli(url: &str) -> Self {
        Self {
            url: url.into(),
            graph_token: "5c946740441C12510a167B447B7dE565C20b9E3C".parse().unwrap(),
            epoch_manager: "03541c5cd35953CD447261122F93A5E7b812D697".parse().unwrap(),
            dispute_manager: "8c344366D9269174F10bB588F16945eb47f78dc9".parse().unwrap(),
            staking: "35e3Cb6B317690d662160d5d02A5b364578F62c9".parse().unwrap(),
            curation: "E59B4820dDE28D2c235Bd9A73aA4e8716Cb93E9B".parse().unwrap(),
            rewards_manager: "1246D7c4c903fDd6147d581010BD194102aD4ee2".parse().unwrap(),
            service_registry: "7CF8aD279E9F26b7DAD2Be452A74068536C8231F".parse().unwrap(),
            gns: "065611D3515325aE6fe14f09AEe5Aa2C0a1f0CA7".parse().unwrap(),
            chain_id: GOERLI,
        }
    }

    pub fn arbitrum_goerli(url: &str) -> Self {
        Self {
            url: url.into(),
            graph_token: "18C924BD5E8b83b47EFaDD632b7178E2Fd36073D".parse().unwrap(),
            epoch_manager: "8ECedc7631f4616D7f4074f9fC9D0368674794BE".parse().unwrap(),
            dispute_manager: "16DEF7E0108A5467A106dbD7537f8591f470342E".parse().unwrap(),
            staking: "cd549d0C43d915aEB21d3a331dEaB9B7aF186D26".parse().unwrap(),
            curation: "7080AAcC4ADF4b1E72615D6eb24CDdE40a04f6Ca".parse().unwrap(),
            rewards_manager: "5F06ABd1CfAcF7AE99530D7Fed60E085f0B15e8D".parse().unwrap(),
            service_registry: "07ECDD4278D83Cd2425cA86256634f666b659e53".parse().unwrap(),
            gns: "6bf9104e054537301cC23A1023Ca30A6Df79eB21".parse().unwrap(),
            chain_id: ARBITRUM_GOERLI,
        }
    }

    pub fn arbitrum_one(url: &str) -> Self {
        Self {
            url: url.into(),
            graph_token: "9623063377AD1B27544C965cCd7342f7EA7e88C7".parse().unwrap(),
            epoch_manager: "5A843145c43d328B9bB7a4401d94918f131bB281".parse().unwrap(),
            dispute_manager: "0Ab2B043138352413Bb02e67E626a70320E3BD46".parse().unwrap(),
            staking: "00669A4CF01450B64E8A2A20E9b1FCB71E61eF03".parse().unwrap(),
            curation: "22d78fb4bc72e191C765807f8891B5e1785C8014".parse().unwrap(),
            rewards_manager: "971B9d3d0Ae3ECa029CAB5eA1fB0F72c85e6a525".parse().unwrap(),
            service_registry: "072884c745c0A23144753335776c99BE22588f8A".parse().unwrap(),
            gns: "ec9A7fb6CbC2E41926127929c2dcE6e9c5D33Bec".parse().unwrap(),
            chain_id: ARBITRUM_ONE,
        }
    }
}

// The idea behind having this .parse() compatible API is to be able to easily
// migrate to more advanced parsing and specify each field.
impl FromStr for ContractConfig {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self> {
        let parts: Vec<_> = s.splitn(2, ':').collect();

        Ok(match parts.as_slice() {
            ["mainnet", url] => ContractConfig::mainnet(url),
            ["ganache/mainnet"] => ContractConfig::ganache(MAIN_NET),
            ["goerli", url] => ContractConfig::goerli(url),
            ["arbitrum-goerli", url] => ContractConfig::arbitrum_goerli(url),
            ["arbitrum-one", url] => ContractConfig::arbitrum_one(url),
            _ => {
                return Err(anyhow!("Unrecognized format. Expecting: network:url (or just network for \"ganache/mainnet\"). Got: {}", s));
            }
        })
    }
}

#[derive(Deserialize)]
struct AddressBook(BTreeMap<String, AddressBookEntry>);

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct AddressBookEntry {
    graph_token: Option<AddressBookContract>,
    epoch_manager: AddressBookContract,
    dispute_manager: AddressBookContract,
    staking: AddressBookContract,
    curation: AddressBookContract,
    rewards_manager: AddressBookContract,
    service_registry: AddressBookContract,
    #[serde(rename = "GNS")]
    gns: AddressBookContract,
}

#[derive(Deserialize)]
struct AddressBookContract {
    #[serde(deserialize_with = "deserialize_h160")]
    address: H160,
}

fn deserialize_h160<'de, D>(deserializer: D) -> Result<H160, D::Error>
where
    D: Deserializer<'de>,
{
    String::deserialize(deserializer)?
        .parse::<H160>()
        .map_err(serde::de::Error::custom)
}
