//! The Lease: which client may change the Channels. See "Lease" in
//! `docs/protocol/README.md`.

use crate::wire::ErrorCode;

/// How long a Lease lives without a renewal.
pub(crate) const TTL_MS: u32 = 10_000;

/// Token 0 means "no token".
const NO_TOKEN: u64 = 0;

#[derive(Default)]
pub(crate) struct Lease {
    held: Option<Held>,
}

struct Held {
    token: u64,
}

impl Lease {
    /// Checks a token-bearing request. The current token renews the Lease,
    /// and any token claims a free Rig.
    pub(crate) fn hold(&mut self, token: u64) -> Result<(), ErrorCode> {
        if token == NO_TOKEN {
            return Err(ErrorCode::LeaseRequired);
        }
        match &self.held {
            Some(held) if held.token == token => {}
            _ => self.held = Some(Held { token }),
        }
        Ok(())
    }

    /// Grants the Lease to `token`, so a retried acquire succeeds.
    pub(crate) fn acquire(&mut self, token: u64) -> Result<(), ErrorCode> {
        self.hold(token)
    }

    /// Frees the Rig if `token` holds the Lease. Any other token is a no-op.
    pub(crate) fn release(&mut self, token: u64) {
        if self.held.as_ref().is_some_and(|held| held.token == token) {
            self.held = None;
        }
    }
}
