extern crate std;
use soroban_sdk::{testutils::Address as _, Address, Bytes, Env, String};

use crate::errors::RecruitmentError;
use crate::types::{JobStatus, Stage};
use crate::{Recruitment, RecruitmentClient};

const MAX_APPLICANTS: u32 = 3;

struct Fixture<'a> {
    env: Env,
    client: RecruitmentClient<'a>,
    employer: Address,
    applicant: Address,
    other: Address,
    job_id: Bytes,
}

fn setup<'a>() -> Fixture<'a> {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let employer = Address::generate(&env);
    let applicant = Address::generate(&env);
    let other = Address::generate(&env);
    let contract_id = env.register_contract(None, Recruitment);
    let client = RecruitmentClient::new(&env, &contract_id);
    client.initialize(&admin, &MAX_APPLICANTS);
    Fixture {
        job_id: Bytes::from_slice(&env, b"job-001"),
        env,
        client,
        employer,
        applicant,
        other,
    }
}

impl Fixture<'_> {
    fn post(&self, openings: u32) {
        self.client.post_job(
            &self.job_id,
            &self.employer,
            &String::from_str(&self.env, "Album cover illustrator"),
            &5_000,
            &openings,
        );
    }

    fn apply(&self, applicant: &Address) {
        self.client.apply_for_job(
            &self.job_id,
            applicant,
            &String::from_str(&self.env, "ipfs://proposal"),
            &1_200,
        );
    }

    fn note(&self) -> String {
        String::from_str(&self.env, "delivered on time")
    }
}

#[test]
fn post_job_creates_open_posting() {
    let f = setup();
    f.post(1);
    let job = f.client.get_job(&f.job_id);
    assert_eq!(job.employer, f.employer);
    assert_eq!(job.status, JobStatus::Open);
    assert_eq!(job.openings, 1);
    assert_eq!(job.applicant_count, 0);
}

#[test]
fn duplicate_job_id_is_rejected() {
    let f = setup();
    f.post(1);
    let err = f
        .client
        .try_post_job(
            &f.job_id,
            &f.employer,
            &String::from_str(&f.env, "duplicate"),
            &5_000,
            &1,
        )
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, RecruitmentError::JobExists);
}

#[test]
fn invalid_posting_terms_are_rejected() {
    let f = setup();
    let title = String::from_str(&f.env, "bad posting");
    assert_eq!(
        f.client
            .try_post_job(&f.job_id, &f.employer, &title, &0, &1)
            .err()
            .unwrap()
            .unwrap(),
        RecruitmentError::InvalidBudget
    );
    assert_eq!(
        f.client
            .try_post_job(&f.job_id, &f.employer, &title, &5_000, &0)
            .err()
            .unwrap()
            .unwrap(),
        RecruitmentError::InvalidOpenings
    );
}

#[test]
fn applying_tracks_the_applicant() {
    let f = setup();
    f.post(1);
    f.apply(&f.applicant);

    let application = f.client.get_application(&f.job_id, &f.applicant);
    assert_eq!(application.stage, Stage::Applied);
    assert_eq!(application.rate, 1_200);
    assert!(f.client.get_offer(&f.job_id, &f.applicant).is_none());
    assert_eq!(f.client.get_job(&f.job_id).applicant_count, 1);
    assert_eq!(f.client.get_applicants(&f.job_id).len(), 1);
    assert_eq!(f.client.get_pipeline(&f.job_id).applied, 1);
}

#[test]
fn duplicate_and_self_applications_are_rejected() {
    let f = setup();
    f.post(1);
    f.apply(&f.applicant);
    assert_eq!(
        f.client
            .try_apply_for_job(
                &f.job_id,
                &f.applicant,
                &String::from_str(&f.env, "ipfs://again"),
                &1_000
            )
            .err()
            .unwrap()
            .unwrap(),
        RecruitmentError::AlreadyApplied
    );
    assert_eq!(
        f.client
            .try_apply_for_job(
                &f.job_id,
                &f.employer,
                &String::from_str(&f.env, "ipfs://self"),
                &1_000
            )
            .err()
            .unwrap()
            .unwrap(),
        RecruitmentError::EmployerCannotApply
    );
}

#[test]
fn applicant_limit_is_enforced() {
    let f = setup();
    f.post(1);
    for _ in 0..MAX_APPLICANTS {
        f.apply(&Address::generate(&f.env));
    }
    let err = f
        .client
        .try_apply_for_job(
            &f.job_id,
            &f.applicant,
            &String::from_str(&f.env, "ipfs://late"),
            &900,
        )
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, RecruitmentError::TooManyApplicants);
}

#[test]
fn closed_job_stops_accepting_applications() {
    let f = setup();
    f.post(1);
    f.client.close_job(&f.job_id);
    assert_eq!(f.client.get_job(&f.job_id).status, JobStatus::Closed);
    let err = f
        .client
        .try_apply_for_job(
            &f.job_id,
            &f.applicant,
            &String::from_str(&f.env, "ipfs://late"),
            &900,
        )
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, RecruitmentError::JobNotOpen);
}

#[test]
fn pipeline_follows_the_full_hire_flow() {
    let f = setup();
    f.post(1);
    f.apply(&f.applicant);

    f.client
        .advance_application(&f.job_id, &f.applicant, &Stage::Screening);
    assert_eq!(f.client.get_pipeline(&f.job_id).screening, 1);
    assert_eq!(f.client.get_pipeline(&f.job_id).applied, 0);

    f.client
        .advance_application(&f.job_id, &f.applicant, &Stage::Interview);
    f.client.make_offer(&f.job_id, &f.applicant, &1_500, &900);

    let application = f.client.get_application(&f.job_id, &f.applicant);
    assert_eq!(application.stage, Stage::Offered);
    assert_eq!(
        f.client.get_offer(&f.job_id, &f.applicant).unwrap().rate,
        1_500
    );

    f.client.accept_offer(&f.job_id, &f.applicant);
    let pipeline = f.client.get_pipeline(&f.job_id);
    assert_eq!(pipeline.hired, 1);
    assert_eq!(pipeline.offered, 0);

    let job = f.client.get_job(&f.job_id);
    assert_eq!(job.filled, 1);
    assert_eq!(job.status, JobStatus::Filled);
}

#[test]
fn pipeline_cannot_move_backwards() {
    let f = setup();
    f.post(1);
    f.apply(&f.applicant);
    f.client
        .advance_application(&f.job_id, &f.applicant, &Stage::Interview);
    let err = f
        .client
        .try_advance_application(&f.job_id, &f.applicant, &Stage::Screening)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, RecruitmentError::InvalidStage);
}

#[test]
fn advance_cannot_skip_straight_to_hired() {
    let f = setup();
    f.post(1);
    f.apply(&f.applicant);
    let err = f
        .client
        .try_advance_application(&f.job_id, &f.applicant, &Stage::Hired)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, RecruitmentError::InvalidStage);
}

#[test]
fn offers_can_be_declined_and_applications_withdrawn() {
    let f = setup();
    f.post(2);
    f.apply(&f.applicant);
    f.apply(&f.other);

    f.client.make_offer(&f.job_id, &f.applicant, &1_500, &900);
    f.client.decline_offer(&f.job_id, &f.applicant);
    assert_eq!(
        f.client.get_application(&f.job_id, &f.applicant).stage,
        Stage::Declined
    );

    f.client.withdraw_application(&f.job_id, &f.other);
    let pipeline = f.client.get_pipeline(&f.job_id);
    assert_eq!(pipeline.declined, 1);
    assert_eq!(pipeline.withdrawn, 1);
    assert_eq!(pipeline.applied, 0);
    assert_eq!(f.client.get_job(&f.job_id).status, JobStatus::Open);
}

#[test]
fn terminal_applications_are_frozen() {
    let f = setup();
    f.post(1);
    f.apply(&f.applicant);
    f.client.reject_application(&f.job_id, &f.applicant);
    assert_eq!(f.client.get_pipeline(&f.job_id).rejected, 1);
    let err = f
        .client
        .try_advance_application(&f.job_id, &f.applicant, &Stage::Screening)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, RecruitmentError::InvalidStage);
}

#[test]
fn performance_is_tracked_only_after_hire() {
    let f = setup();
    f.post(1);
    f.apply(&f.applicant);

    let err = f
        .client
        .try_record_performance(&f.job_id, &f.applicant, &5, &f.note())
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, RecruitmentError::NotHired);

    f.client.make_offer(&f.job_id, &f.applicant, &1_500, &900);
    f.client.accept_offer(&f.job_id, &f.applicant);

    f.client
        .record_performance(&f.job_id, &f.applicant, &5, &f.note());
    f.client
        .record_performance(&f.job_id, &f.applicant, &4, &f.note());

    let performance = f.client.get_performance(&f.job_id, &f.applicant).unwrap();
    assert_eq!(performance.reviews, 2);
    assert_eq!(performance.total_rating, 9);
    assert_eq!(performance.last_rating, 4);
    // (5 + 4) / 2 = 4.5 stars, scaled by 100.
    assert_eq!(f.client.get_average_rating(&f.job_id, &f.applicant), 450);
}

#[test]
fn ratings_outside_the_scale_are_rejected() {
    let f = setup();
    f.post(1);
    f.apply(&f.applicant);
    f.client.make_offer(&f.job_id, &f.applicant, &1_500, &900);
    f.client.accept_offer(&f.job_id, &f.applicant);
    for rating in [0u32, 6u32] {
        let err = f
            .client
            .try_record_performance(&f.job_id, &f.applicant, &rating, &f.note())
            .err()
            .unwrap()
            .unwrap();
        assert_eq!(err, RecruitmentError::InvalidRating);
    }
}

#[test]
fn unknown_job_and_application_are_reported() {
    let f = setup();
    let missing = Bytes::from_slice(&f.env, b"job-999");
    assert_eq!(
        f.client.try_get_job(&missing).err().unwrap().unwrap(),
        RecruitmentError::JobNotFound
    );
    f.post(1);
    assert_eq!(
        f.client
            .try_get_application(&f.job_id, &f.applicant)
            .err()
            .unwrap()
            .unwrap(),
        RecruitmentError::ApplicationNotFound
    );
    assert_eq!(f.client.get_average_rating(&f.job_id, &f.applicant), 0);
}
