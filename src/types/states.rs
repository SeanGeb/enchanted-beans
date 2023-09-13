use std::time::Instant;

use serde::Serialize;

#[derive(Debug)]
pub(crate) enum JobState {
    Ready,
    Delayed { until: Instant },
    Reserved { at: Instant },
    Buried,
}

// This impl is used to allow JobStats to be serialised to YAML.
impl Serialize for JobState {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use JobState::*;

        serializer.serialize_str(match self {
            Ready => "ready",
            Delayed { until: _ } => "delayed",
            Reserved { at: _ } => "reserved",
            Buried => "buried",
        })
    }
}
