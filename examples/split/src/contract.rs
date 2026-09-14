//! Soroban payment splitting and revenue-sharing contract.
//!
//! Inspired by [stellar-split/split-contracts](https://github.com/stellar-split/split-contracts)
//! with basis-point proportional splits, exact dust tracking, and pull claims.

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, token, Address, Env, Vec,
};

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum SplitError {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    Unauthorized = 3,
    InvalidShares = 4,
    InvalidAmount = 5,
    NothingToClaim = 6,
    BatchNotFound = 7,
    AlreadyDistributed = 8,
    Overflow = 9,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecipientShare {
    pub recipient: Address,
    pub share_bps: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SplitBatch {
    pub id: u64,
    pub total_amount: i128,
    pub distributed_amount: i128,
    pub unallocated_dust: i128,
    pub is_distributed: bool,
}

#[contracttype]
pub enum DataKey {
    Admin,
    Token,
    BatchCounter,
    TotalDeposited,
    TotalDistributed,
    TotalDust,
    Recipients,
    Unclaimed(Address),
    TotalClaimed(Address),
    TotalAllocated(Address),
    Batch(u64),
}

#[contract]
pub struct SplitContract;

#[contractimpl]
impl SplitContract {
    pub fn initialize(
        env: Env,
        admin: Address,
        token: Address,
        recipients: Vec<RecipientShare>,
    ) -> Result<(), SplitError> {
        if env.storage().instance().has(&DataKey::Admin) {
            return Err(SplitError::AlreadyInitialized);
        }

        Self::validate_shares(&recipients)?;

        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::Token, &token);
        env.storage().instance().set(&DataKey::BatchCounter, &0u64);
        env.storage().instance().set(&DataKey::TotalDeposited, &0i128);
        env.storage().instance().set(&DataKey::TotalDistributed, &0i128);
        env.storage().instance().set(&DataKey::TotalDust, &0i128);
        env.storage().instance().set(&DataKey::Recipients, &recipients);

        Ok(())
    }

    pub fn update_shares(
        env: Env,
        admin: Address,
        new_recipients: Vec<RecipientShare>,
    ) -> Result<(), SplitError> {
        admin.require_auth();

        let current_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(SplitError::NotInitialized)?;
        if admin != current_admin {
            return Err(SplitError::Unauthorized);
        }

        Self::validate_shares(&new_recipients)?;

        env.storage()
            .instance()
            .set(&DataKey::Recipients, &new_recipients);

        Ok(())
    }

    pub fn deposit_and_split(
        env: Env,
        sender: Address,
        amount: i128,
    ) -> Result<u64, SplitError> {
        sender.require_auth();

        if amount <= 0 {
            return Err(SplitError::InvalidAmount);
        }

        let token_addr = Self::get_token(env.clone())?;
        let token_client = token::Client::new(&env, &token_addr);

        // Pull tokens from sender into contract
        token_client.transfer(&sender, &env.current_contract_address(), &amount);

        let recipients: Vec<RecipientShare> = env
            .storage()
            .instance()
            .get(&DataKey::Recipients)
            .ok_or(SplitError::NotInitialized)?;

        let mut distributed: i128 = 0;

        for r in recipients.iter() {
            let alloc = amount
                .checked_mul(r.share_bps as i128)
                .ok_or(SplitError::Overflow)?
                / 10_000;

            distributed = distributed
                .checked_add(alloc)
                .ok_or(SplitError::Overflow)?;

            let curr_unclaimed: i128 = env
                .storage()
                .persistent()
                .get(&DataKey::Unclaimed(r.recipient.clone()))
                .unwrap_or(0);
            let new_unclaimed = curr_unclaimed
                .checked_add(alloc)
                .ok_or(SplitError::Overflow)?;
            env.storage()
                .persistent()
                .set(&DataKey::Unclaimed(r.recipient.clone()), &new_unclaimed);

            let curr_alloc: i128 = env
                .storage()
                .persistent()
                .get(&DataKey::TotalAllocated(r.recipient.clone()))
                .unwrap_or(0);
            let new_alloc = curr_alloc
                .checked_add(alloc)
                .ok_or(SplitError::Overflow)?;
            env.storage()
                .persistent()
                .set(&DataKey::TotalAllocated(r.recipient.clone()), &new_alloc);
        }

        let dust = amount
            .checked_sub(distributed)
            .ok_or(SplitError::Overflow)?;

        let counter: u64 = env
            .storage()
            .instance()
            .get(&DataKey::BatchCounter)
            .unwrap_or(0);
        let id = counter.checked_add(1).ok_or(SplitError::Overflow)?;
        env.storage().instance().set(&DataKey::BatchCounter, &id);

        let batch = SplitBatch {
            id,
            total_amount: amount,
            distributed_amount: distributed,
            unallocated_dust: dust,
            is_distributed: true,
        };
        env.storage().persistent().set(&DataKey::Batch(id), &batch);

        let total_dep: i128 = env
            .storage()
            .instance()
            .get(&DataKey::TotalDeposited)
            .unwrap_or(0);
        let new_dep = total_dep
            .checked_add(amount)
            .ok_or(SplitError::Overflow)?;
        env.storage()
            .instance()
            .set(&DataKey::TotalDeposited, &new_dep);

        let total_dust: i128 = env
            .storage()
            .instance()
            .get(&DataKey::TotalDust)
            .unwrap_or(0);
        let new_dust = total_dust
            .checked_add(dust)
            .ok_or(SplitError::Overflow)?;
        env.storage().instance().set(&DataKey::TotalDust, &new_dust);

        Ok(id)
    }

    pub fn claim(env: Env, recipient: Address) -> Result<i128, SplitError> {
        recipient.require_auth();

        let unclaimed: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::Unclaimed(recipient.clone()))
            .unwrap_or(0);

        if unclaimed <= 0 {
            return Err(SplitError::NothingToClaim);
        }

        env.storage()
            .persistent()
            .set(&DataKey::Unclaimed(recipient.clone()), &0i128);

        let curr_claimed: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::TotalClaimed(recipient.clone()))
            .unwrap_or(0);
        let new_claimed = curr_claimed
            .checked_add(unclaimed)
            .ok_or(SplitError::Overflow)?;
        env.storage()
            .persistent()
            .set(&DataKey::TotalClaimed(recipient.clone()), &new_claimed);

        let total_dist: i128 = env
            .storage()
            .instance()
            .get(&DataKey::TotalDistributed)
            .unwrap_or(0);
        let new_dist = total_dist
            .checked_add(unclaimed)
            .ok_or(SplitError::Overflow)?;
        env.storage()
            .instance()
            .set(&DataKey::TotalDistributed, &new_dist);

        let token_addr = Self::get_token(env.clone())?;
        let token_client = token::Client::new(&env, &token_addr);
        token_client.transfer(&env.current_contract_address(), &recipient, &unclaimed);

        Ok(unclaimed)
    }

    pub fn get_recipients(env: Env) -> Result<Vec<RecipientShare>, SplitError> {
        env.storage()
            .instance()
            .get(&DataKey::Recipients)
            .ok_or(SplitError::NotInitialized)
    }

    pub fn get_unclaimed_balance(env: Env, recipient: Address) -> i128 {
        env.storage()
            .persistent()
            .get(&DataKey::Unclaimed(recipient))
            .unwrap_or(0)
    }

    pub fn get_total_allocated(env: Env, recipient: Address) -> i128 {
        env.storage()
            .persistent()
            .get(&DataKey::TotalAllocated(recipient))
            .unwrap_or(0)
    }

    pub fn get_total_claimed(env: Env, recipient: Address) -> i128 {
        env.storage()
            .persistent()
            .get(&DataKey::TotalClaimed(recipient))
            .unwrap_or(0)
    }

    pub fn get_total_deposited(env: Env) -> i128 {
        env.storage()
            .instance()
            .get(&DataKey::TotalDeposited)
            .unwrap_or(0)
    }

    pub fn get_total_distributed(env: Env) -> i128 {
        env.storage()
            .instance()
            .get(&DataKey::TotalDistributed)
            .unwrap_or(0)
    }

    pub fn get_total_dust(env: Env) -> i128 {
        env.storage()
            .instance()
            .get(&DataKey::TotalDust)
            .unwrap_or(0)
    }

    pub fn get_batch(env: Env, id: u64) -> Result<SplitBatch, SplitError> {
        env.storage()
            .persistent()
            .get(&DataKey::Batch(id))
            .ok_or(SplitError::BatchNotFound)
    }

    pub fn batch_counter(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::BatchCounter)
            .unwrap_or(0)
    }

    pub fn get_token(env: Env) -> Result<Address, SplitError> {
        env.storage()
            .instance()
            .get(&DataKey::Token)
            .ok_or(SplitError::NotInitialized)
    }

    pub fn get_admin(env: Env) -> Result<Address, SplitError> {
        env.storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(SplitError::NotInitialized)
    }

    fn validate_shares(recipients: &Vec<RecipientShare>) -> Result<(), SplitError> {
        if recipients.is_empty() {
            return Err(SplitError::InvalidShares);
        }

        let mut sum_bps: u32 = 0;
        for r in recipients.iter() {
            if r.share_bps == 0 || r.share_bps > 10_000 {
                return Err(SplitError::InvalidShares);
            }
            sum_bps = sum_bps
                .checked_add(r.share_bps)
                .ok_or(SplitError::Overflow)?;
        }

        if sum_bps != 10_000 {
            return Err(SplitError::InvalidShares);
        }

        Ok(())
    }
}
