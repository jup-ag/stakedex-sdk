use anyhow::Result;
use lazy_static::lazy_static;
use solana_program::{borsh::try_from_slice_unchecked, pubkey::Pubkey, stake_history::Epoch};
use spl_stake_pool::{
    error::StakePoolError,
    state::{StakePool, StakeStatus, ValidatorList},
};
use stakedex_sdk_common::{
    cogent_stake_pool, daopool_stake_pool, jito_stake_pool, jpool_stake_pool, laine_stake_pool,
    mrgn_stake_pool, risklol_stake_pool, solblaze_stake_pool, WithdrawStakeQuote,
    STAKE_ACCOUNT_RENT_EXEMPT_LAMPORTS,
};
use std::collections::HashMap;

mod stakedex_traits;
pub use stakedex_traits::*;

lazy_static! {
    pub static ref SPL_STAKE_POOL_STATE_TO_LABEL: HashMap<Pubkey, &'static str> = {
        let mut m = HashMap::new();
        m.insert(cogent_stake_pool::ID, "Cogent");
        m.insert(daopool_stake_pool::ID, "DaoPool");
        m.insert(jito_stake_pool::ID, "Jito");
        m.insert(jpool_stake_pool::ID, "JPool");
        m.insert(laine_stake_pool::ID, "Laine");
        m.insert(risklol_stake_pool::ID, "Risk.lol");
        m.insert(solblaze_stake_pool::ID, "SolBlaze");
        m.insert(mrgn_stake_pool::ID, "mrgn");
        m
    };
}

#[derive(Clone, Default)]
pub struct SplStakePoolStakedex {
    pub stake_pool_addr: Pubkey,
    pub withdraw_authority_addr: Pubkey,
    pub stake_pool_label: &'static str,
    pub stake_pool: StakePool,
    pub validator_list: ValidatorList,
    pub curr_epoch: Epoch,
}

impl SplStakePoolStakedex {
    pub fn update_stake_pool(&mut self, data: &[u8]) -> Result<()> {
        self.stake_pool = try_from_slice_unchecked::<StakePool>(data)?;
        Ok(())
    }

    pub fn update_validator_list(&mut self, data: &[u8]) -> Result<()> {
        self.validator_list = try_from_slice_unchecked::<ValidatorList>(data)?;
        Ok(())
    }

    pub fn is_updated_this_epoch(&self) -> bool {
        self.stake_pool.last_update_epoch >= self.curr_epoch
    }

    fn get_quote_for_validator_copied(
        &self,
        validator_index: usize,
        withdraw_amount: u64,
    ) -> Result<WithdrawStakeQuote, StakePoolError> {
        let validator_list_entry = self.validator_list.validators.get(validator_index).unwrap();
        // only handle withdrawal from active stake accounts for simplicity.
        // Likely other stake pools can't accept non active stake anyway
        if validator_list_entry.status != StakeStatus::Active {
            return Err(StakePoolError::InvalidState);
        }
        let stake_pool = &self.stake_pool;
        let pool_tokens = withdraw_amount;

        // Copied from:
        // https://github.com/solana-labs/solana-program-library/blob/58c1226a513d3d8bb2de8ec67586a679be7fd2d4/stake-pool/program/src/processor.rs#L2297
        let pool_tokens_fee = stake_pool
            .calc_pool_tokens_stake_withdrawal_fee(pool_tokens)
            .ok_or(StakePoolError::CalculationFailure)?;
        let pool_tokens_burnt = pool_tokens
            .checked_sub(pool_tokens_fee)
            .ok_or(StakePoolError::CalculationFailure)?;

        let withdraw_lamports = stake_pool
            .calc_lamports_withdraw_amount(pool_tokens_burnt)
            .ok_or(StakePoolError::CalculationFailure)?;

        if withdraw_lamports == 0 {
            return Err(StakePoolError::WithdrawalTooSmall);
        }
        // end copy

        // according to https://github.com/solana-labs/solana-program-library/blob/58c1226a513d3d8bb2de8ec67586a679be7fd2d4/stake-pool/program/src/state.rs#L536C1-L542
        // `active_stake_lamports` = delegation.stake - MIN_ACTIVE_STAKE_LAMPORTS.
        // Withdrawals must leave at least MIN_ACTIVE_STAKE_LAMPORTS active stake in vsa
        if withdraw_lamports > validator_list_entry.active_stake_lamports {
            return Err(StakePoolError::InvalidState);
        }
        let lamports_staked = withdraw_lamports
            .checked_sub(STAKE_ACCOUNT_RENT_EXEMPT_LAMPORTS)
            .ok_or(StakePoolError::CalculationFailure)?;
        Ok(WithdrawStakeQuote {
            lamports_out: withdraw_lamports,
            lamports_staked,
            fee_amount: pool_tokens_fee,
            voter: validator_list_entry.vote_account_address,
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::*;
    use stakedex_sdk_common::DepositSolWrapper;

    #[test]
    fn test_wrapper_impls_amm_correctly_compile_time() {
        // DepositSolWrapper<SplStakePoolDepositSol>
        // impls Amm
        let _sp = DepositSolWrapper(SplStakePoolStakedex::default());
    }
}
