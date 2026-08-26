use soroban_sdk::contracterror;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum ReputationError {
    /// Contract not yet initialized.
    NotInitialized = 1,
    /// Contract already initialized.
    AlreadyInitialized = 2,
    /// Review not found.
    NotFound = 3,
    /// Review already exists from this client for this artist.
    DuplicateReview = 4,
    /// Rating value out of range (must be 1–100).
    InvalidRating = 5,
    /// Caller is not authorized for this operation.
    Unauthorized = 6,
    /// Review is already moderated / removed.
    AlreadyModerated = 7,
    /// Dispute already open for this review.
    DisputeAlreadyOpen = 8,
    /// No dispute is open for this review.
    NoOpenDispute = 9,
    /// Arithmetic overflow.
    ArithmeticOverflow = 10,
    // Review moderation & appeal — closes #604
    /// Report already submitted by this address for this review.
    DuplicateReport = 11,
    /// Appeal not found.
    AppealNotFound = 12,
    /// An appeal is already open for this review.
    AppealAlreadyOpen = 13,
    /// Appeal is not in a pending state.
    AppealNotPending = 14,
    /// Review is not in a moderatable state for this action.
    InvalidReviewState = 15,
}
