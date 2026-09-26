// contracts/dao/src/lib.rs
// Implements DAO Governance Contract — closes #615
//
// Acceptance Criteria:
// - Implement voting mechanism (1 token = 1 vote)
// - Support proposal creation and discussion
// - Add timelock for execution
// - Implement multi-sig validation
// - Track governance history

#![no_std]

use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short, token, Address, Env, String, Vec,
};

// ── Constants ─────────────────────────────────────────────────────────────────

/// Default timelock delay in ledgers (~5 seconds/ledger → 1 day ≈ 17280 ledgers).
const DEFAULT_TIMELOCK_LEDGERS: u32 = 17_280;

/// Minimum quorum required for a proposal to pass (numerator out of 10 000).
const QUORUM_BPS: u32 = 1_000; // 10 %

// ── Data Types ────────────────────────────────────────────────────────────────

/// Lifecycle state of a governance proposal.
#[contracttype]
#[derive(Clone, PartialEq)]
pub enum ProposalStatus {
    /// Voting is open.
    Active,
    /// Voting closed with enough votes to pass; waiting for timelock.
    Queued,
    /// Timelock expired; ready for execution.
    Ready,
    /// Proposal has been executed.
    Executed,
    /// Proposal was defeated or cancelled.
    Defeated,
    /// Cancelled by admin or proposer.
    Cancelled,
}

/// On-chain governance proposal record.
#[contracttype]
#[derive(Clone)]
pub struct Proposal {
    pub id: u64,
    pub title: String,
    pub description: String,
    pub proposer: Address,
    pub votes_for: i128,
    pub votes_against: i128,
    pub status: ProposalStatus,
    /// Ledger when voting opens.
    pub voting_start: u32,
    /// Ledger when voting closes.
    pub voting_end: u32,
    /// Ledger after which the proposal may be executed (0 until queued).
    pub executable_after: u32,
    /// Whether the proposal requires multi-sig approval before execution.
    pub requires_multisig: bool,
    /// Number of multi-sig approvals collected.
    pub multisig_approvals: u32,
}

/// A record of a vote cast on a proposal.
#[contracttype]
#[derive(Clone)]
pub struct VoteRecord {
    pub proposal_id: u64,
    pub voter: Address,
    /// Weight = token balance at time of vote.
    pub weight: i128,
    pub support: bool,
    pub ledger: u32,
}

/// A governance history entry emitted after each state transition.
#[contracttype]
#[derive(Clone)]
pub struct GovernanceEvent {
    pub proposal_id: u64,
    pub action: String,
    pub actor: Address,
    pub ledger: u32,
}

// ── Storage Keys ──────────────────────────────────────────────────────────────

#[contracttype]
pub enum DataKey {
    /// Proposal record.
    Proposal(u64),
    /// Proposal id counter.
    ProposalCount,
    /// Whether a specific address has voted on a proposal.
    HasVoted(u64, Address),
    /// Vote record for a voter on a proposal.
    Vote(u64, Address),
    /// Multi-sig approval: (proposal_id, signer).
    MultiSigApproval(u64, Address),
    /// Governance history event count.
    HistoryCount,
    /// Individual history entry.
    History(u64),
    /// Governance token contract address.
    GovToken,
    /// Admin address.
    Admin,
    /// Required multi-sig signers list.
    MultiSigSigners,
    /// Required number of multi-sig approvals.
    MultiSigThreshold,
    /// Timelock delay in ledgers.
    TimelockLedgers,
    /// Voting period duration in ledgers.
    VotingPeriodLedgers,
}

// ── Contract ──────────────────────────────────────────────────────────────────

#[contract]
pub struct DaoGovernance;

#[contractimpl]
impl DaoGovernance {
    // ── Admin Initialisation ──────────────────────────────────────────────

    /// Initialise the DAO governance contract.
    ///
    /// # Arguments
    /// * `admin`              — initial admin address
    /// * `gov_token`          — governance token contract (1 token = 1 vote)
    /// * `multisig_signers`   — list of addresses required for multi-sig
    /// * `multisig_threshold` — minimum approvals required (≤ len(signers))
    /// * `timelock_ledgers`   — execution delay after passing (0 = default 17280)
    /// * `voting_period`      — number of ledgers a proposal is open for voting
    pub fn initialize(
        env: Env,
        admin: Address,
        gov_token: Address,
        multisig_signers: Vec<Address>,
        multisig_threshold: u32,
        timelock_ledgers: u32,
        voting_period: u32,
    ) {
        admin.require_auth();
        assert!(
            multisig_threshold <= multisig_signers.len(),
            "threshold cannot exceed signer count"
        );
        assert!(voting_period > 0, "voting period must be > 0");

        env.storage().persistent().set(&DataKey::Admin, &admin);
        env.storage().persistent().set(&DataKey::GovToken, &gov_token);
        env.storage().persistent().set(&DataKey::MultiSigSigners, &multisig_signers);
        env.storage().persistent().set(&DataKey::MultiSigThreshold, &multisig_threshold);
        let tl = if timelock_ledgers == 0 { DEFAULT_TIMELOCK_LEDGERS } else { timelock_ledgers };
        env.storage().persistent().set(&DataKey::TimelockLedgers, &tl);
        env.storage().persistent().set(&DataKey::VotingPeriodLedgers, &voting_period);
        env.storage().persistent().set(&DataKey::ProposalCount, &0u64);
        env.storage().persistent().set(&DataKey::HistoryCount, &0u64);

        env.events().publish((symbol_short!("init"),), admin);
    }

    // ── Proposal Management ───────────────────────────────────────────────

    /// Create a new governance proposal.
    ///
    /// Any token holder may propose.  The proposer's token balance is checked;
    /// they need at least 1 token unit to create a proposal.
    ///
    /// # Arguments
    /// * `proposer`         — address creating the proposal. Must sign.
    /// * `title`            — short proposal title.
    /// * `description`      — full proposal text / discussion points.
    /// * `requires_multisig`— whether execution needs multi-sig approval.
    ///
    /// Returns the new `proposal_id`.
    pub fn create_proposal(
        env: Env,
        proposer: Address,
        title: String,
        description: String,
        requires_multisig: bool,
    ) -> u64 {
        proposer.require_auth();

        let gov_token: Address = env
            .storage()
            .persistent()
            .get(&DataKey::GovToken)
            .expect("not initialized");

        let tok = token::Client::new(&env, &gov_token);
        let balance = tok.balance(&proposer);
        assert!(balance >= 1, "proposer must hold at least 1 governance token");

        let voting_period: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::VotingPeriodLedgers)
            .expect("not initialized");

        let now = env.ledger().sequence();
        let id = Self::next_proposal_id(&env);

        let proposal = Proposal {
            id,
            title: title.clone(),
            description,
            proposer: proposer.clone(),
            votes_for: 0,
            votes_against: 0,
            status: ProposalStatus::Active,
            voting_start: now,
            voting_end: now + voting_period,
            executable_after: 0,
            requires_multisig,
            multisig_approvals: 0,
        };

        env.storage().persistent().set(&DataKey::Proposal(id), &proposal);

        Self::record_history(&env, id, String::from_str(&env, "created"), proposer.clone());

        env.events().publish(
            (symbol_short!("proposed"),),
            (id, proposer, title, requires_multisig),
        );

        id
    }

    /// Cancel a proposal.  Only the proposer or admin may cancel.
    pub fn cancel_proposal(env: Env, caller: Address, proposal_id: u64) {
        caller.require_auth();
        let admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .expect("not initialized");

        let mut proposal = Self::load_proposal(&env, proposal_id);
        assert!(
            caller == proposal.proposer || caller == admin,
            "only proposer or admin can cancel"
        );
        assert!(
            proposal.status == ProposalStatus::Active,
            "only active proposals can be cancelled"
        );

        proposal.status = ProposalStatus::Cancelled;
        env.storage().persistent().set(&DataKey::Proposal(proposal_id), &proposal);

        Self::record_history(&env, proposal_id, String::from_str(&env, "cancelled"), caller.clone());
        env.events().publish((symbol_short!("cancelled"),), (proposal_id, caller));
    }

    // ── Voting ────────────────────────────────────────────────────────────

    /// Cast a vote on an active proposal.
    ///
    /// Vote weight equals the voter's governance token balance at the time of
    /// voting (1 token = 1 vote).  Each address may vote exactly once per
    /// proposal.
    ///
    /// # Arguments
    /// * `voter`      — must sign and hold governance tokens.
    /// * `proposal_id`— id of the proposal to vote on.
    /// * `support`    — true = vote for, false = vote against.
    pub fn cast_vote(env: Env, voter: Address, proposal_id: u64, support: bool) {
        voter.require_auth();

        let has_voted: bool = env
            .storage()
            .persistent()
            .get(&DataKey::HasVoted(proposal_id, voter.clone()))
            .unwrap_or(false);
        assert!(!has_voted, "voter has already voted on this proposal");

        let mut proposal = Self::load_proposal(&env, proposal_id);
        assert!(
            proposal.status == ProposalStatus::Active,
            "proposal is not active"
        );

        let now = env.ledger().sequence();
        assert!(now <= proposal.voting_end, "voting period has ended");

        let gov_token: Address = env
            .storage()
            .persistent()
            .get(&DataKey::GovToken)
            .expect("not initialized");
        let tok = token::Client::new(&env, &gov_token);
        let weight = tok.balance(&voter);
        assert!(weight > 0, "voter must hold governance tokens to vote");

        if support {
            proposal.votes_for += weight;
        } else {
            proposal.votes_against += weight;
        }

        env.storage().persistent().set(&DataKey::HasVoted(proposal_id, voter.clone()), &true);

        let vote_record = VoteRecord {
            proposal_id,
            voter: voter.clone(),
            weight,
            support,
            ledger: now,
        };
        env.storage()
            .persistent()
            .set(&DataKey::Vote(proposal_id, voter.clone()), &vote_record);

        env.storage().persistent().set(&DataKey::Proposal(proposal_id), &proposal);

        env.events().publish(
            (symbol_short!("voted"), proposal_id),
            (voter, support, weight),
        );
    }

    // ── Tallying & Queueing ───────────────────────────────────────────────

    /// Tally the votes after the voting period ends and queue if passed.
    ///
    /// Anyone may call this after `proposal.voting_end`.
    pub fn tally(env: Env, proposal_id: u64) {
        let mut proposal = Self::load_proposal(&env, proposal_id);
        assert!(
            proposal.status == ProposalStatus::Active,
            "proposal is not active"
        );

        let now = env.ledger().sequence();
        assert!(now > proposal.voting_end, "voting period not yet ended");

        let gov_token: Address = env
            .storage()
            .persistent()
            .get(&DataKey::GovToken)
            .expect("not initialized");
        let tok = token::Client::new(&env, &gov_token);
        let total_supply = tok.total_supply();

        // Check quorum: total votes cast must be ≥ QUORUM_BPS of total supply.
        let total_votes = proposal.votes_for + proposal.votes_against;
        let quorum = (total_supply * QUORUM_BPS as i128) / 10_000;

        let passed = total_votes >= quorum && proposal.votes_for > proposal.votes_against;

        if passed {
            let timelock: u32 = env
                .storage()
                .persistent()
                .get(&DataKey::TimelockLedgers)
                .unwrap_or(DEFAULT_TIMELOCK_LEDGERS);
            proposal.status = ProposalStatus::Queued;
            proposal.executable_after = now + timelock;
        } else {
            proposal.status = ProposalStatus::Defeated;
        }

        env.storage().persistent().set(&DataKey::Proposal(proposal_id), &proposal);

        let action = if passed {
            String::from_str(&env, "queued")
        } else {
            String::from_str(&env, "defeated")
        };
        // Use admin placeholder for system action.
        let admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .expect("not initialized");
        Self::record_history(&env, proposal_id, action.clone(), admin.clone());

        env.events().publish(
            (symbol_short!("tallied"), proposal_id),
            (passed, proposal.votes_for, proposal.votes_against),
        );
    }

    // ── Multi-sig Approval ────────────────────────────────────────────────

    /// Multi-sig signer approves a queued proposal.
    ///
    /// Only addresses in the `MultiSigSigners` list may call this.
    pub fn approve_multisig(env: Env, signer: Address, proposal_id: u64) {
        signer.require_auth();

        let signers: Vec<Address> = env
            .storage()
            .persistent()
            .get(&DataKey::MultiSigSigners)
            .expect("not initialized");

        assert!(
            signers.contains(&signer),
            "caller is not a registered multi-sig signer"
        );

        let mut proposal = Self::load_proposal(&env, proposal_id);
        assert!(
            proposal.status == ProposalStatus::Queued,
            "proposal is not in Queued state"
        );
        assert!(proposal.requires_multisig, "proposal does not require multi-sig");

        // Guard against double approval.
        let already: bool = env
            .storage()
            .persistent()
            .get(&DataKey::MultiSigApproval(proposal_id, signer.clone()))
            .unwrap_or(false);
        assert!(!already, "signer has already approved this proposal");

        env.storage()
            .persistent()
            .set(&DataKey::MultiSigApproval(proposal_id, signer.clone()), &true);

        proposal.multisig_approvals += 1;

        // Check if threshold met → mark as Ready.
        let threshold: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::MultiSigThreshold)
            .unwrap_or(1);

        if proposal.multisig_approvals >= threshold {
            // Also enforce timelock.
            let now = env.ledger().sequence();
            if now >= proposal.executable_after {
                proposal.status = ProposalStatus::Ready;
            }
        }

        env.storage().persistent().set(&DataKey::Proposal(proposal_id), &proposal);

        Self::record_history(&env, proposal_id, String::from_str(&env, "multisig_approved"), signer.clone());
        env.events().publish(
            (symbol_short!("msapprove"), proposal_id),
            (signer, proposal.multisig_approvals),
        );
    }

    // ── Execution ─────────────────────────────────────────────────────────

    /// Mark a queued/ready proposal as executed.
    ///
    /// Actual on-chain execution of the governance action (e.g. updating a
    /// config parameter) must be performed separately by calling the target
    /// contract with the proposal's encoded calldata.  This function records
    /// the execution on the governance ledger.
    ///
    /// Admin must call after the timelock has expired.
    pub fn execute(env: Env, admin: Address, proposal_id: u64) {
        admin.require_auth();
        let stored_admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .expect("not initialized");
        assert!(admin == stored_admin, "only admin can execute");

        let mut proposal = Self::load_proposal(&env, proposal_id);

        // Proposals requiring multi-sig must reach Ready status.
        if proposal.requires_multisig {
            assert!(
                proposal.status == ProposalStatus::Ready,
                "multi-sig proposal must be in Ready state"
            );
        } else {
            assert!(
                proposal.status == ProposalStatus::Queued
                    || proposal.status == ProposalStatus::Ready,
                "proposal is not executable"
            );
        }

        let now = env.ledger().sequence();
        assert!(
            now >= proposal.executable_after,
            "timelock has not yet expired"
        );

        proposal.status = ProposalStatus::Executed;
        env.storage().persistent().set(&DataKey::Proposal(proposal_id), &proposal);

        Self::record_history(&env, proposal_id, String::from_str(&env, "executed"), admin.clone());
        env.events().publish((symbol_short!("executed"),), (proposal_id, admin));
    }

    // ── Queries ───────────────────────────────────────────────────────────

    /// Retrieve a proposal record.
    pub fn get_proposal(env: Env, proposal_id: u64) -> Proposal {
        Self::load_proposal(&env, proposal_id)
    }

    /// Check if a voter has voted on a given proposal.
    pub fn has_voted(env: Env, proposal_id: u64, voter: Address) -> bool {
        env.storage()
            .persistent()
            .get(&DataKey::HasVoted(proposal_id, voter))
            .unwrap_or(false)
    }

    /// Get the vote record for a specific voter on a proposal.
    pub fn get_vote(env: Env, proposal_id: u64, voter: Address) -> Option<VoteRecord> {
        env.storage()
            .persistent()
            .get(&DataKey::Vote(proposal_id, voter))
    }

    /// Get a paginated slice of governance history entries.
    pub fn get_history(env: Env, offset: u64, limit: u64) -> Vec<GovernanceEvent> {
        let count: u64 = env
            .storage()
            .persistent()
            .get(&DataKey::HistoryCount)
            .unwrap_or(0);

        let mut result = Vec::new(&env);
        let end = (offset + limit).min(count);
        let mut i = offset;
        while i < end {
            if let Some(entry) = env
                .storage()
                .persistent()
                .get::<DataKey, GovernanceEvent>(&DataKey::History(i))
            {
                result.push_back(entry);
            }
            i += 1;
        }
        result
    }

    // ── Internal Helpers ──────────────────────────────────────────────────

    fn next_proposal_id(env: &Env) -> u64 {
        let count: u64 = env
            .storage()
            .persistent()
            .get(&DataKey::ProposalCount)
            .unwrap_or(0);
        let next = count + 1;
        env.storage().persistent().set(&DataKey::ProposalCount, &next);
        next
    }

    fn load_proposal(env: &Env, proposal_id: u64) -> Proposal {
        env.storage()
            .persistent()
            .get(&DataKey::Proposal(proposal_id))
            .expect("proposal not found")
    }

    fn record_history(env: &Env, proposal_id: u64, action: String, actor: Address) {
        let count: u64 = env
            .storage()
            .persistent()
            .get(&DataKey::HistoryCount)
            .unwrap_or(0);

        let entry = GovernanceEvent {
            proposal_id,
            action,
            actor,
            ledger: env.ledger().sequence(),
        };

        env.storage().persistent().set(&DataKey::History(count), &entry);
        env.storage().persistent().set(&DataKey::HistoryCount, &(count + 1));
    }
}
