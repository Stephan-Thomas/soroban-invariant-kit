//! Soroban payment streaming contract based on StreamPay.
//!
//! Adapted from [StreamPay-Organization/StreamPay-Contracts](https://github.com/StreamPay-Organization/StreamPay-Contracts)
//! for invariant fuzz testing and formal validation.

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, token, Address, Env,
};

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    Unauthorized = 3,
    InvalidAmount = 4,
    InvalidTimeRange = 5,
    StreamNotFound = 6,
    StreamNotActive = 7,
    NothingToWithdraw = 8,
    AlreadyCancelled = 9,
    AlreadyCompleted = 10,
    Overflow = 11,
}

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Status {
    Active = 0,
    Cancelled = 1,
    Completed = 2,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Stream {
    pub sender: Address,
    pub recipient: Address,
    pub total: i128,
    pub withdrawn: i128,
    pub refunded: i128,
    pub start: u64,
    pub end: u64,
    pub status: Status,
    pub accrued: i128,
    pub accrued_at: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StreamSummary {
    pub total: i128,
    pub vested: i128,
    pub withdrawn: i128,
    pub withdrawable: i128,
    pub progress_bps: u32,
    pub status: Status,
}

#[contracttype]
pub enum DataKey {
    Admin,
    Token,
    Counter,
    Stream(u64),
    TotalDeposited,
    TotalRefunded,
}

// ─────────────────────────────────────────────────────────────────────────────
// Vesting Math
// ─────────────────────────────────────────────────────────────────────────────

pub fn linear_segment(
    segment_start: u64,
    end: u64,
    remaining: i128,
    now: u64,
) -> Result<i128, Error> {
    if end <= segment_start {
        return Ok(remaining);
    }
    if now <= segment_start {
        return Ok(0);
    }
    if now >= end {
        return Ok(remaining);
    }

    let elapsed = (now - segment_start) as i128;
    let duration = (end - segment_start) as i128;
    let numerator = remaining.checked_mul(elapsed).ok_or(Error::Overflow)?;
    Ok(numerator / duration)
}

pub fn compute_vested(stream: &Stream, now: u64) -> Result<i128, Error> {
    let remaining = stream
        .total
        .checked_sub(stream.accrued)
        .ok_or(Error::Overflow)?;
    let segment = linear_segment(stream.accrued_at, stream.end, remaining, now)?;
    let total_vested = stream.accrued.checked_add(segment).ok_or(Error::Overflow)?;
    Ok(total_vested.clamp(0, stream.total))
}

pub fn compute_unvested(stream: &Stream, now: u64) -> Result<i128, Error> {
    let v = compute_vested(stream, now)?;
    stream.total.checked_sub(v).ok_or(Error::Overflow)
}

pub fn compute_withdrawable(stream: &Stream, now: u64) -> Result<i128, Error> {
    let v = compute_vested(stream, now)?;
    Ok((v - stream.withdrawn).max(0))
}

pub fn compute_progress_bps(stream: &Stream, now: u64) -> u32 {
    if now <= stream.start {
        return 0;
    }
    if now >= stream.end {
        return 10_000;
    }
    let elapsed = (now - stream.start) as u128;
    let duration = (stream.end - stream.start) as u128;
    (elapsed * 10_000 / duration) as u32
}

pub fn advance_checkpoint(stream: &mut Stream, now: u64) -> Result<(), Error> {
    let v = compute_vested(stream, now)?;
    stream.accrued = v;
    stream.accrued_at = now;
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// Contract Implementation
// ─────────────────────────────────────────────────────────────────────────────

#[contract]
pub struct StreamPayContract;

#[contractimpl]
impl StreamPayContract {
    pub fn initialize(env: Env, admin: Address, token: Address) -> Result<(), Error> {
        if env.storage().instance().has(&DataKey::Admin) {
            return Err(Error::AlreadyInitialized);
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::Token, &token);
        env.storage().instance().set(&DataKey::Counter, &0u64);
        env.storage().instance().set(&DataKey::TotalDeposited, &0i128);
        env.storage().instance().set(&DataKey::TotalRefunded, &0i128);
        Ok(())
    }

    pub fn get_admin(env: Env) -> Result<Address, Error> {
        env.storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::NotInitialized)
    }

    pub fn get_token(env: Env) -> Result<Address, Error> {
        env.storage()
            .instance()
            .get(&DataKey::Token)
            .ok_or(Error::NotInitialized)
    }

    pub fn stream_counter(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::Counter)
            .unwrap_or(0u64)
    }

    pub fn get_total_deposited(env: Env) -> i128 {
        env.storage()
            .instance()
            .get(&DataKey::TotalDeposited)
            .unwrap_or(0i128)
    }

    pub fn get_total_refunded(env: Env) -> i128 {
        env.storage()
            .instance()
            .get(&DataKey::TotalRefunded)
            .unwrap_or(0i128)
    }

    pub fn get_stream(env: Env, id: u64) -> Result<Stream, Error> {
        env.storage()
            .persistent()
            .get(&DataKey::Stream(id))
            .ok_or(Error::StreamNotFound)
    }

    pub fn get_summary(env: Env, id: u64) -> Result<StreamSummary, Error> {
        let stream = Self::get_stream(env.clone(), id)?;
        let now = env.ledger().timestamp();
        let vested = compute_vested(&stream, now)?;
        let withdrawable = compute_withdrawable(&stream, now)?;
        let progress_bps = compute_progress_bps(&stream, now);

        Ok(StreamSummary {
            total: stream.total,
            vested,
            withdrawn: stream.withdrawn,
            withdrawable,
            progress_bps,
            status: stream.status,
        })
    }

    pub fn withdrawable_amount(env: Env, id: u64) -> Result<i128, Error> {
        let stream = Self::get_stream(env.clone(), id)?;
        compute_withdrawable(&stream, env.ledger().timestamp())
    }

    pub fn create_stream(
        env: Env,
        sender: Address,
        recipient: Address,
        total_amount: i128,
        start_time: u64,
        end_time: u64,
    ) -> Result<u64, Error> {
        sender.require_auth();

        if total_amount <= 0 {
            return Err(Error::InvalidAmount);
        }
        if end_time <= start_time {
            return Err(Error::InvalidTimeRange);
        }

        let token_addr = Self::get_token(env.clone())?;
        let token_client = token::Client::new(&env, &token_addr);

        // Pull tokens from sender into contract escrow
        token_client.transfer(&sender, &env.current_contract_address(), &total_amount);

        let counter: u64 = env
            .storage()
            .instance()
            .get(&DataKey::Counter)
            .unwrap_or(0);
        let id = counter.checked_add(1).ok_or(Error::Overflow)?;

        let stream = Stream {
            sender: sender.clone(),
            recipient,
            total: total_amount,
            withdrawn: 0,
            refunded: 0,
            start: start_time,
            end: end_time,
            status: Status::Active,
            accrued: 0,
            accrued_at: start_time,
        };

        env.storage().instance().set(&DataKey::Counter, &id);
        env.storage().persistent().set(&DataKey::Stream(id), &stream);

        let total_dep: i128 = env
            .storage()
            .instance()
            .get(&DataKey::TotalDeposited)
            .unwrap_or(0);
        let new_dep = total_dep.checked_add(total_amount).ok_or(Error::Overflow)?;
        env.storage().instance().set(&DataKey::TotalDeposited, &new_dep);

        Ok(id)
    }

    pub fn withdraw(env: Env, id: u64, recipient: Address) -> Result<i128, Error> {
        recipient.require_auth();

        let mut stream = Self::get_stream(env.clone(), id)?;
        if stream.recipient != recipient {
            return Err(Error::Unauthorized);
        }
        if stream.status == Status::Cancelled {
            return Err(Error::AlreadyCancelled);
        }
        if stream.status == Status::Completed {
            return Err(Error::AlreadyCompleted);
        }

        let now = env.ledger().timestamp();
        let available = compute_withdrawable(&stream, now)?;
        if available <= 0 {
            return Err(Error::NothingToWithdraw);
        }

        stream.withdrawn = stream
            .withdrawn
            .checked_add(available)
            .ok_or(Error::Overflow)?;

        if stream.withdrawn >= stream.total {
            stream.status = Status::Completed;
        }

        env.storage().persistent().set(&DataKey::Stream(id), &stream);

        let token_addr = Self::get_token(env.clone())?;
        let token_client = token::Client::new(&env, &token_addr);
        token_client.transfer(&env.current_contract_address(), &recipient, &available);

        Ok(available)
    }

    pub fn cancel(env: Env, id: u64, caller: Address) -> Result<(), Error> {
        caller.require_auth();

        let mut stream = Self::get_stream(env.clone(), id)?;
        if caller != stream.sender && caller != stream.recipient {
            return Err(Error::Unauthorized);
        }
        if stream.status == Status::Cancelled {
            return Err(Error::AlreadyCancelled);
        }
        if stream.status == Status::Completed {
            return Err(Error::AlreadyCompleted);
        }

        let now = env.ledger().timestamp();
        let vested = compute_vested(&stream, now)?;

        let recipient_paid = vested
            .checked_sub(stream.withdrawn)
            .ok_or(Error::Overflow)?;
        let sender_refund = compute_unvested(&stream, now)?;

        stream.withdrawn = vested;
        stream.refunded = sender_refund;
        stream.status = Status::Cancelled;
        env.storage().persistent().set(&DataKey::Stream(id), &stream);

        let total_ref: i128 = env
            .storage()
            .instance()
            .get(&DataKey::TotalRefunded)
            .unwrap_or(0);
        let new_ref = total_ref.checked_add(sender_refund).ok_or(Error::Overflow)?;
        env.storage().instance().set(&DataKey::TotalRefunded, &new_ref);

        let token_addr = Self::get_token(env.clone())?;
        let token_client = token::Client::new(&env, &token_addr);

        if recipient_paid > 0 {
            token_client.transfer(
                &env.current_contract_address(),
                &stream.recipient,
                &recipient_paid,
            );
        }
        if sender_refund > 0 {
            token_client.transfer(
                &env.current_contract_address(),
                &stream.sender,
                &sender_refund,
            );
        }

        Ok(())
    }

    pub fn top_up(env: Env, id: u64, sender: Address, amount: i128) -> Result<i128, Error> {
        sender.require_auth();

        if amount <= 0 {
            return Err(Error::InvalidAmount);
        }

        let mut stream = Self::get_stream(env.clone(), id)?;
        if sender != stream.sender {
            return Err(Error::Unauthorized);
        }
        if stream.status != Status::Active {
            return Err(Error::StreamNotActive);
        }

        let new_total = stream.total.checked_add(amount).ok_or(Error::Overflow)?;
        stream.total = new_total;
        env.storage().persistent().set(&DataKey::Stream(id), &stream);

        let total_dep: i128 = env
            .storage()
            .instance()
            .get(&DataKey::TotalDeposited)
            .unwrap_or(0);
        let new_dep = total_dep.checked_add(amount).ok_or(Error::Overflow)?;
        env.storage().instance().set(&DataKey::TotalDeposited, &new_dep);

        let token_addr = Self::get_token(env.clone())?;
        let token_client = token::Client::new(&env, &token_addr);
        token_client.transfer(&sender, &env.current_contract_address(), &amount);

        Ok(new_total)
    }

    pub fn extend_stream(env: Env, id: u64, sender: Address, new_end: u64) -> Result<(), Error> {
        sender.require_auth();

        let mut stream = Self::get_stream(env.clone(), id)?;
        if sender != stream.sender {
            return Err(Error::Unauthorized);
        }
        if stream.status != Status::Active {
            return Err(Error::StreamNotActive);
        }
        if new_end <= stream.end {
            return Err(Error::InvalidTimeRange);
        }

        advance_checkpoint(&mut stream, env.ledger().timestamp())?;
        stream.end = new_end;
        env.storage().persistent().set(&DataKey::Stream(id), &stream);

        Ok(())
    }
}
