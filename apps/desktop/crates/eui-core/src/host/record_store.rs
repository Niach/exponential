//! VAPP-161: the format's NEUTRAL records (docs/package-format.md
//! "Records"). A surface reads a kind through the binding
//! `records:<kind>?query` and writes through the host functions
//! `records.upsert` / `records.delete`; the HOST provides the store. The
//! query and its value are [`super::query`] (VAPP-189: one evaluator over
//! the rows of any store, `fixtures/records-query.json`).

pub use super::query::{
    parse_records_source, RecordsFilter, RecordsQuery, RECORDS_LIMIT_DEFAULT, RECORDS_LIMIT_MAX, RECORDS_ORDER_DEFAULT, RECORDS_RESERVED_PARAMS, RECORDS_SCHEME, RECORDS_SORT_DEFAULT,
};

/// The write functions (no consent card: the app's own rows); VAPP-194:
/// `records.batch {writes}` = a package action's writes as ONE unit.
pub const RECORDS_FUNCTIONS: &[&str] = &["records.upsert", "records.delete", super::actions::RECORDS_BATCH_FUNCTION];
