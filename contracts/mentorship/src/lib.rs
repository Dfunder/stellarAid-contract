// contracts/mentorship/src/lib.rs
// Implements Mentorship Program Contract — closes #613
//
// Acceptance Criteria:
// - Support mentor-mentee pairing
// - Track mentorship milestones
// - Implement feedback collection
// - Add certification upon completion
// - Support compensation for mentors

#![no_std]

use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short, token, Address, Env, String, Vec,
};

// ── Data Types ────────────────────────────────────────────────────────────────

/// Status of a mentorship engagement.
#[contracttype]
#[derive(Clone, PartialEq)]
pub enum EngagementStatus {
    /// Mentor has been proposed, awaiting mentee acceptance.
    Proposed,
    /// Both parties have confirmed the engagement.
    Active,
    /// All milestones completed; certificate may be issued.
    Completed,
    /// Engagement cancelled by either party.
    Cancelled,
}

/// Individual milestone within a mentorship.
#[contracttype]
#[derive(Clone, PartialEq)]
pub enum MilestoneStatus {
    Pending,
    Submitted,
    Approved,
    Rejected,
}

/// A single milestone the mentee must complete.
#[contracttype]
#[derive(Clone)]
pub struct MentoringMilestone {
    pub index: u32,
    pub title: String,
    pub description: String,
    pub status: MilestoneStatus,
    /// Ledger when the milestone was approved (0 = not approved).
    pub approved_at: u32,
}

/// A feedback record submitted by either party.
#[contracttype]
#[derive(Clone)]
pub struct FeedbackEntry {
    pub engagement_id: u64,
    pub author: Address,
    pub rating: u32,
    pub comment: String,
    pub ledger: u32,
}

/// The core mentorship engagement record.
#[contracttype]
#[derive(Clone)]
pub struct MentoringEngagement {
    pub id: u64,
    /// Experienced professional providing guidance.
    pub mentor: Address,
    /// Emerging talent receiving mentorship.
    pub mentee: Address,
    /// Human-readable description of the program.
    pub description: String,
    /// Total compensation to be paid to the mentor.
    pub compensation: i128,
    /// SAX/USDC token contract used for compensation.
    pub token: Address,
    pub status: EngagementStatus,
    pub created_at: u32,
    pub completed_at: u32,
    /// Whether the completion certificate has been issued.
    pub certificate_issued: bool,
}

// ── Storage Keys ──────────────────────────────────────────────────────────────

#[contracttype]
pub enum DataKey {
    /// Engagement record.
    Engagement(u64),
    /// Engagement id counter.
    EngagementCount,
    /// Milestones for an engagement.
    Milestones(u64),
    /// Feedback entries count for an engagement.
    FeedbackCount(u64),
    /// Individual feedback entry.
    Feedback(u64, u64),
    /// Admin address.
    Admin,
}

// ── Contract ──────────────────────────────────────────────────────────────────

#[contract]
pub struct MentorshipProgram;

#[contractimpl]
impl MentorshipProgram {
    // ── Admin ─────────────────────────────────────────────────────────────

    /// Initialise the mentorship program contract.
    pub fn initialize(env: Env, admin: Address) {
        admin.require_auth();
        env.storage()
            .persistent()
            .set(&DataKey::Admin, &admin);
        env.storage()
            .persistent()
            .set(&DataKey::EngagementCount, &0u64);
        env.events()
            .publish((symbol_short!("init"),), admin);
    }

    // ── Engagement Lifecycle ──────────────────────────────────────────────

    /// Propose a new mentorship engagement.
    ///
    /// The `mentor` must sign.  The engagement starts in `Proposed` status
    /// until the mentee calls `accept_engagement`.
    ///
    /// # Arguments
    /// * `mentor`        — address of the mentor
    /// * `mentee`        — address of the mentee
    /// * `description`   — program overview
    /// * `milestones`    — ordered list of milestone titles/descriptions
    /// * `compensation`  — total payment to mentor (in base token units)
    /// * `token`         — token contract for compensation
    ///
    /// Returns the engagement id.
    pub fn propose_engagement(
        env: Env,
        mentor: Address,
        mentee: Address,
        description: String,
        milestones: Vec<(String, String)>,
        compensation: i128,
        token: Address,
    ) -> u64 {
        mentor.require_auth();
        assert!(compensation >= 0, "compensation must be non-negative");
        assert!(!milestones.is_empty(), "at least one milestone required");

        let id = Self::next_id(&env);

        let engagement = MentoringEngagement {
            id,
            mentor: mentor.clone(),
            mentee: mentee.clone(),
            description,
            compensation,
            token,
            status: EngagementStatus::Proposed,
            created_at: env.ledger().sequence(),
            completed_at: 0,
            certificate_issued: false,
        };

        env.storage()
            .persistent()
            .set(&DataKey::Engagement(id), &engagement);

        // Store milestones.
        let mut ms: Vec<MentoringMilestone> = Vec::new(&env);
        let mut idx: u32 = 0;
        for (title, desc) in milestones.iter() {
            ms.push_back(MentoringMilestone {
                index: idx,
                title,
                description: desc,
                status: MilestoneStatus::Pending,
                approved_at: 0,
            });
            idx += 1;
        }
        env.storage()
            .persistent()
            .set(&DataKey::Milestones(id), &ms);

        env.events()
            .publish((symbol_short!("proposed"),), (id, mentor, mentee));

        id
    }

    /// Mentee accepts the proposed engagement, activating it.
    pub fn accept_engagement(env: Env, mentee: Address, engagement_id: u64) {
        mentee.require_auth();
        let mut engagement = Self::load_engagement(&env, engagement_id);
        assert!(engagement.mentee == mentee, "caller is not the mentee");
        assert!(
            engagement.status == EngagementStatus::Proposed,
            "engagement is not in Proposed state"
        );

        engagement.status = EngagementStatus::Active;
        env.storage()
            .persistent()
            .set(&DataKey::Engagement(engagement_id), &engagement);

        env.events()
            .publish((symbol_short!("accepted"),), engagement_id);
    }

    /// Cancel an active or proposed engagement.
    pub fn cancel_engagement(env: Env, caller: Address, engagement_id: u64) {
        caller.require_auth();
        let mut engagement = Self::load_engagement(&env, engagement_id);
        assert!(
            caller == engagement.mentor || caller == engagement.mentee,
            "only mentor or mentee can cancel"
        );
        assert!(
            engagement.status == EngagementStatus::Proposed
                || engagement.status == EngagementStatus::Active,
            "cannot cancel a completed or already-cancelled engagement"
        );

        engagement.status = EngagementStatus::Cancelled;
        env.storage()
            .persistent()
            .set(&DataKey::Engagement(engagement_id), &engagement);

        env.events()
            .publish((symbol_short!("cancelled"),), (engagement_id, caller));
    }

    // ── Milestone Tracking ────────────────────────────────────────────────

    /// Mentee submits a milestone for mentor review.
    pub fn submit_milestone(env: Env, mentee: Address, engagement_id: u64, milestone_index: u32) {
        mentee.require_auth();
        let engagement = Self::load_engagement(&env, engagement_id);
        assert!(engagement.mentee == mentee, "caller is not the mentee");
        assert!(
            engagement.status == EngagementStatus::Active,
            "engagement is not active"
        );

        let mut milestones: Vec<MentoringMilestone> = env
            .storage()
            .persistent()
            .get(&DataKey::Milestones(engagement_id))
            .expect("milestones not found");

        let ms = milestones
            .get(milestone_index)
            .expect("milestone index out of range");
        assert!(
            ms.status == MilestoneStatus::Pending,
            "milestone already submitted or approved"
        );

        milestones.set(
            milestone_index,
            MentoringMilestone {
                status: MilestoneStatus::Submitted,
                ..ms
            },
        );
        env.storage()
            .persistent()
            .set(&DataKey::Milestones(engagement_id), &milestones);

        env.events().publish(
            (symbol_short!("ms_sub"), engagement_id),
            milestone_index,
        );
    }

    /// Mentor approves or rejects a submitted milestone.
    ///
    /// On approval of the final milestone the engagement automatically
    /// transitions to `Completed` and mentor compensation is released.
    pub fn review_milestone(
        env: Env,
        mentor: Address,
        engagement_id: u64,
        milestone_index: u32,
        approve: bool,
    ) {
        mentor.require_auth();
        let mut engagement = Self::load_engagement(&env, engagement_id);
        assert!(engagement.mentor == mentor, "caller is not the mentor");
        assert!(
            engagement.status == EngagementStatus::Active,
            "engagement is not active"
        );

        let mut milestones: Vec<MentoringMilestone> = env
            .storage()
            .persistent()
            .get(&DataKey::Milestones(engagement_id))
            .expect("milestones not found");

        let ms = milestones
            .get(milestone_index)
            .expect("milestone index out of range");
        assert!(
            ms.status == MilestoneStatus::Submitted,
            "milestone must be in Submitted state"
        );

        let new_status = if approve {
            MilestoneStatus::Approved
        } else {
            MilestoneStatus::Rejected
        };

        milestones.set(
            milestone_index,
            MentoringMilestone {
                status: new_status,
                approved_at: if approve { env.ledger().sequence() } else { 0 },
                ..ms
            },
        );
        env.storage()
            .persistent()
            .set(&DataKey::Milestones(engagement_id), &milestones);

        env.events().publish(
            (symbol_short!("ms_rev"), engagement_id),
            (milestone_index, approve),
        );

        // Check if all milestones are approved — auto-complete.
        if approve && milestones.iter().all(|m| m.status == MilestoneStatus::Approved) {
            engagement.status = EngagementStatus::Completed;
            engagement.completed_at = env.ledger().sequence();
            env.storage()
                .persistent()
                .set(&DataKey::Engagement(engagement_id), &engagement);

            // Release compensation to mentor.
            if engagement.compensation > 0 {
                let tok = token::Client::new(&env, &engagement.token);
                tok.transfer(
                    &env.current_contract_address(),
                    &engagement.mentor,
                    &engagement.compensation,
                );
            }

            env.events()
                .publish((symbol_short!("complete"),), engagement_id);
        }
    }

    // ── Feedback Collection ───────────────────────────────────────────────

    /// Submit feedback (rating 1–5) after an engagement.
    ///
    /// Either mentor or mentee may leave feedback once the engagement is
    /// completed or cancelled.
    pub fn submit_feedback(
        env: Env,
        author: Address,
        engagement_id: u64,
        rating: u32,
        comment: String,
    ) {
        author.require_auth();
        assert!(rating >= 1 && rating <= 5, "rating must be between 1 and 5");

        let engagement = Self::load_engagement(&env, engagement_id);
        assert!(
            author == engagement.mentor || author == engagement.mentee,
            "only mentor or mentee may submit feedback"
        );
        assert!(
            engagement.status == EngagementStatus::Completed
                || engagement.status == EngagementStatus::Cancelled,
            "feedback can only be submitted after the engagement ends"
        );

        let count: u64 = env
            .storage()
            .persistent()
            .get(&DataKey::FeedbackCount(engagement_id))
            .unwrap_or(0);

        let entry = FeedbackEntry {
            engagement_id,
            author: author.clone(),
            rating,
            comment: comment.clone(),
            ledger: env.ledger().sequence(),
        };

        env.storage()
            .persistent()
            .set(&DataKey::Feedback(engagement_id, count), &entry);
        env.storage()
            .persistent()
            .set(&DataKey::FeedbackCount(engagement_id), &(count + 1));

        env.events().publish(
            (symbol_short!("feedback"), engagement_id),
            (author, rating, comment),
        );
    }

    // ── Certification ─────────────────────────────────────────────────────

    /// Issue a completion certificate for a finished engagement.
    ///
    /// Any party (admin, mentor, or mentee) may trigger issuance once the
    /// engagement is complete.  Emits a `certified` event that off-chain
    /// services can use to mint an NFT or record the credential.
    pub fn issue_certificate(env: Env, caller: Address, engagement_id: u64) {
        caller.require_auth();
        let mut engagement = Self::load_engagement(&env, engagement_id);

        let admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .expect("contract not initialized");

        assert!(
            caller == admin
                || caller == engagement.mentor
                || caller == engagement.mentee,
            "unauthorized"
        );
        assert!(
            engagement.status == EngagementStatus::Completed,
            "engagement must be completed to issue a certificate"
        );
        assert!(
            !engagement.certificate_issued,
            "certificate already issued"
        );

        engagement.certificate_issued = true;
        env.storage()
            .persistent()
            .set(&DataKey::Engagement(engagement_id), &engagement);

        env.events().publish(
            (symbol_short!("certified"),),
            (engagement_id, engagement.mentee, engagement.mentor),
        );
    }

    // ── Queries ───────────────────────────────────────────────────────────

    /// Retrieve a full engagement record.
    pub fn get_engagement(env: Env, engagement_id: u64) -> MentoringEngagement {
        Self::load_engagement(&env, engagement_id)
    }

    /// Retrieve all milestones for an engagement.
    pub fn get_milestones(env: Env, engagement_id: u64) -> Vec<MentoringMilestone> {
        env.storage()
            .persistent()
            .get(&DataKey::Milestones(engagement_id))
            .expect("milestones not found")
    }

    /// Retrieve all feedback entries for an engagement.
    pub fn get_feedback(env: Env, engagement_id: u64) -> Vec<FeedbackEntry> {
        let count: u64 = env
            .storage()
            .persistent()
            .get(&DataKey::FeedbackCount(engagement_id))
            .unwrap_or(0);

        let mut result = Vec::new(&env);
        let mut i: u64 = 0;
        while i < count {
            if let Some(entry) = env
                .storage()
                .persistent()
                .get::<DataKey, FeedbackEntry>(&DataKey::Feedback(engagement_id, i))
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
            .get(&DataKey::EngagementCount)
            .unwrap_or(0);
        let next = count + 1;
        env.storage()
            .persistent()
            .set(&DataKey::EngagementCount, &next);
        next
    }

    fn load_engagement(env: &Env, engagement_id: u64) -> MentoringEngagement {
        env.storage()
            .persistent()
            .get(&DataKey::Engagement(engagement_id))
            .expect("engagement not found")
    }
}
