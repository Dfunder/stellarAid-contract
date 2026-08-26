#![no_std]
use soroban_sdk::{contract, contractimpl, symbol_short, Address, Bytes, Env, String, Vec};

pub mod errors;
pub mod types;

#[cfg(test)]
mod test;

use errors::RecruitmentError;
use types::{Application, DataKey, Job, JobStatus, Offer, Performance, Pipeline, Stage};

#[contract]
pub struct Recruitment;

fn has_admin(env: &Env) -> bool {
    env.storage().instance().has(&DataKey::Admin)
}

fn require_initialized(env: &Env) -> Result<(), RecruitmentError> {
    if has_admin(env) {
        Ok(())
    } else {
        Err(RecruitmentError::NotInitialized)
    }
}

fn load_job(env: &Env, job_id: &Bytes) -> Result<Job, RecruitmentError> {
    env.storage()
        .persistent()
        .get(&DataKey::Job(job_id.clone()))
        .ok_or(RecruitmentError::JobNotFound)
}

fn save_job(env: &Env, job: &Job) {
    env.storage()
        .persistent()
        .set(&DataKey::Job(job.id.clone()), job);
}

fn load_application(
    env: &Env,
    job_id: &Bytes,
    applicant: &Address,
) -> Result<Application, RecruitmentError> {
    env.storage()
        .persistent()
        .get(&DataKey::Application(job_id.clone(), applicant.clone()))
        .ok_or(RecruitmentError::ApplicationNotFound)
}

fn save_application(env: &Env, application: &Application) {
    env.storage().persistent().set(
        &DataKey::Application(application.job_id.clone(), application.applicant.clone()),
        application,
    );
}

fn load_pipeline(env: &Env, job_id: &Bytes) -> Pipeline {
    env.storage()
        .persistent()
        .get(&DataKey::Pipeline(job_id.clone()))
        .unwrap_or_default()
}

fn stage_slot(pipeline: &mut Pipeline, stage: Stage) -> &mut u32 {
    match stage {
        Stage::Applied => &mut pipeline.applied,
        Stage::Screening => &mut pipeline.screening,
        Stage::Interview => &mut pipeline.interview,
        Stage::Offered => &mut pipeline.offered,
        Stage::Hired => &mut pipeline.hired,
        Stage::Rejected => &mut pipeline.rejected,
        Stage::Withdrawn => &mut pipeline.withdrawn,
        Stage::Declined => &mut pipeline.declined,
    }
}

/// Move one applicant between stage buckets. `from` is `None` for a brand new
/// application, which only increments the destination.
fn shift_pipeline(env: &Env, job_id: &Bytes, from: Option<Stage>, to: Stage) {
    let mut pipeline = load_pipeline(env, job_id);
    if let Some(from) = from {
        let slot = stage_slot(&mut pipeline, from);
        *slot = slot.saturating_sub(1);
    }
    *stage_slot(&mut pipeline, to) += 1;
    env.storage()
        .persistent()
        .set(&DataKey::Pipeline(job_id.clone()), &pipeline);
}

/// Employer-driven transitions all share the same guard: the caller must own
/// the posting and the application must still be live.
fn require_employer(
    env: &Env,
    job_id: &Bytes,
    applicant: &Address,
) -> Result<(Job, Application), RecruitmentError> {
    require_initialized(env)?;
    let job = load_job(env, job_id)?;
    job.employer.require_auth();
    let application = load_application(env, job_id, applicant)?;
    if application.stage.is_terminal() {
        return Err(RecruitmentError::InvalidStage);
    }
    Ok((job, application))
}

fn move_stage(env: &Env, application: &mut Application, to: Stage) {
    shift_pipeline(env, &application.job_id, Some(application.stage), to);
    application.stage = to;
    application.updated_ledger = env.ledger().sequence();
    save_application(env, application);
}

#[contractimpl]
impl Recruitment {
    pub fn initialize(
        env: Env,
        admin: Address,
        max_applicants: u32,
    ) -> Result<(), RecruitmentError> {
        if has_admin(&env) {
            return Err(RecruitmentError::AlreadyInitialized);
        }
        if max_applicants == 0 {
            return Err(RecruitmentError::InvalidOpenings);
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage()
            .instance()
            .set(&DataKey::MaxApplicants, &max_applicants);
        env.events()
            .publish((symbol_short!("init"),), (admin, max_applicants));
        Ok(())
    }

    pub fn post_job(
        env: Env,
        job_id: Bytes,
        employer: Address,
        title: String,
        budget: i128,
        openings: u32,
    ) -> Result<(), RecruitmentError> {
        require_initialized(&env)?;
        employer.require_auth();
        if env
            .storage()
            .persistent()
            .has(&DataKey::Job(job_id.clone()))
        {
            return Err(RecruitmentError::JobExists);
        }
        if budget <= 0 {
            return Err(RecruitmentError::InvalidBudget);
        }
        if openings == 0 {
            return Err(RecruitmentError::InvalidOpenings);
        }
        let job = Job {
            id: job_id.clone(),
            employer: employer.clone(),
            title,
            budget,
            openings,
            filled: 0,
            applicant_count: 0,
            status: JobStatus::Open,
            posted_ledger: env.ledger().sequence(),
        };
        save_job(&env, &job);
        env.events()
            .publish((symbol_short!("posted"),), (job_id, employer, openings));
        Ok(())
    }

    pub fn close_job(env: Env, job_id: Bytes) -> Result<(), RecruitmentError> {
        require_initialized(&env)?;
        let mut job = load_job(&env, &job_id)?;
        job.employer.require_auth();
        if job.status != JobStatus::Open {
            return Err(RecruitmentError::JobNotOpen);
        }
        job.status = JobStatus::Closed;
        save_job(&env, &job);
        env.events().publish((symbol_short!("closed"),), job_id);
        Ok(())
    }

    pub fn apply_for_job(
        env: Env,
        job_id: Bytes,
        applicant: Address,
        proposal_uri: String,
        rate: i128,
    ) -> Result<(), RecruitmentError> {
        require_initialized(&env)?;
        applicant.require_auth();
        let mut job = load_job(&env, &job_id)?;
        if job.status != JobStatus::Open {
            return Err(RecruitmentError::JobNotOpen);
        }
        if job.employer == applicant {
            return Err(RecruitmentError::EmployerCannotApply);
        }
        if rate <= 0 {
            return Err(RecruitmentError::InvalidBudget);
        }
        if env
            .storage()
            .persistent()
            .has(&DataKey::Application(job_id.clone(), applicant.clone()))
        {
            return Err(RecruitmentError::AlreadyApplied);
        }
        let max_applicants: u32 = env
            .storage()
            .instance()
            .get(&DataKey::MaxApplicants)
            .unwrap_or(0);
        if job.applicant_count >= max_applicants {
            return Err(RecruitmentError::TooManyApplicants);
        }

        let ledger = env.ledger().sequence();
        let application = Application {
            job_id: job_id.clone(),
            applicant: applicant.clone(),
            proposal_uri,
            rate,
            stage: Stage::Applied,
            applied_ledger: ledger,
            updated_ledger: ledger,
        };
        save_application(&env, &application);

        let applicants_key = DataKey::Applicants(job_id.clone());
        let mut applicants: Vec<Address> = env
            .storage()
            .persistent()
            .get(&applicants_key)
            .unwrap_or_else(|| Vec::new(&env));
        applicants.push_back(applicant.clone());
        env.storage().persistent().set(&applicants_key, &applicants);

        job.applicant_count += 1;
        save_job(&env, &job);
        shift_pipeline(&env, &job_id, None, Stage::Applied);

        env.events()
            .publish((symbol_short!("applied"),), (job_id, applicant));
        Ok(())
    }

    /// Move an application forward through the screening funnel. Only
    /// `Screening` and `Interview` are reachable here; offers and hires have
    /// their own entry points so both sides have to sign.
    pub fn advance_application(
        env: Env,
        job_id: Bytes,
        applicant: Address,
        stage: Stage,
    ) -> Result<(), RecruitmentError> {
        let (_, mut application) = require_employer(&env, &job_id, &applicant)?;
        if !matches!(stage, Stage::Screening | Stage::Interview) {
            return Err(RecruitmentError::InvalidStage);
        }
        let (current, target) = match (application.stage.rank(), stage.rank()) {
            (Some(current), Some(target)) => (current, target),
            _ => return Err(RecruitmentError::InvalidStage),
        };
        if target <= current {
            return Err(RecruitmentError::InvalidStage);
        }
        move_stage(&env, &mut application, stage);
        env.events()
            .publish((symbol_short!("advanced"),), (job_id, applicant, stage));
        Ok(())
    }

    pub fn make_offer(
        env: Env,
        job_id: Bytes,
        applicant: Address,
        rate: i128,
        start_ledger: u32,
    ) -> Result<(), RecruitmentError> {
        let (job, mut application) = require_employer(&env, &job_id, &applicant)?;
        if job.status != JobStatus::Open {
            return Err(RecruitmentError::JobNotOpen);
        }
        if application.stage == Stage::Offered {
            return Err(RecruitmentError::InvalidStage);
        }
        if rate <= 0 {
            return Err(RecruitmentError::InvalidBudget);
        }
        env.storage().persistent().set(
            &DataKey::Offer(job_id.clone(), applicant.clone()),
            &Offer {
                rate,
                start_ledger,
                made_ledger: env.ledger().sequence(),
            },
        );
        move_stage(&env, &mut application, Stage::Offered);
        env.events()
            .publish((symbol_short!("offered"),), (job_id, applicant, rate));
        Ok(())
    }

    /// Accepting an offer closes the loop: the applicant is hired and the
    /// posting is marked filled once every opening is taken.
    pub fn accept_offer(
        env: Env,
        job_id: Bytes,
        applicant: Address,
    ) -> Result<(), RecruitmentError> {
        require_initialized(&env)?;
        applicant.require_auth();
        let mut job = load_job(&env, &job_id)?;
        let mut application = load_application(&env, &job_id, &applicant)?;
        if application.stage != Stage::Offered {
            return Err(RecruitmentError::InvalidStage);
        }
        move_stage(&env, &mut application, Stage::Hired);
        job.filled += 1;
        if job.filled >= job.openings {
            job.status = JobStatus::Filled;
        }
        save_job(&env, &job);
        env.events()
            .publish((symbol_short!("hired"),), (job_id, applicant));
        Ok(())
    }

    pub fn decline_offer(
        env: Env,
        job_id: Bytes,
        applicant: Address,
    ) -> Result<(), RecruitmentError> {
        require_initialized(&env)?;
        applicant.require_auth();
        let mut application = load_application(&env, &job_id, &applicant)?;
        if application.stage != Stage::Offered {
            return Err(RecruitmentError::InvalidStage);
        }
        move_stage(&env, &mut application, Stage::Declined);
        env.events()
            .publish((symbol_short!("declined"),), (job_id, applicant));
        Ok(())
    }

    pub fn withdraw_application(
        env: Env,
        job_id: Bytes,
        applicant: Address,
    ) -> Result<(), RecruitmentError> {
        require_initialized(&env)?;
        applicant.require_auth();
        let mut application = load_application(&env, &job_id, &applicant)?;
        if application.stage.is_terminal() {
            return Err(RecruitmentError::InvalidStage);
        }
        move_stage(&env, &mut application, Stage::Withdrawn);
        env.events()
            .publish((symbol_short!("withdrawn"),), (job_id, applicant));
        Ok(())
    }

    pub fn reject_application(
        env: Env,
        job_id: Bytes,
        applicant: Address,
    ) -> Result<(), RecruitmentError> {
        let (_, mut application) = require_employer(&env, &job_id, &applicant)?;
        move_stage(&env, &mut application, Stage::Rejected);
        env.events()
            .publish((symbol_short!("rejected"),), (job_id, applicant));
        Ok(())
    }

    /// Post-hire review. Ratings accumulate so a hire's track record on the
    /// posting can be read back as an average.
    pub fn record_performance(
        env: Env,
        job_id: Bytes,
        applicant: Address,
        rating: u32,
        note: String,
    ) -> Result<(), RecruitmentError> {
        require_initialized(&env)?;
        let job = load_job(&env, &job_id)?;
        job.employer.require_auth();
        if !(1..=5).contains(&rating) {
            return Err(RecruitmentError::InvalidRating);
        }
        let application = load_application(&env, &job_id, &applicant)?;
        if application.stage != Stage::Hired {
            return Err(RecruitmentError::NotHired);
        }
        let key = DataKey::Performance(job_id.clone(), applicant.clone());
        let mut performance: Performance =
            env.storage().persistent().get(&key).unwrap_or(Performance {
                reviews: 0,
                total_rating: 0,
                last_rating: 0,
                last_ledger: 0,
                last_note: String::from_str(&env, ""),
            });
        performance.reviews += 1;
        performance.total_rating += rating;
        performance.last_rating = rating;
        performance.last_ledger = env.ledger().sequence();
        performance.last_note = note;
        env.storage().persistent().set(&key, &performance);
        env.events()
            .publish((symbol_short!("perf"),), (job_id, applicant, rating));
        Ok(())
    }

    pub fn get_job(env: Env, job_id: Bytes) -> Result<Job, RecruitmentError> {
        load_job(&env, &job_id)
    }

    pub fn get_application(
        env: Env,
        job_id: Bytes,
        applicant: Address,
    ) -> Result<Application, RecruitmentError> {
        load_application(&env, &job_id, &applicant)
    }

    /// The standing offer for an applicant, if one has been made. Offers are
    /// kept beside the application so a declined offer stays auditable.
    pub fn get_offer(env: Env, job_id: Bytes, applicant: Address) -> Option<Offer> {
        env.storage()
            .persistent()
            .get(&DataKey::Offer(job_id, applicant))
    }

    pub fn get_applicants(env: Env, job_id: Bytes) -> Vec<Address> {
        env.storage()
            .persistent()
            .get(&DataKey::Applicants(job_id))
            .unwrap_or_else(|| Vec::new(&env))
    }

    pub fn get_pipeline(env: Env, job_id: Bytes) -> Pipeline {
        load_pipeline(&env, &job_id)
    }

    pub fn get_performance(env: Env, job_id: Bytes, applicant: Address) -> Option<Performance> {
        env.storage()
            .persistent()
            .get(&DataKey::Performance(job_id, applicant))
    }

    /// Average rating scaled by 100 to keep one hundredth of a star of
    /// precision without floating point. Zero when there are no reviews.
    pub fn get_average_rating(env: Env, job_id: Bytes, applicant: Address) -> u32 {
        match env
            .storage()
            .persistent()
            .get::<DataKey, Performance>(&DataKey::Performance(job_id, applicant))
        {
            Some(performance) if performance.reviews > 0 => {
                performance.total_rating * 100 / performance.reviews
            }
            _ => 0,
        }
    }
}
