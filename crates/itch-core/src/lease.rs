//! The Lease: which client may change the Channels. See "Lease" in
//! `docs/protocol/README.md`.

use crate::wire::{ErrorCode, MAX_LABEL};

/// How long a Lease lives without a renewal.
pub(crate) const TTL_MS: u32 = 10_000;

/// Token 0 means "no token".
const NO_TOKEN: u64 = 0;

/// The Holder of a Lease claimed by a request that names none.
const UNKNOWN_HOLDER: &str = "unknown";

#[derive(Default)]
pub(crate) struct Lease {
    held: Option<Held>,
}

struct Held {
    token: u64,
    /// When the Lease was last claimed or renewed, in `Board::now` ms.
    renewed_at: u64,
    /// The Holder text, held inline so the core needs no allocator.
    holder: [u8; MAX_LABEL],
    holder_len: usize,
}

impl Held {
    fn new(token: u64, holder: &str, now: u64) -> Self {
        let mut held = Self { token, renewed_at: now, holder: [0; MAX_LABEL], holder_len: 0 };
        held.name(holder);
        held
    }

    /// Sets the Holder, "unknown" if empty. The decoder has already capped
    /// it at 64 bytes.
    fn name(&mut self, holder: &str) {
        let holder = if holder.is_empty() { UNKNOWN_HOLDER } else { holder };
        self.holder[..holder.len()].copy_from_slice(holder.as_bytes());
        self.holder_len = holder.len();
    }
}

impl Lease {
    /// Frees the Rig once the Lease has gone a TTL without a renewal.
    pub(crate) fn expire(&mut self, now: u64) {
        if self
            .held
            .as_ref()
            .is_some_and(|held| now.saturating_sub(held.renewed_at) >= u64::from(TTL_MS))
        {
            self.held = None;
        }
    }

    /// Checks a token-bearing request. The current token renews the Lease,
    /// any token claims a free Rig, and `holder`, if the request carries
    /// one, names the Holder.
    pub(crate) fn hold(
        &mut self,
        token: u64,
        holder: Option<&str>,
        now: u64,
    ) -> Result<(), ErrorCode> {
        if token == NO_TOKEN {
            return Err(ErrorCode::LeaseRequired);
        }
        let held = match &mut self.held {
            Some(held) if held.token == token => held,
            Some(_) => return Err(ErrorCode::LeaseLost),
            None => self.held.insert(Held::new(token, UNKNOWN_HOLDER, now)),
        };
        held.renewed_at = now;
        if let Some(holder) = holder {
            held.name(holder);
        }
        Ok(())
    }

    /// Grants the Lease to `token` when the Rig is free or `token` already
    /// holds it, so a retried acquire succeeds. `takeover` replaces a live
    /// Holder.
    pub(crate) fn acquire(
        &mut self,
        token: u64,
        takeover: bool,
        holder: &str,
        now: u64,
    ) -> Result<(), ErrorCode> {
        if token == NO_TOKEN {
            return Err(ErrorCode::LeaseRequired);
        }
        if !takeover && self.held.as_ref().is_some_and(|held| held.token != token) {
            return Err(ErrorCode::LeaseHeld);
        }
        self.held = Some(Held::new(token, holder, now));
        Ok(())
    }

    /// Frees the Rig if `token` holds the Lease. Any other token is a no-op.
    pub(crate) fn release(&mut self, token: u64) {
        if self.held.as_ref().is_some_and(|held| held.token == token) {
            self.held = None;
        }
    }

    /// The current Holder, or "" when the Rig is free.
    pub(crate) fn holder(&self) -> &str {
        match &self.held {
            Some(held) => {
                core::str::from_utf8(&held.holder[..held.holder_len]).expect("holders are UTF-8")
            }
            None => "",
        }
    }
}
