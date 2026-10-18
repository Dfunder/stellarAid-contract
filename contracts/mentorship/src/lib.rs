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
    contract, contracterror, contractimpl, contracttype, symbol_short, token, Address, Env, String, Vec,
};

/// Typed errors returned by the contract entry points.
///
/// These replace the `assert!`/`expect` panics that previously guarded the
/// entry points, so a caller can handle each failure programmatically.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum MentorshipError {
    /// The contract has not been initialized (no admin).
    NotInitialized = 1,
    /// No engagement exists for the given id.
    EngagementNotFound = 2,
    /// No milestones are stored for the engagement.
    MilestonesNotFound = 3,
    /// The milestone index is out of range.
    MilestoneOutOfRange = 4,
    /// Compensation must be non-negative.
    InvalidCompensation = 5,
    /// At least one milestone is required.
    NoMilestones = 6,
    /// The caller is not the mentee.
    NotMentee = 7,
    /// The caller is not the mentor.
    NotMentor = 8,
    /// The caller is not permitted to perform this action.
    Unauthorized = 9,
    /// The engagement is not in a state that allows this action.
    InvalidStatus = 10,
    /// The milestone is not in a state that allows this action.
    InvalidMilestoneStatus = 11,
    /// Rating must be between 1 and 5.
    InvalidRating = 12,
    /// A certificate has already been issued.
    CertificateAlreadyIssued = 13,
}

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
    ) -> Result<u64, MentorshipError> {
        mentor.require_auth();
        if !(compensation >= 0) {
 return Err(MentorshipError::InvalidCompensation);
 }
        if !(!milestones.is_empty()) {
 return Err(MentorshipError::NoMilestones);
 }

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
        for (idx, (title, desc)) in (0_u32..).zip(milestones.iter()) {
            ms.push_back(MentoringMilestone {
                index: idx,
                title,
                description: desc,
                status: MilestoneStatus::Pending,
                approved_at: 0,
            });
        }
        env.storage()
            .persistent()
            .set(&DataKey::Milestones(id), &ms);

        env.events()
            .publish((symbol_short!("proposed"),), (id, mentor, mentee));

        Ok(id)
    }

    /// Mentee accepts the proposed engagement, activating it.
    pub fn accept_engagement(env: Env, mentee: Address, engagement_id: u64) -> Result<(), MentorshipError> {
        mentee.require_auth();
        let mut engagement = Self::load_engagement(&env, engagement_id)?;
        if !(engagement.mentee == mentee) {
 return Err(MentorshipError::NotMentee);
 }
        if !(engagement.status == EngagementStatus::Proposed) {
 return Err(MentorshipError::InvalidStatus);
 }

        engagement.status = EngagementStatus::Active;
        env.storage()
            .persistent()
            .set(&DataKey::Engagement(engagement_id), &engagement);

        env.events()
            .publish((symbol_short!("accepted"),), engagement_id);
        Ok(())
    }

    /// Cancel an active or proposed engagement.
    pub fn cancel_engagement(env: Env, caller: Address, engagement_id: u64) -> Result<(), MentorshipError> {
        caller.require_auth();
        let mut engagement = Self::load_engagement(&env, engagement_id)?;
        if !(caller == engagement.mentor || caller == engagement.mentee) {
 return Err(MentorshipError::Unauthorized);
 }
        if !(engagement.status == EngagementStatus::Proposed || engagement.status == EngagementStatus::Active) {
 return Err(MentorshipError::InvalidStatus);
 }

        engagement.status = EngagementStatus::Cancelled;
        env.storage()
            .persistent()
            .set(&DataKey::Engagement(engagement_id), &engagement);

        env.events()
            .publish((symbol_short!("cancelled"),), (engagement_id, caller));
        Ok(())
    }

    // ── Milestone Tracking ────────────────────────────────────────────────

    /// Mentee submits a milestone for mentor review.
    pub fn submit_milestone(env: Env, mentee: Address, engagement_id: u64, milestone_index: u32) -> Result<(), MentorshipError> {
        shared::guard::with_reentrancy_guard(&env, || -> Result<(), MentorshipError> {
            mentee.require_auth();
            let engagement = Self::load_engagement(&env, engagement_id)?;
            if !(engagement.mentee == mentee) {
 return Err(MentorshipError::NotMentee);
 }
            if !(engagement.status == EngagementStatus::Active) {
 return Err(MentorshipError::InvalidStatus);
 }

            let mut milestones: Vec<MentoringMilestone> = env
                .storage()
                .persistent()
                .get(&DataKey::Milestones(engagement_id))
                .ok_or(MentorshipError::MilestonesNotFound)?;

            let ms = milestones
                .get(milestone_index)
                .ok_or(MentorshipError::MilestoneOutOfRange)?;
            if !(ms.status == MilestoneStatus::Pending) {
 return Err(MentorshipError::InvalidMilestoneStatus);
 }

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
            Ok(())
        })
    }

    /// Mentor approves or rejects a submitted milestone.
    ///
    /// On approval of the final milestone the engagement automatically
    /// transitions to `Completed` and mentor compensation is released.
    ///
    /// Follows Checks-Effects-Interactions (CEI): milestone and engagement states
    /// are updated and persisted before any external token transfer is initiated.
    /// Guarded by `shared::guard::with_reentrancy_guard` against reentrant calls.
    pub fn review_milestone(
        env: Env,
        mentor: Address,
        engagement_id: u64,
        milestone_index: u32,
        approve: bool,
    ) -> Result<(), MentorshipError> {
        shared::guard::with_reentrancy_guard(&env, || -> Result<(), MentorshipError> {
            // Checks
            mentor.require_auth();
            let mut engagement = Self::load_engagement(&env, engagement_id)?;
            if !(engagement.mentor == mentor) {
 return Err(MentorshipError::NotMentor);
 }
            if !(engagement.status == EngagementStatus::Active) {
 return Err(MentorshipError::InvalidStatus);
 }

            let mut milestones: Vec<MentoringMilestone> = env
                .storage()
                .persistent()
                .get(&DataKey::Milestones(engagement_id))
                .ok_or(MentorshipError::MilestonesNotFound)?;

            let ms = milestones
                .get(milestone_index)
                .ok_or(MentorshipError::MilestoneOutOfRange)?;
            if !(ms.status == MilestoneStatus::Submitted) {
 return Err(MentorshipError::InvalidMilestoneStatus);
 }

            let new_status = if approve {
                MilestoneStatus::Approved
            } else {
                MilestoneStatus::Rejected
            };

            // Effects: update milestone status and persist
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
                // Effects: update engagement status and persist before any external calls
                engagement.status = EngagementStatus::Completed;
                engagement.completed_at = env.ledger().sequence();
                env.storage()
                    .persistent()
                    .set(&DataKey::Engagement(engagement_id), &engagement);

                env.events()
                    .publish((symbol_short!("complete"),), engagement_id);

                // Interactions: external token transfer happens strictly AFTER all effects
                if engagement.compensation > 0 {
                    let tok = token::Client::new(&env, &engagement.token);
                    tok.transfer(
                        &env.current_contract_address(),
                        &engagement.mentor,
                        &engagement.compensation,
                    );
                }
            }
            Ok(())
        })
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
    ) -> Result<(), MentorshipError> {
        author.require_auth();
        if !((1..=5).contains(&rating)) {
 return Err(MentorshipError::InvalidRating);
 }

        let engagement = Self::load_engagement(&env, engagement_id)?;
        if !(author == engagement.mentor || author == engagement.mentee) {
 return Err(MentorshipError::Unauthorized);
 }
        if !(engagement.status == EngagementStatus::Completed || engagement.status == EngagementStatus::Cancelled) {
 return Err(MentorshipError::InvalidStatus);
 }

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
        Ok(())
    }

    // ── Certification ─────────────────────────────────────────────────────

    /// Issue a completion certificate for a finished engagement.
    ///
    /// Any party (admin, mentor, or mentee) may trigger issuance once the
    /// engagement is complete.  Emits a `certified` event that off-chain
    /// services can use to mint an NFT or record the credential.
    pub fn issue_certificate(env: Env, caller: Address, engagement_id: u64) -> Result<(), MentorshipError> {
        caller.require_auth();
        let mut engagement = Self::load_engagement(&env, engagement_id)?;

        let admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .ok_or(MentorshipError::NotInitialized)?;

        if !(caller == admin || caller == engagement.mentor || caller == engagement.mentee) {
 return Err(MentorshipError::Unauthorized);
 }
        if !(engagement.status == EngagementStatus::Completed) {
 return Err(MentorshipError::InvalidStatus);
 }
        if !(!engagement.certificate_issued) {
 return Err(MentorshipError::CertificateAlreadyIssued);
 }

        engagement.certificate_issued = true;
        env.storage()
            .persistent()
            .set(&DataKey::Engagement(engagement_id), &engagement);

        env.events().publish(
            (symbol_short!("certified"),),
            (engagement_id, engagement.mentee, engagement.mentor),
        );
        Ok(())
    }

    // ── Queries ───────────────────────────────────────────────────────────

    /// Retrieve a full engagement record.
    pub fn get_engagement(env: Env, engagement_id: u64) -> Result<MentoringEngagement, MentorshipError> {
        Self::load_engagement(&env, engagement_id)
    }

    /// Retrieve all milestones for an engagement.
    ///
    /// Unbounded: this returns the whole stored vector, and the milestone list
    /// is supplied by the proposer, so its size is caller-controlled. Prefer
    /// [`get_milestones_page`](Self::get_milestones_page), which bounds the
    /// response with `shared::pagination`. Kept for callers that already rely
    /// on the unbounded shape.
    pub fn get_milestones(env: Env, engagement_id: u64) -> Result<Vec<MentoringMilestone>, MentorshipError> {
        env.storage()
            .persistent()
            .get(&DataKey::Milestones(engagement_id))
            .ok_or(MentorshipError::MilestonesNotFound)
    }

    /// Retrieve a bounded page of an engagement's milestones, plus its
    /// `PageInfo`.
    ///
    /// A page spans at most `shared::pagination::MAX_LIMIT` milestones however
    /// large `limit` is. A `limit` of `0` yields the default page size rather
    /// than an empty result, and an `offset` past the end is an empty page
    /// rather than an error.
    pub fn get_milestones_page(
        env: Env,
        engagement_id: u64,
        offset: u32,
        limit: u32,
    ) -> (Vec<MentoringMilestone>, shared::pagination::PageInfo) {
        let milestones: Vec<MentoringMilestone> = env
            .storage()
            .persistent()
            .get(&DataKey::Milestones(engagement_id))
            .unwrap_or_else(|| Vec::new(&env));
        shared::pagination::paginated(&env, &milestones, offset, limit)
    }

    /// Retrieve feedback entries for an engagement, starting at the beginning.
    ///
    /// Behaviour change (closes #876): this used to walk the engagement's
    /// entire feedback log, which is never pruned, so its cost grew for the
    /// life of the engagement. It now returns at most
    /// `shared::pagination::MAX_LIMIT` entries. Nothing is lost — the entries
    /// are still readable — but a caller that needs more must page. Use
    /// [`get_feedback_page`](Self::get_feedback_page) to do that, since it also
    /// reports the total and the next cursor.
    pub fn get_feedback(env: Env, engagement_id: u64) -> Vec<FeedbackEntry> {
        Self::feedback_page(&env, engagement_id, 0, shared::pagination::MAX_LIMIT as u64).0
    }

    /// Like [`get_feedback`](Self::get_feedback), but returns a bounded page of
    /// the feedback log together with its `PageInfo`: the total entry count, the
    /// next cursor, and whether anything exists past this page.
    ///
    /// A page reads at most `shared::pagination::MAX_LIMIT` storage entries
    /// however large `limit` is. A `limit` of `0` yields the default page size
    /// rather than an empty result, and an `offset` past the end is an empty
    /// page rather than an error, so a client can page to the end without
    /// reading a count first.
    pub fn get_feedback_page(
        env: Env,
        engagement_id: u64,
        offset: u64,
        limit: u64,
    ) -> (Vec<FeedbackEntry>, shared::pagination::PageInfo) {
        Self::feedback_page(&env, engagement_id, offset, limit)
    }

    // ── Internal ──────────────────────────────────────────────────────────

    /// Shared body of the two feedback readers, bounded by
    /// `shared::pagination::collect_window` (closes #876).
    ///
    /// `FeedbackCount` and `DataKey::Feedback` are keyed by `u64`, while the
    /// pagination arithmetic is `u32`. The narrowing conversions saturate
    /// rather than wrap: a log of `u32::MAX` entries is already far past
    /// anything a 100-entry page can address, and wrapping would report a
    /// small `total` for a very long log and mis-page the client.
    fn feedback_page(
        env: &Env,
        engagement_id: u64,
        offset: u64,
        limit: u64,
    ) -> (Vec<FeedbackEntry>, shared::pagination::PageInfo) {
        let count: u64 = env
            .storage()
            .persistent()
            .get(&DataKey::FeedbackCount(engagement_id))
            .unwrap_or(0);

        shared::pagination::collect_window(
            env,
            u32::try_from(count).unwrap_or(u32::MAX),
            u32::try_from(offset).unwrap_or(u32::MAX),
            u32::try_from(limit).unwrap_or(u32::MAX),
            |i| {
                env.storage()
                    .persistent()
                    .get::<DataKey, FeedbackEntry>(&DataKey::Feedback(engagement_id, u64::from(i)))
            },
        )
    }

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

    fn load_engagement(env: &Env, engagement_id: u64) -> Result<MentoringEngagement, MentorshipError> {
        env.storage()
            .persistent()
            .get(&DataKey::Engagement(engagement_id))
            .ok_or(MentorshipError::EngagementNotFound)
    }
}

#[cfg(test)]
mod test;

