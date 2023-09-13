use std::time::Instant;

use super::states::JobState;

#[derive(Debug)]
pub(crate) struct Job {
    pub(crate) id: u64,
    pub(crate) pri: u32,
    pub(crate) data: Vec<u8>,
    pub(crate) state: JobState, // also contains state-specific data
    pub(crate) created: Instant,
    pub(crate) ttr: u32,
    pub(crate) reserves: u64,
    pub(crate) timeouts: u64,
    pub(crate) releases: u64,
    pub(crate) buries: u64,
    pub(crate) kicks: u64,
}
