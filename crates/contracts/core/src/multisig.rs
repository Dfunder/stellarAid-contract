use crate::{assets, events, rbac::Rbac};
use soroban_sdk::{contracttype, token, Address, Env, String};

const DEFAULT_SINGLE_SIG_LIMIT: i128 = 1_000_000;
const DEFAULT_PROPOSAL_TTL_SECS: u64 = 86_400;

#[contracttype]
#[derive(Clone, Eq, PartialEq)]
pub enum ProposalStatus {
    Pending,
    Executed,
    Cancelled,
    Expired,
}

#[contracttype]
#[derive(Clone)]
pub struct MultisigConfig {
    pub threshold: u32,
    pub single_sig_limit: i128,
    pub proposal_ttl_secs: u64,
}

#[contracttype]
#[derive(Clone)]
pub struct WithdrawalProposal {
    pub id: u64,
    pub proposer: Address,
    pub recipient: Address,
    pub amount: i128,
    pub asset: String,
    pub created_at: u64,
    pub expires_at: u64,
    pub approval_count: u32,
    pub status: ProposalStatus,
}

#[contracttype]
pub enum MultisigStorageKey {
    Config,
    ProposalNonce,
    Proposal(u64),
    Approval(u64, Address),
}

pub struct MultisigWithdrawal;

impl MultisigWithdrawal {
    pub fn init(env: &Env, admin: &Address) {
        Rbac::init_approvers(env, admin);

        if env.storage().instance().has(&MultisigStorageKey::Config) {
            return;
        }

        let config = MultisigConfig {
            threshold: 1,
            single_sig_limit: DEFAULT_SINGLE_SIG_LIMIT,
            proposal_ttl_secs: DEFAULT_PROPOSAL_TTL_SECS,
        };

        env.storage().instance().set(&MultisigStorageKey::Config, &config);
    }

    pub fn configure(
        env: &Env,
        caller: &Address,
        threshold: u32,
        single_sig_limit: i128,
        proposal_ttl_secs: u64,
    ) {
        Rbac::require_admin_auth(env, caller);

        if threshold == 0 {
            panic!("Threshold must be greater than zero");
        }

        let approver_count = Rbac::get_approvers(env).len();
        if threshold > approver_count {
            panic!("Threshold exceeds approver count");
        }

        if single_sig_limit <= 0 {
            panic!("Single-sig limit must be positive");
        }

        if proposal_ttl_secs == 0 {
            panic!("Proposal TTL must be greater than zero");
        }

        let config = MultisigConfig {
            threshold,
            single_sig_limit,
            proposal_ttl_secs,
        };

        env.storage().instance().set(&MultisigStorageKey::Config, &config);

        events::WithdrawalMultisigConfigUpdated {
            admin: caller.clone(),
            threshold,
            single_sig_limit,
            proposal_ttl_secs,
            timestamp: env.ledger().timestamp(),
        }
        .emit(env);
    }

    pub fn get_config(env: &Env) -> MultisigConfig {
        env.storage()
            .instance()
            .get(&MultisigStorageKey::Config)
            .unwrap_or(MultisigConfig {
                threshold: 1,
                single_sig_limit: DEFAULT_SINGLE_SIG_LIMIT,
                proposal_ttl_secs: DEFAULT_PROPOSAL_TTL_SECS,
            })
    }

    pub fn propose_withdrawal(
        env: &Env,
        caller: &Address,
        recipient: &Address,
        amount: i128,
        asset: &String,
    ) -> u64 {
        Rbac::require_approver_auth(env, caller);

        if amount <= 0 {
            panic!("Withdrawal amount must be positive");
        }

        let config = Self::get_config(env);
        if amount <= config.single_sig_limit {
            panic!("Amount is within single-sig limit; use withdraw");
        }

        if config.threshold > Rbac::get_approvers(env).len() {
            panic!("Threshold exceeds approver count");
        }

        let proposal_id = Self::next_proposal_id(env);
        let now = env.ledger().timestamp();

        let proposal = WithdrawalProposal {
            id: proposal_id,
            proposer: caller.clone(),
            recipient: recipient.clone(),
            amount,
            asset: asset.clone(),
            created_at: now,
            expires_at: now + config.proposal_ttl_secs,
            approval_count: 0,
            status: ProposalStatus::Pending,
        };

        env.storage()
            .instance()
            .set(&MultisigStorageKey::Proposal(proposal_id), &proposal);

        events::WithdrawalProposalCreated {
            proposal_id,
            proposer: caller.clone(),
            recipient: recipient.clone(),
            amount,
            asset: asset.clone(),
            threshold: config.threshold,
            expires_at: proposal.expires_at,
            timestamp: now,
        }
        .emit(env);

        let _ = Self::approve_withdrawal_internal(env, caller, proposal_id);

        proposal_id
    }

    pub fn approve_withdrawal(env: &Env, caller: &Address, proposal_id: u64) -> bool {
        Rbac::require_approver_auth(env, caller);
        Self::approve_withdrawal_internal(env, caller, proposal_id)
    }

    pub fn cancel_withdrawal(env: &Env, caller: &Address, proposal_id: u64) -> bool {
        caller.require_auth();

        let mut proposal = Self::get_required_proposal(env, proposal_id);

        if proposal.status != ProposalStatus::Pending {
            panic!("Proposal is not pending");
        }

        Self::expire_if_needed(env, &mut proposal);

        let is_admin = Rbac::get_admin(env)
            .map(|admin| admin == caller.clone())
            .unwrap_or(false);
        let is_proposer = proposal.proposer == caller.clone();

        if !is_admin && !is_proposer {
            panic!("Unauthorized: caller cannot cancel proposal");
        }

        proposal.status = ProposalStatus::Cancelled;
        env.storage()
            .instance()
            .set(&MultisigStorageKey::Proposal(proposal_id), &proposal);

        events::WithdrawalProposalCancelled {
            proposal_id,
            cancelled_by: caller.clone(),
            timestamp: env.ledger().timestamp(),
        }
        .emit(env);

        true
    }

    pub fn get_proposal(env: &Env, proposal_id: u64) -> Option<WithdrawalProposal> {
        env.storage()
            .instance()
            .get(&MultisigStorageKey::Proposal(proposal_id))
    }

    fn approve_withdrawal_internal(env: &Env, caller: &Address, proposal_id: u64) -> bool {
        let mut proposal = Self::get_required_proposal(env, proposal_id);

        if proposal.status != ProposalStatus::Pending {
            panic!("Proposal is not pending");
        }

        Self::expire_if_needed(env, &mut proposal);

        let approval_key = MultisigStorageKey::Approval(proposal_id, caller.clone());
        if env.storage().instance().has(&approval_key) {
            panic!("Approver has already approved this proposal");
        }

        env.storage().instance().set(&approval_key, &true);

        proposal.approval_count += 1;
        env.storage()
            .instance()
            .set(&MultisigStorageKey::Proposal(proposal_id), &proposal);

        let config = Self::get_config(env);
        events::WithdrawalProposalApproved {
            proposal_id,
            approver: caller.clone(),
            approval_count: proposal.approval_count,
            threshold: config.threshold,
            timestamp: env.ledger().timestamp(),
        }
        .emit(env);

        if proposal.approval_count >= config.threshold {
            Self::execute_proposal(env, caller, &mut proposal);
            return true;
        }

        false
    }

    fn execute_proposal(env: &Env, caller: &Address, proposal: &mut WithdrawalProposal) {
        if proposal.status != ProposalStatus::Pending {
            panic!("Proposal is not pending");
        }

        if env.ledger().timestamp() > proposal.expires_at {
            proposal.status = ProposalStatus::Expired;
            env.storage()
                .instance()
                .set(&MultisigStorageKey::Proposal(proposal.id), proposal);

            events::WithdrawalProposalExpired {
                proposal_id: proposal.id,
                timestamp: env.ledger().timestamp(),
            }
            .emit(env);

            panic!("Proposal has expired");
        }

        let _ = execute_withdrawal_transfer(env, &proposal.recipient, proposal.amount, &proposal.asset);

        proposal.status = ProposalStatus::Executed;
        env.storage()
            .instance()
            .set(&MultisigStorageKey::Proposal(proposal.id), proposal);

        events::WithdrawalProposalExecuted {
            proposal_id: proposal.id,
            recipient: proposal.recipient.clone(),
            amount: proposal.amount,
            asset: proposal.asset.clone(),
            executed_by: caller.clone(),
            timestamp: env.ledger().timestamp(),
        }
        .emit(env);
    }

    fn expire_if_needed(env: &Env, proposal: &mut WithdrawalProposal) {
        if env.ledger().timestamp() <= proposal.expires_at {
            return;
        }

        proposal.status = ProposalStatus::Expired;
        env.storage()
            .instance()
            .set(&MultisigStorageKey::Proposal(proposal.id), proposal);

        events::WithdrawalProposalExpired {
            proposal_id: proposal.id,
            timestamp: env.ledger().timestamp(),
        }
        .emit(env);

        panic!("Proposal has expired");
    }

    fn get_required_proposal(env: &Env, proposal_id: u64) -> WithdrawalProposal {
        env.storage()
            .instance()
            .get(&MultisigStorageKey::Proposal(proposal_id))
            .unwrap_or_else(|| panic!("Withdrawal proposal not found"))
    }

    fn next_proposal_id(env: &Env) -> u64 {
        let current = env
            .storage()
            .instance()
            .get::<_, u64>(&MultisigStorageKey::ProposalNonce)
            .unwrap_or(0);
        let next = current + 1;
        env.storage()
            .instance()
            .set(&MultisigStorageKey::ProposalNonce, &next);
        next
    }
}

pub fn execute_withdrawal_transfer(env: &Env, recipient: &Address, amount: i128, asset: &String) -> i128 {
    if amount <= 0 {
        panic!("Withdrawal amount must be positive");
    }

    let asset_code = asset.to_string();
    let asset_contract = assets::AssetConfig::get_contract_address(env, &asset_code)
        .unwrap_or_else(|| panic!("Asset contract address not configured for {}", asset_code));

    let token_client = token::Client::new(env, &asset_contract);

    let contract_address = env.current_contract_address();
    let balance = token_client.balance(&contract_address);
    if balance < amount {
        panic!("Insufficient contract balance for withdrawal");
    }

    token_client.transfer(&contract_address, recipient, &amount);

    events::WithdrawalProcessed {
        recipient: recipient.clone(),
        amount,
        asset: asset.clone(),
        timestamp: env.ledger().timestamp(),
    }
    .emit(env);

    amount
}
