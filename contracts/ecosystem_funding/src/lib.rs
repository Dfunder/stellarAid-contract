// contracts/ecosystem_funding/src/lib.rs
// Implements Ecosystem Funding Programs — closes #617
//
// Acceptance Criteria:
// - Support program creation and management
// - Implement fund allocation
// - Add milestone-based disbursement
// - Track program outcomes
// - Implement performance metrics

#![no_std]

use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short, token, Address, Env, String, Vec,
};

// ── Data Types ────────────────────────────────────────────────────────────────

/// Category of ecosystem program.
#[contracttype]
#[derive(Clone, PartialEq)]
pub enum ProgramType {
    /// Direct grant with no return expectation.
    Grant,
    /// Early-stage support with mentoring and resources.
    Incubator,
    /// Growth-stage program with funding and go-to-market support.
    Accelerator,
}

/// Status of an ecosystem program.
#[contracttype]
#[derive(Clone, PartialEq)]
pub enum ProgramStatus {
    Active,
    Paused,
    Completed,
    Cancelled,
}

/// Milestone within a program that triggers a fund disbursement.
#[contracttype]
#[derive(Clone, PartialEq)]
pub enum ProgramMilestoneStatus {
    Pending,
    Submitted,
    Approved,
    Rejected,
}

/// A single funding milestone.
#[contracttype]
#[derive(Clone)]
pub struct ProgramMilestone {
    pub index: u32,
    pub title: String,
    pub description: String,
    pub disbursement_amount: i128,
    pub status: ProgramMilestoneStatus,
    pub submitted_at: u32,
    pub approved_at: u32,
    /// Hash / CID of the deliverable report (off-chain).
    pub deliverable_uri: String,
}

/// A program outcome / metrics record submitted by the recipient.
#[contracttype]
#[derive(Clone)]
pub struct ProgramOutcome {
    pub program_id: u64,
    pub recipient: Address,
    pub metric_name: String,
    pub metric_value: i128,
    pub description: String,
    pub ledger: u32,
}

/// The core ecosystem funding program record.
#[contracttype]
#[derive(Clone)]
pub struct FundingProgram {
    pub id: u64,
    pub name: String,
    pub description: String,
    pub program_type: ProgramType,
    /// Address managing the program (admin or designated manager).
    pub manager: Address,
    /// Primary recipient / grantee.
    pub recipient: Address,
    /// Total funds allocated to the program.
    pub total_allocation: i128,
    /// Total funds disbursed so far.
    pub total_disbursed: i128,
    /// Token contract for disbursements.
    pub token: Address,
    pub status: ProgramStatus,
    pub created_at: u32,
    pub completed_at: u32,
}

// ── Storage Keys ──────────────────────────────────────────────────────────────

#[contracttype]
pub enum DataKey {
    /// Program record.
    Program(u64),
    /// Program id counter.
    ProgramCount,
    /// Milestones for a program.
    Milestones(u64),
    /// Performance outcome count.
    OutcomeCount(u64),
    /// Individual outcome entry.
    Outcome(u64, u64),
    /// Admin address.
    Admin,
}

// ── Contract ──────────────────────────────────────────────────────────────────

#[contract]
pub struct EcosystemFunding;

#[contractimpl]
impl EcosystemFunding {
    // ── Admin ─────────────────────────────────────────────────────────────

    /// Initialise the ecosystem funding contract.
    pub fn initialize(env: Env, admin: Address) {
        admin.require_auth();
        env.storage().persistent().set(&DataKey::Admin, &admin);
        env.storage().persistent().set(&DataKey::ProgramCount, &0u64);
        env.events().publish((symbol_short!("init"),), admin);
    }

    // ── Program Lifecycle ─────────────────────────────────────────────────

    /// Create a new ecosystem funding program.
    ///
    /// # Arguments
    /// * `manager`           — program manager / admin. Must sign.
    /// * `recipient`         — grantee / startup receiving funding.
    /// * `name`              — program name.
    /// * `description`       — program overview.
    /// * `program_type`      — `Grant` / `Incubator` / `Accelerator`.
    /// * `milestones`        — list of `(title, description, disbursement_amount)`.
    /// * `token`             — token contract for disbursements.
    ///
    /// Total allocation is computed as the sum of all milestone amounts.
    ///
    /// Returns the new `program_id`.
    pub fn create_program(
        env: Env,
        manager: Address,
        recipient: Address,
        name: String,
        description: String,
        program_type: ProgramType,
        milestones: Vec<(String, String, i128)>,
        token: Address,
    ) -> u64 {
        manager.require_auth();
        assert!(!milestones.is_empty(), "at least one milestone required");

        // Compute total allocation.
        let mut total_allocation: i128 = 0;
        for (_, _, amount) in milestones.iter() {
            assert!(amount > 0, "milestone disbursement must be positive");
            total_allocation += amount;
        }

        let id = Self::next_id(&env);

        let program = FundingProgram {
            id,
            name: name.clone(),
            description,
            program_type: program_type.clone(),
            manager: manager.clone(),
            recipient: recipient.clone(),
            total_allocation,
            total_disbursed: 0,
            token,
            status: ProgramStatus::Active,
            created_at: env.ledger().sequence(),
            completed_at: 0,
        };

        env.storage().persistent().set(&DataKey::Program(id), &program);
        env.storage().persistent().set(&DataKey::OutcomeCount(id), &0u64);

        // Store milestones.
        let mut ms: Vec<ProgramMilestone> = Vec::new(&env);
        let mut idx: u32 = 0;
        for (title, desc, amount) in milestones.iter() {
            ms.push_back(ProgramMilestone {
                index: idx,
                title,
                description: desc,
                disbursement_amount: amount,
                status: ProgramMilestoneStatus::Pending,
                submitted_at: 0,
                approved_at: 0,
                deliverable_uri: String::from_str(&env, ""),
            });
            idx += 1;
        }
        env.storage().persistent().set(&DataKey::Milestones(id), &ms);

        env.events().publish(
            (symbol_short!("created"),),
            (id, manager, recipient, name, program_type),
        );

        id
    }

    /// Pause an active program (admin or manager only).
    pub fn pause_program(env: Env, caller: Address, program_id: u64) {
        caller.require_auth();
        let mut program = Self::load_program(&env, program_id);
        Self::require_manager_or_admin(&env, &caller, &program);
        assert!(program.status == ProgramStatus::Active, "program is not active");

        program.status = ProgramStatus::Paused;
        env.storage().persistent().set(&DataKey::Program(program_id), &program);
        env.events().publish((symbol_short!("paused"),), (program_id, caller));
    }

    /// Resume a paused program.
    pub fn resume_program(env: Env, caller: Address, program_id: u64) {
        caller.require_auth();
        let mut program = Self::load_program(&env, program_id);
        Self::require_manager_or_admin(&env, &caller, &program);
        assert!(program.status == ProgramStatus::Paused, "program is not paused");

        program.status = ProgramStatus::Active;
        env.storage().persistent().set(&DataKey::Program(program_id), &program);
        env.events().publish((symbol_short!("resumed"),), (program_id, caller));
    }

    /// Cancel a program and stop disbursements.
    pub fn cancel_program(env: Env, manager: Address, program_id: u64) {
        manager.require_auth();
        let mut program = Self::load_program(&env, program_id);
        Self::require_manager_or_admin(&env, &manager, &program);
        assert!(
            program.status == ProgramStatus::Active || program.status == ProgramStatus::Paused,
            "cannot cancel a completed or already-cancelled program"
        );

        program.status = ProgramStatus::Cancelled;
        env.storage().persistent().set(&DataKey::Program(program_id), &program);
        env.events().publish((symbol_short!("cancelled"),), (program_id, manager));
    }

    // ── Milestone-based Disbursement ──────────────────────────────────────

    /// Recipient submits a milestone deliverable for manager review.
    ///
    /// # Arguments
    /// * `recipient`          — grantee. Must sign.
    /// * `program_id`         — program identifier.
    /// * `milestone_index`    — index of the milestone being submitted.
    /// * `deliverable_uri`    — URI / CID of the deliverable report.
    pub fn submit_milestone(
        env: Env,
        recipient: Address,
        program_id: u64,
        milestone_index: u32,
        deliverable_uri: String,
    ) {
        recipient.require_auth();
        let program = Self::load_program(&env, program_id);
        assert!(program.recipient == recipient, "caller is not the program recipient");
        assert!(program.status == ProgramStatus::Active, "program is not active");

        let mut milestones: Vec<ProgramMilestone> = env
            .storage()
            .persistent()
            .get(&DataKey::Milestones(program_id))
            .expect("milestones not found");

        let ms = milestones
            .get(milestone_index)
            .expect("milestone index out of range");
        assert!(
            ms.status == ProgramMilestoneStatus::Pending,
            "milestone already submitted or approved"
        );

        milestones.set(
            milestone_index,
            ProgramMilestone {
                status: ProgramMilestoneStatus::Submitted,
                submitted_at: env.ledger().sequence(),
                deliverable_uri: deliverable_uri.clone(),
                ..ms
            },
        );
        env.storage().persistent().set(&DataKey::Milestones(program_id), &milestones);

        env.events().publish(
            (symbol_short!("ms_sub"), program_id),
            (milestone_index, recipient, deliverable_uri),
        );
    }

    /// Manager approves or rejects a submitted milestone.
    ///
    /// On approval, the milestone disbursement amount is transferred to the
    /// recipient.  When all milestones are approved the program automatically
    /// transitions to `Completed`.
    pub fn review_milestone(
        env: Env,
        manager: Address,
        program_id: u64,
        milestone_index: u32,
        approve: bool,
    ) {
        manager.require_auth();
        let mut program = Self::load_program(&env, program_id);
        Self::require_manager_or_admin(&env, &manager, &program);
        assert!(program.status == ProgramStatus::Active, "program is not active");

        let mut milestones: Vec<ProgramMilestone> = env
            .storage()
            .persistent()
            .get(&DataKey::Milestones(program_id))
            .expect("milestones not found");

        let ms = milestones
            .get(milestone_index)
            .expect("milestone index out of range");
        assert!(
            ms.status == ProgramMilestoneStatus::Submitted,
            "milestone must be in Submitted state"
        );

        if approve {
            // Disburse funds.
            let tok = token::Client::new(&env, &program.token);
            tok.transfer(
                &env.current_contract_address(),
                &program.recipient,
                &ms.disbursement_amount,
            );

            program.total_disbursed += ms.disbursement_amount;

            milestones.set(
                milestone_index,
                ProgramMilestone {
                    status: ProgramMilestoneStatus::Approved,
                    approved_at: env.ledger().sequence(),
                    ..ms
                },
            );

            // Auto-complete when all milestones approved.
            if milestones
                .iter()
                .all(|m| m.status == ProgramMilestoneStatus::Approved)
            {
                program.status = ProgramStatus::Completed;
                program.completed_at = env.ledger().sequence();
                env.events()
                    .publish((symbol_short!("complete"),), program_id);
            }
        } else {
            milestones.set(
                milestone_index,
                ProgramMilestone {
                    status: ProgramMilestoneStatus::Rejected,
                    ..ms
                },
            );
        }

        env.storage().persistent().set(&DataKey::Milestones(program_id), &milestones);
        env.storage().persistent().set(&DataKey::Program(program_id), &program);

        env.events().publish(
            (symbol_short!("ms_rev"), program_id),
            (milestone_index, approve),
        );
    }

    // ── Performance Metrics ───────────────────────────────────────────────

    /// Recipient records a performance outcome metric.
    ///
    /// Examples: `("users_onboarded", 150)`, `("revenue_usd_cents", 120000)`.
    pub fn record_outcome(
        env: Env,
        recipient: Address,
        program_id: u64,
        metric_name: String,
        metric_value: i128,
        description: String,
    ) {
        recipient.require_auth();
        let program = Self::load_program(&env, program_id);
        assert!(program.recipient == recipient, "caller is not the program recipient");

        let count: u64 = env
            .storage()
            .persistent()
            .get(&DataKey::OutcomeCount(program_id))
            .unwrap_or(0);

        let outcome = ProgramOutcome {
            program_id,
            recipient: recipient.clone(),
            metric_name: metric_name.clone(),
            metric_value,
            description: description.clone(),
            ledger: env.ledger().sequence(),
        };

        env.storage()
            .persistent()
            .set(&DataKey::Outcome(program_id, count), &outcome);
        env.storage()
            .persistent()
            .set(&DataKey::OutcomeCount(program_id), &(count + 1));

        env.events().publish(
            (symbol_short!("outcome"), program_id),
            (recipient, metric_name, metric_value, description),
        );
    }

    // ── Queries ───────────────────────────────────────────────────────────

    /// Retrieve a program record.
    pub fn get_program(env: Env, program_id: u64) -> FundingProgram {
        Self::load_program(&env, program_id)
    }

    /// Retrieve all milestones for a program.
    pub fn get_milestones(env: Env, program_id: u64) -> Vec<ProgramMilestone> {
        env.storage()
            .persistent()
            .get(&DataKey::Milestones(program_id))
            .expect("milestones not found")
    }

    /// Retrieve a paginated list of outcome metrics.
    pub fn get_outcomes(env: Env, program_id: u64, offset: u64, limit: u64) -> Vec<ProgramOutcome> {
        let count: u64 = env
            .storage()
            .persistent()
            .get(&DataKey::OutcomeCount(program_id))
            .unwrap_or(0);

        let mut result = Vec::new(&env);
        let end = (offset + limit).min(count);
        let mut i = offset;
        while i < end {
            if let Some(entry) = env
                .storage()
                .persistent()
                .get::<DataKey, ProgramOutcome>(&DataKey::Outcome(program_id, i))
            {
                result.push_back(entry);
            }
            i += 1;
        }
        result
    }

    // ── Internal ──────────────────────────────────────────────────────────

    fn next_id(env: &Env) -> u64 {
        let count: u64 = env
            .storage()
            .persistent()
            .get(&DataKey::ProgramCount)
            .unwrap_or(0);
        let next = count + 1;
        env.storage().persistent().set(&DataKey::ProgramCount, &next);
        next
    }

    fn load_program(env: &Env, program_id: u64) -> FundingProgram {
        env.storage()
            .persistent()
            .get(&DataKey::Program(program_id))
            .expect("program not found")
    }

    fn require_manager_or_admin(env: &Env, caller: &Address, program: &FundingProgram) {
        if *caller == program.manager {
            return;
        }
        let admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .expect("not initialized");
        assert!(*caller == admin, "caller must be manager or admin");
    }
}
