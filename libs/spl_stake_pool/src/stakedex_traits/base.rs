use std::num::NonZeroU64;

use anyhow::Result;
use jupiter_amm_interface::{AccountMap, AmmContext, KeyedAccount};
use solana_program::pubkey::Pubkey;
use spl_stake_pool::error::StakePoolError;
use stakedex_sdk_common::{account_missing_err, BaseStakePoolAmm, InitFromKeyedAccount};

use crate::{SplStakePoolStakedex, SplStakePoolStakedexWithWithdrawSol};

impl InitFromKeyedAccount for SplStakePoolStakedex {
    /// Initialize from stake pool main account
    fn from_keyed_account(
        KeyedAccount {
            key,
            account,
            params,
        }: &KeyedAccount,
        amm_context: &AmmContext,
    ) -> Result<Self> {
        let mut res = Self::new_uninitialized(
            crate::SplStakePoolStakedexInitKeys {
                stake_pool_program: account.owner,
                stake_pool_addr: *key,
            },
            amm_context.clock_ref.epoch.clone(),
        );

        res.update_stake_pool(&account.data)?;

        res.stake_pool_label = params
            .as_ref()
            .map_or_else(|| None, |v| v.as_str())
            .map_or_else(
                || format!("{} stake pool", res.stake_pool.pool_mint),
                |token_name| format!("{token_name} stake pool"),
            );
        // NOTE: the validator_list is not initialized until self.update() is
        // called for the first time with fetched on-chain data
        Ok(res)
    }
}

impl BaseStakePoolAmm for SplStakePoolStakedex {
    #[inline]
    fn program_id(&self) -> Pubkey {
        self.stake_pool_program
    }

    #[inline]
    fn stake_pool_label(&self) -> &str {
        &self.stake_pool_label
    }

    #[inline]
    fn main_state_key(&self) -> Pubkey {
        self.stake_pool_addr
    }

    #[inline]
    fn staked_sol_mint(&self) -> Pubkey {
        self.stake_pool.pool_mint
    }

    #[inline]
    fn get_accounts_to_update(&self) -> Vec<Pubkey> {
        let mut res = Vec::from([self.stake_pool_addr, self.stake_pool.validator_list]);
        if self.is_sol_deposit_capped() || self.is_stake_deposit_capped() {
            res.push(self.spl_deposit_cap_guard_program_address);
        }
        res
    }

    fn update(&mut self, accounts_map: &AccountMap) -> Result<()> {
        let stake_pool_data = accounts_map
            .get(&self.stake_pool_addr)
            .ok_or_else(|| account_missing_err(&self.stake_pool_addr))?
            .data
            .as_ref();
        self.update_stake_pool(stake_pool_data)?;
        let validator_list_data = accounts_map
            .get(&self.stake_pool.validator_list)
            .ok_or_else(|| account_missing_err(&self.stake_pool.validator_list))?
            .data
            .as_ref();
        self.update_validator_list(validator_list_data)?;
        if self.is_sol_deposit_capped() || self.is_stake_deposit_capped() {
            let deposit_cap_data = accounts_map
                .get(&self.spl_deposit_cap_guard_program_address)
                .ok_or_else(|| account_missing_err(&self.spl_deposit_cap_guard_program_address))?
                .data
                .as_ref();
            self.update_deposit_cap_state(deposit_cap_data)?;
        }
        Ok(())
    }
}

impl InitFromKeyedAccount for SplStakePoolStakedexWithWithdrawSol {
    #[inline]
    fn from_keyed_account(keyed_account: &KeyedAccount, amm_context: &AmmContext) -> Result<Self> {
        Ok(Self {
            inner: SplStakePoolStakedex::from_keyed_account(keyed_account, amm_context)?,
            reserve_stake_lamports: None,
        })
    }
}

impl BaseStakePoolAmm for SplStakePoolStakedexWithWithdrawSol {
    #[inline]
    fn program_id(&self) -> Pubkey {
        self.inner.program_id()
    }

    #[inline]
    fn stake_pool_label(&self) -> &str {
        self.inner.stake_pool_label()
    }

    #[inline]
    fn main_state_key(&self) -> Pubkey {
        self.inner.main_state_key()
    }

    #[inline]
    fn staked_sol_mint(&self) -> Pubkey {
        self.inner.staked_sol_mint()
    }

    #[inline]
    fn get_accounts_to_update(&self) -> Vec<Pubkey> {
        let mut res = self.inner.get_accounts_to_update();
        res.push(self.inner.stake_pool.reserve_stake);
        res
    }

    fn update(&mut self, account_map: &AccountMap) -> Result<()> {
        self.inner.update(account_map)?;
        let reserve_stake = account_map
            .get(&self.inner.stake_pool.reserve_stake)
            .ok_or_else(|| account_missing_err(&self.inner.stake_pool.reserve_stake))?;
        let reserve_stake_lamports =
            NonZeroU64::new(reserve_stake.lamports).ok_or(StakePoolError::WrongStakeStake)?;
        self.reserve_stake_lamports = Some(reserve_stake_lamports);
        Ok(())
    }
}
