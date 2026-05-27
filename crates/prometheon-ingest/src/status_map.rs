//! Mapping between Yellowstone proto enum *discriminants* and our domain types.
//!
//! We deliberately map by integer code rather than the generated proto enum type: the
//! discriminants are part of the gRPC wire contract (`geyser.proto`) and are stable across the
//! proto-crate minor versions we target, so this keeps the (unit-tested) mapping decoupled from
//! generated-type churn. `SubscribeUpdateSlot.status` is an `i32` in the prost output, which we
//! feed straight in here.
//!
//! Source of truth (`geyser.proto`):
//! ```text
//! enum SlotStatus {
//!   SLOT_PROCESSED = 0; SLOT_CONFIRMED = 1; SLOT_FINALIZED = 2;
//!   SLOT_FIRST_SHRED_RECEIVED = 3; SLOT_COMPLETED = 4; SLOT_CREATED_BANK = 5; SLOT_DEAD = 6;
//! }
//! enum CommitmentLevel { PROCESSED = 0; CONFIRMED = 1; FINALIZED = 2; }
//! ```

use prometheon_types::{Commitment, SlotStatus};

/// Proto `SlotStatus` discriminants (wire contract).
pub mod slot_status_code {
    pub const PROCESSED: i32 = 0;
    pub const CONFIRMED: i32 = 1;
    pub const FINALIZED: i32 = 2;
    pub const FIRST_SHRED_RECEIVED: i32 = 3;
    pub const COMPLETED: i32 = 4;
    pub const CREATED_BANK: i32 = 5;
    pub const DEAD: i32 = 6;
}

/// Proto `CommitmentLevel` discriminants (wire contract).
pub mod commitment_code {
    pub const PROCESSED: i32 = 0;
    pub const CONFIRMED: i32 = 1;
    pub const FINALIZED: i32 = 2;
}

/// Map a proto `SlotStatus` discriminant to our [`SlotStatus`], or `None` if unknown.
pub fn slot_status_from_code(code: i32) -> Option<SlotStatus> {
    use slot_status_code as c;
    Some(match code {
        c::PROCESSED => SlotStatus::Processed,
        c::CONFIRMED => SlotStatus::Confirmed,
        c::FINALIZED => SlotStatus::Finalized,
        c::FIRST_SHRED_RECEIVED => SlotStatus::FirstShredReceived,
        c::COMPLETED => SlotStatus::Completed,
        c::CREATED_BANK => SlotStatus::CreatedBank,
        c::DEAD => SlotStatus::Dead,
        _ => return None,
    })
}

/// Map our [`Commitment`] to the proto `CommitmentLevel` discriminant for a `SubscribeRequest`.
pub fn commitment_to_code(commitment: Commitment) -> i32 {
    match commitment {
        Commitment::Processed => commitment_code::PROCESSED,
        Commitment::Confirmed => commitment_code::CONFIRMED,
        Commitment::Finalized => commitment_code::FINALIZED,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_slot_status_code_maps_to_the_right_variant() {
        assert_eq!(slot_status_from_code(0), Some(SlotStatus::Processed));
        assert_eq!(slot_status_from_code(1), Some(SlotStatus::Confirmed));
        assert_eq!(slot_status_from_code(2), Some(SlotStatus::Finalized));
        assert_eq!(
            slot_status_from_code(3),
            Some(SlotStatus::FirstShredReceived)
        );
        assert_eq!(slot_status_from_code(4), Some(SlotStatus::Completed));
        assert_eq!(slot_status_from_code(5), Some(SlotStatus::CreatedBank));
        assert_eq!(slot_status_from_code(6), Some(SlotStatus::Dead));
    }

    #[test]
    fn unknown_slot_status_code_is_none() {
        assert_eq!(slot_status_from_code(7), None);
        assert_eq!(slot_status_from_code(-1), None);
    }

    #[test]
    fn commitment_maps_to_proto_codes() {
        assert_eq!(commitment_to_code(Commitment::Processed), 0);
        assert_eq!(commitment_to_code(Commitment::Confirmed), 1);
        assert_eq!(commitment_to_code(Commitment::Finalized), 2);
    }
}
