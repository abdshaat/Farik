//! The Procurement Specialist's purchase orders and renewals (`docs/SPEC.md` 6.10, ADR 0039): the
//! locks under which an order is numbered, decided, placed, received, closed and expired, and the
//! renewals are read.

use std::sync::Mutex;

/// Held from the first read of the orders to the record that changes them: by the agent's draft,
/// by each of the owner's commands, by a status and by the expiry, so that two of them never take
/// one number, decide one order twice or expire an order being placed. One lock for every project
/// in the process.
pub(crate) static ORDERS: Mutex<()> = Mutex::new(());
