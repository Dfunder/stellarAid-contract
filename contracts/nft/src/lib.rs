// contracts/nft/src/lib.rs
// Implements NFT Ownership Certificates — closes #614
//
// Acceptance Criteria:
// - Support NFT minting for completed commissions
// - Implement royalty tracking
// - Add secondary sale support
// - Track NFT ownership history
// - Implement NFT burn/transfer restrictions

#![no_std]

use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short, token, Address, Env, String, Vec,
};

// ── Data Types ────────────────────────────────────────────────────────────────

/// Current state of an NFT certificate.
#[contracttype]
#[derive(Clone, PartialEq)]
pub enum NftStatus {
    Active,
    Burned,
}

/// An ownership transfer record (provenance entry).
#[contracttype]
#[derive(Clone)]
pub struct TransferRecord {
    pub nft_id: u64,
    pub from: Address,
    pub to: Address,
    pub price: i128,
    pub ledger: u32,
}

/// The core NFT certificate record stored on-chain.
#[contracttype]
#[derive(Clone)]
pub struct NftCertificate {
    /// Unique NFT identifier.
    pub id: u64,
    /// Title / name of the certificate.
    pub title: String,
    /// Off-chain metadata URI (e.g. IPFS CID).
    pub metadata_uri: String,
    /// Address that originally created the certificate (the artist).
    pub creator: Address,
    /// Current owner.
    pub owner: Address,
    /// Royalty in basis points paid to `creator` on secondary sales (max 3000 = 30%).
    pub royalty_bps: u32,
    /// ID of the commission this certificate is linked to (0 = standalone).
    pub commission_id: u64,
    /// Whether the certificate can be transferred.
    pub transferable: bool,
    /// Whether the certificate can be burned (destroyed).
    pub burnable: bool,
    /// Current status.
    pub status: NftStatus,
    /// Ledger when minted.
    pub minted_at: u32,
}

// ── Storage Keys ──────────────────────────────────────────────────────────────

#[contracttype]
pub enum DataKey {
    /// NFT record.
    Nft(u64),
    /// Monotonic NFT id counter.
    NftCount,
    /// Transfer history count for an NFT.
    TransferCount(u64),
    /// Individual transfer record.
    Transfer(u64, u64),
    /// Admin address.
    Admin,
    /// Token contract used for royalty payments.
    RoyaltyToken,
}

// ── Contract ──────────────────────────────────────────────────────────────────

#[contract]
pub struct NftOwnership;

#[contractimpl]
impl NftOwnership {
    // ── Admin ─────────────────────────────────────────────────────────────

    /// Initialise the NFT contract.
    ///
    /// # Arguments
    /// * `admin`         — privileged admin address
    /// * `royalty_token` — token contract used for royalty settlement
    pub fn initialize(env: Env, admin: Address, royalty_token: Address) {
        admin.require_auth();
        env.storage().persistent().set(&DataKey::Admin, &admin);
        env.storage().persistent().set(&DataKey::RoyaltyToken, &royalty_token);
        env.storage().persistent().set(&DataKey::NftCount, &0u64);
        env.events().publish((symbol_short!("init"),), admin);
    }

    // ── Minting ───────────────────────────────────────────────────────────

    /// Mint a new NFT ownership certificate.
    ///
    /// Typically called by the admin (platform) on behalf of a creator when a
    /// commission is completed, though the creator may also call directly.
    ///
    /// # Arguments
    /// * `creator`       — artist who created the work. Must sign.
    /// * `owner`         — initial owner (may differ from creator on primary sale).
    /// * `title`         — human-readable certificate name.
    /// * `metadata_uri`  — URI pointing to off-chain metadata (IPFS / HTTPS).
    /// * `royalty_bps`   — basis points paid to creator on secondary sales (≤ 3000).
    /// * `commission_id` — linked commission id; 0 if standalone.
    /// * `transferable`  — whether the NFT may be transferred after minting.
    /// * `burnable`      — whether the NFT may be burned by the owner.
    ///
    /// Returns the new `nft_id`.
    pub fn mint(
        env: Env,
        creator: Address,
        owner: Address,
        title: String,
        metadata_uri: String,
        royalty_bps: u32,
        commission_id: u64,
        transferable: bool,
        burnable: bool,
    ) -> u64 {
        creator.require_auth();
        assert!(royalty_bps <= 3000, "royalty_bps must not exceed 3000 (30%)");

        let id = Self::next_id(&env);

        let nft = NftCertificate {
            id,
            title: title.clone(),
            metadata_uri,
            creator: creator.clone(),
            owner: owner.clone(),
            royalty_bps,
            commission_id,
            transferable,
            burnable,
            status: NftStatus::Active,
            minted_at: env.ledger().sequence(),
        };

        env.storage().persistent().set(&DataKey::Nft(id), &nft);
        env.storage().persistent().set(&DataKey::TransferCount(id), &0u64);

        env.events().publish(
            (symbol_short!("minted"), title),
            (id, creator, owner),
        );

        id
    }

    // ── Transfer ──────────────────────────────────────────────────────────

    /// Transfer ownership of an NFT to a new holder.
    ///
    /// Royalties are automatically calculated and transferred to the original
    /// creator when `price > 0` (secondary sale).
    ///
    /// # Arguments
    /// * `from`  — current owner. Must sign.
    /// * `to`    — recipient address.
    /// * `nft_id`— certificate identifier.
    /// * `price` — sale price in token base units (0 for a gift/no royalty).
    pub fn transfer(env: Env, from: Address, to: Address, nft_id: u64, price: i128) {
        from.require_auth();
        assert!(price >= 0, "price must be non-negative");

        let mut nft = Self::load_nft(&env, nft_id);

        assert!(nft.status == NftStatus::Active, "NFT is not active");
        assert!(nft.owner == from, "caller is not the NFT owner");
        assert!(nft.transferable, "this NFT cannot be transferred");

        // Settle royalty on secondary sale.
        if price > 0 && nft.royalty_bps > 0 {
            let royalty_amount = (price * nft.royalty_bps as i128) / 10_000;
            if royalty_amount > 0 {
                let royalty_token: Address = env
                    .storage()
                    .persistent()
                    .get(&DataKey::RoyaltyToken)
                    .expect("contract not initialized");
                let tok = token::Client::new(&env, &royalty_token);
                // Transfer royalty from the buyer (caller context) to creator.
                tok.transfer(&from, &nft.creator, &royalty_amount);
            }
        }

        // Record provenance.
        let count: u64 = env
            .storage()
            .persistent()
            .get(&DataKey::TransferCount(nft_id))
            .unwrap_or(0);

        let record = TransferRecord {
            nft_id,
            from: from.clone(),
            to: to.clone(),
            price,
            ledger: env.ledger().sequence(),
        };
        env.storage().persistent().set(&DataKey::Transfer(nft_id, count), &record);
        env.storage().persistent().set(&DataKey::TransferCount(nft_id), &(count + 1));

        // Update owner.
        nft.owner = to.clone();
        env.storage().persistent().set(&DataKey::Nft(nft_id), &nft);

        env.events().publish(
            (symbol_short!("transfer"), nft_id),
            (from, to, price),
        );
    }

    // ── Burn ──────────────────────────────────────────────────────────────

    /// Permanently destroy an NFT certificate.
    ///
    /// Only the current owner may burn, and only if `burnable` is true.
    pub fn burn(env: Env, owner: Address, nft_id: u64) {
        owner.require_auth();

        let mut nft = Self::load_nft(&env, nft_id);

        assert!(nft.status == NftStatus::Active, "NFT is not active");
        assert!(nft.owner == owner, "caller is not the NFT owner");
        assert!(nft.burnable, "this NFT cannot be burned");

        nft.status = NftStatus::Burned;
        env.storage().persistent().set(&DataKey::Nft(nft_id), &nft);

        env.events().publish((symbol_short!("burned"),), (nft_id, owner));
    }

    // ── Admin Freeze ──────────────────────────────────────────────────────

    /// Admin may restrict a certificate (mark as non-transferable).
    ///
    /// Used in cases of IP dispute or fraudulent minting.
    pub fn freeze(env: Env, admin: Address, nft_id: u64) {
        admin.require_auth();
        let stored_admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .expect("contract not initialized");
        assert!(admin == stored_admin, "only admin can freeze");

        let mut nft = Self::load_nft(&env, nft_id);
        nft.transferable = false;
        env.storage().persistent().set(&DataKey::Nft(nft_id), &nft);

        env.events().publish((symbol_short!("frozen"),), (nft_id, admin));
    }

    // ── Royalty Configuration ─────────────────────────────────────────────

    /// Update the royalty basis points for a certificate.
    ///
    /// Only the original creator may update royalties.
    pub fn update_royalty(env: Env, creator: Address, nft_id: u64, new_royalty_bps: u32) {
        creator.require_auth();
        assert!(new_royalty_bps <= 3000, "royalty_bps must not exceed 3000 (30%)");

        let mut nft = Self::load_nft(&env, nft_id);
        assert!(nft.creator == creator, "only the creator can update royalties");
        assert!(nft.status == NftStatus::Active, "NFT is not active");

        nft.royalty_bps = new_royalty_bps;
        env.storage().persistent().set(&DataKey::Nft(nft_id), &nft);

        env.events().publish(
            (symbol_short!("royalty"),),
            (nft_id, creator, new_royalty_bps),
        );
    }

    // ── Queries ───────────────────────────────────────────────────────────

    /// Retrieve a certificate record.
    pub fn get_nft(env: Env, nft_id: u64) -> NftCertificate {
        Self::load_nft(&env, nft_id)
    }

    /// Return the number of recorded ownership transfers for an NFT.
    pub fn get_transfer_count(env: Env, nft_id: u64) -> u64 {
        env.storage()
            .persistent()
            .get(&DataKey::TransferCount(nft_id))
            .unwrap_or(0)
    }

    /// Return a paginated slice of the ownership history for an NFT.
    ///
    /// The cursor and the limit are both clamped by `shared::pagination`, so a
    /// page spans at most `shared::pagination::MAX_LIMIT` records however large
    /// `limit` is. A `limit` of `0` yields the default page size rather than an
    /// empty result, and an `offset` past the end of the history is an empty
    /// page rather than an error, so a client can page to the end without
    /// calling `get_transfer_count` first.
    ///
    /// For the page metadata that makes that loop safe, use
    /// [`get_transfer_history_page`](Self::get_transfer_history_page).
    pub fn get_transfer_history(
        env: Env,
        nft_id: u64,
        offset: u64,
        limit: u64,
    ) -> Vec<TransferRecord> {
        let (records, _info) = Self::transfer_history_page(&env, nft_id, offset, limit);
        records
    }

    /// Like [`get_transfer_history`](Self::get_transfer_history), but returns
    /// the page together with its `PageInfo`: the total record count, the next
    /// cursor, and whether anything exists past this page.
    pub fn get_transfer_history_page(
        env: Env,
        nft_id: u64,
        offset: u64,
        limit: u64,
    ) -> (Vec<TransferRecord>, shared::pagination::PageInfo) {
        Self::transfer_history_page(&env, nft_id, offset, limit)
    }

    // ── Internal ──────────────────────────────────────────────────────────

    /// Shared body of the two transfer-history readers, bounded by
    /// `shared::pagination::collect_window` (closes #876).
    ///
    /// `TransferCount` and `DataKey::Transfer` are keyed by `u64`, while the
    /// pagination arithmetic is `u32`. The narrowing conversions saturate
    /// rather than wrap: a history of `u32::MAX` records is already far past
    /// anything a 100-record page can address, and wrapping would report a
    /// small `total` for a very long history and mis-page the client.
    fn transfer_history_page(
        env: &Env,
        nft_id: u64,
        offset: u64,
        limit: u64,
    ) -> (Vec<TransferRecord>, shared::pagination::PageInfo) {
        let count: u64 = env
            .storage()
            .persistent()
            .get(&DataKey::TransferCount(nft_id))
            .unwrap_or(0);

        shared::pagination::collect_window(
            env,
            u32::try_from(count).unwrap_or(u32::MAX),
            u32::try_from(offset).unwrap_or(u32::MAX),
            u32::try_from(limit).unwrap_or(u32::MAX),
            |i| {
                env.storage()
                    .persistent()
                    .get::<DataKey, TransferRecord>(&DataKey::Transfer(nft_id, u64::from(i)))
            },
        )
    }

    fn next_id(env: &Env) -> u64 {
        let count: u64 = env
            .storage()
            .persistent()
            .get(&DataKey::NftCount)
            .unwrap_or(0);
        let next = count + 1;
        env.storage().persistent().set(&DataKey::NftCount, &next);
        next
    }

    fn load_nft(env: &Env, nft_id: u64) -> NftCertificate {
        env.storage()
            .persistent()
            .get(&DataKey::Nft(nft_id))
            .expect("NFT not found")
    }
}
