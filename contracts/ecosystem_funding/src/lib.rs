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
    ///
    /// Unbounded: this returns the whole stored vector, and
    /// `add_milestone` appends to it for the life of the program. Prefer
    /// [`get_milestones_page`](Self::get_milestones_page), which bounds the
    /// response with `shared::pagination`. Kept for callers that already rely
    /// on the unbounded shape.
    pub fn get_milestones(env: Env, program_id: u64) -> Vec<ProgramMilestone> {
        env.storage()
            .persistent()
            .get(&DataKey::Milestones(program_id))
            .expect("milestones not found")
    }

    /// Retrieve a bounded page of a program's milestones, plus its `PageInfo`.
    ///
    /// A page spans at most `shared::pagination::MAX_LIMIT` milestones however
    /// large `limit` is. A `limit` of `0` yields the default page size rather
    /// than an empty result, and an `offset` past the end is an empty page
    /// rather than an error.
    pub fn get_milestones_page(
        env: Env,
        program_id: u64,
        offset: u32,
        limit: u32,
    ) -> (Vec<ProgramMilestone>, shared::pagination::PageInfo) {
        let milestones: Vec<ProgramMilestone> = env
            .storage()
            .persistent()
            .get(&DataKey::Milestones(program_id))
            .unwrap_or_else(|| Vec::new(&env));
        shared::pagination::paginated(&env, &milestones, offset, limit)
    }

    /// Retrieve a paginated list of outcome metrics.
    ///
    /// The cursor and the limit are both clamped by `shared::pagination`, so a
    /// page spans at most `shared::pagination::MAX_LIMIT` outcomes however
    /// large `limit` is. A `limit` of `0` yields the default page size rather
    /// than an empty result, and an `offset` past the end of the log is an
    /// empty page rather than an error, so a client can page to the end without
    /// reading a count first.
    ///
    /// For the page metadata that makes that loop safe, use
    /// [`get_outcomes_page`](Self::get_outcomes_page).
    pub fn get_outcomes(env: Env, program_id: u64, offset: u64, limit: u64) -> Vec<ProgramOutcome> {
        let (outcomes, _info) = Self::outcomes_page(&env, program_id, offset, limit);
        outcomes
    }

    /// Like [`get_outcomes`](Self::get_outcomes), but returns the page together
    /// with its `PageInfo`: the total outcome count, the next cursor, and
    /// whether anything exists past this page.
    pub fn get_outcomes_page(
        env: Env,
        program_id: u64,
        offset: u64,
        limit: u64,
    ) -> (Vec<ProgramOutcome>, shared::pagination::PageInfo) {
        Self::outcomes_page(&env, program_id, offset, limit)
    }

    // ── Internal ──────────────────────────────────────────────────────────

    /// Shared body of the two outcome readers, bounded by
    /// `shared::pagination::collect_window` (closes #876).
    ///
    /// `OutcomeCount` and `DataKey::Outcome` are keyed by `u64`, while the
    /// pagination arithmetic is `u32`. The narrowing conversions saturate
    /// rather than wrap: a log of `u32::MAX` outcomes is already far past
    /// anything a 100-entry page can address, and wrapping would report a
    /// small `total` for a very long log and mis-page the client.
    fn outcomes_page(
        env: &Env,
        program_id: u64,
        offset: u64,
        limit: u64,
    ) -> (Vec<ProgramOutcome>, shared::pagination::PageInfo) {
        let count: u64 = env
            .storage()
            .persistent()
            .get(&DataKey::OutcomeCount(program_id))
            .unwrap_or(0);

        shared::pagination::collect_window(
            env,
            u32::try_from(count).unwrap_or(u32::MAX),
            u32::try_from(offset).unwrap_or(u32::MAX),
            u32::try_from(limit).unwrap_or(u32::MAX),
            |i| {
                env.storage()
                    .persistent()
                    .get::<DataKey, ProgramOutcome>(&DataKey::Outcome(program_id, u64::from(i)))
            },
        )
    }

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

// ── Bounded page reads (#876) ───────────────────────────────────────────────

#[cfg(test)]
mod pagination_tests {
    extern crate std;

    use super::*;
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::String;

    fn s(env: &Env, text: &str) -> String {
        String::from_str(env, text)
    }

    /// A program with `milestones` milestones and `outcomes` outcome records.
    ///
    /// `initialize` and `create_program` take no token calls, so a generated
    /// address stands in for the disbursement token. `create_program` rejects an
    /// empty milestone list, so `milestones == 0` still creates one; the outcome
    /// tests pass 0 and read only the outcome log.
    fn setup(env: &Env, milestones: u32, outcomes: u32) -> (EcosystemFundingClient, u64) {
        env.mock_all_auths();

        let contract_id = env.register_contract(None, EcosystemFunding);
        let client = EcosystemFundingClient::new(env, &contract_id);
        client.initialize(&Address::generate(env));

        let mut ms: Vec<(String, String, i128)> = soroban_sdk::vec![env];
        for i in 0..std::cmp::max(milestones, 1) {
            ms.push_back((s(env, "m"), s(env, "d"), 100 + i as i128));
        }

        let manager = Address::generate(env);
        let recipient = Address::generate(env);
        let program_id = client.create_program(
            &manager,
            &recipient,
            &s(env, "program"),
            &s(env, "desc"),
            &ProgramType::Grant,
            &ms,
            &Address::generate(env),
        );

        for i in 0..outcomes {
            client.record_outcome(
                &recipient,
                &program_id,
                &s(env, "metric"),
                &(i as i128),
                &s(env, "note"),
            );
        }

        (client, program_id)
    }

    #[test]
    fn get_milestones_page_returns_a_bounded_page() {
        let env = Env::default();
        let (client, program_id) = setup(&env, 5, 0);

        let (page, info) = client.get_milestones_page(&program_id, &0, &2);
        assert_eq!(page.len(), 2);
        assert_eq!(info.total, 5);
        assert_eq!(info.count, 2);
        assert!(info.has_more);
        assert_eq!(info.next_start, Some(2));

        let (page, info) = client.get_milestones_page(&program_id, &2, &2);
        assert_eq!(page.len(), 2);
        assert_eq!(info.next_start, Some(4));

        // A short final page rather than an error.
        let (page, info) = client.get_milestones_page(&program_id, &4, &2);
        assert_eq!(page.len(), 1);
        assert!(!info.has_more);
        assert_eq!(info.next_start, None);

        // The unbounded reader still returns everything it always did.
        assert_eq!(client.get_milestones(&program_id).len(), 5);
    }

    #[test]
    fn get_milestones_page_handles_the_edge_cases() {
        let env = Env::default();
        let (client, program_id) = setup(&env, 3, 0);

        // start > len: empty page, not an error, and no next cursor.
        let (page, info) = client.get_milestones_page(&program_id, &99, &10);
        assert!(page.is_empty());
        assert_eq!(info.total, 3);
        assert_eq!(info.count, 0);
        assert!(!info.has_more);
        assert_eq!(info.next_start, None);

        // limit == 0 must not mean "return nothing": it yields the default page
        // size, which here covers the whole log.
        let (page, info) = client.get_milestones_page(&program_id, &0, &0);
        assert_eq!(page.len(), 3);
        assert_eq!(info.limit, 3);
        assert_eq!(info.count, 3);

        // A limit above the cap is clamped rather than honoured. With only 3
        // milestones the resulting span is still the whole list.
        let (page, info) =
            client.get_milestones_page(&program_id, &0, &(shared::pagination::MAX_LIMIT + 1));
        assert_eq!(info.limit, 3);
        assert_eq!(page.len(), 3);
    }

    #[test]
    fn get_milestones_page_reports_a_single_milestone_as_final() {
        let env = Env::default();
        let (client, program_id) = setup(&env, 1, 0);

        // The smallest non-empty program: one milestone, no next page.
        let (page, info) = client.get_milestones_page(&program_id, &0, &50);
        assert_eq!(page.len(), 1);
        assert_eq!(info.total, 1);
        assert!(!info.has_more);
        assert_eq!(info.next_start, None);
    }

    #[test]
    fn get_outcomes_clamps_an_oversized_limit() {
        let env = Env::default();
        let (client, program_id) = setup(&env, 0, 4);

        // This is the defect #876 fixes: `limit` used to be trusted verbatim, so
        // `u64::MAX` scanned the whole log. It is now capped.
        let outcomes = client.get_outcomes(&program_id, &0, &u64::MAX);
        assert_eq!(
            outcomes.len(),
            4,
            "an oversized limit must not widen the read"
        );

        // `PageInfo.limit` is the span actually visited, so with 4 records on
        // file the cap is never reached and the span is the whole log. The
        // clamp itself is pinned against a log longer than the cap by
        // `shared::pagination`'s own unit tests.
        let (page, info) = client.get_outcomes_page(&program_id, &0, &u64::MAX);
        assert_eq!(info.limit, 4);
        assert_eq!(info.total, 4);
        assert_eq!(info.count, 4);
        assert_eq!(page.len(), 4);
        assert!(!info.has_more);
    }

    #[test]
    fn get_outcomes_pages_through_the_log() {
        let env = Env::default();
        let (client, program_id) = setup(&env, 0, 5);

        let (page, info) = client.get_outcomes_page(&program_id, &0, &2);
        assert_eq!(page.len(), 2);
        assert_eq!(info.total, 5);
        assert!(info.has_more);
        assert_eq!(info.next_start, Some(2));
        assert_eq!(page.get(0).unwrap().metric_value, 0);
        assert_eq!(page.get(1).unwrap().metric_value, 1);

        let (page, info) = client.get_outcomes_page(&program_id, &2, &2);
        assert_eq!(page.get(0).unwrap().metric_value, 2);
        assert_eq!(page.get(1).unwrap().metric_value, 3);
        assert_eq!(info.next_start, Some(4));

        // Short final page.
        let (page, info) = client.get_outcomes_page(&program_id, &4, &2);
        assert_eq!(page.len(), 1);
        assert_eq!(page.get(0).unwrap().metric_value, 4);
        assert!(!info.has_more);
        assert_eq!(info.next_start, None);

        // The unbounded reader keeps its existing behaviour and signature.
        assert_eq!(client.get_outcomes(&program_id, &0, &2).len(), 2);
    }

    #[test]
    fn get_outcomes_page_handles_the_edge_cases() {
        let env = Env::default();
        let (client, program_id) = setup(&env, 0, 2);

        // start > len.
        let (page, info) = client.get_outcomes_page(&program_id, &u64::MAX, &10);
        assert!(page.is_empty());
        assert_eq!(info.total, 2);
        assert!(!info.has_more);

        // limit == 0 falls back to the default page size, which covers the whole
        // 2-record log.
        let (page, info) = client.get_outcomes_page(&program_id, &0, &0);
        assert_eq!(info.limit, 2);
        assert_eq!(page.len(), 2);

        // offset + limit must not wrap: the old code did `offset + limit`
        // unchecked, so a huge offset and a huge limit could overflow.
        let (page, info) = client.get_outcomes_page(&program_id, &u64::MAX, &u64::MAX);
        assert!(page.is_empty());
        assert!(!info.has_more);
    }
}
