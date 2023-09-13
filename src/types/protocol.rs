use serde::Serialize;

use super::serialisable::BeanstalkSerialisable;
use super::states::JobState;

/// A command sent by the client to the server.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum BeanstalkCommand {
    /// Places a job onto the currently `use`d queue.
    ///
    /// On the wire: `put <pri> <delay> <ttr>`
    Put {
        pri: u32,
        delay: u32,
        ttr: u32,
        n_bytes: u32,
    },
    /// Awaits a job from all the `watch`ed queues, blocking until one appears
    /// (or until the server shuts down).
    ///
    /// On the wire: `reserve`
    Reserve,
    /// As `reserve`, but after `timeout` seconds pass, a `TIMED_OUT` response
    /// is sent instead.
    ///
    /// On the wire: `reserve-with-timeout <seconds>`
    ReserveWithTimeout { timeout: u32 },
    /// Reserves a job with a given ID if it exists and is not already reserved,
    /// otherwise returning `NOT_FOUND`.
    ///
    /// On the wire: `reserve-job <id>`
    ReserveJob { id: u64 },
    /// Releases a job reserved by the same client, returning it to the ready
    /// queue. Returns `RELEASED` or `NOT_FOUND` in most cases, but can also
    /// return `BURIED` if the server was unable to expand the priority queue
    /// data structure.
    ///
    /// On the wire: `release <id> <pri> <delay>`
    Release { id: u64, pri: u32, delay: u32 },
    /// Deletes a job reserved by the same client, or in the ready, buried, or
    /// delayed states. Returns `DELETED` or `NOT_FOUND`.
    ///
    /// On the wire: `delete <id>`
    Delete { id: u64 },
    /// Buries a job reserved by the same client. Returns `BURIED` or
    /// `NOT_FOUND`.
    ///
    /// On the wire: `bury <id>`
    Bury { id: u64, pri: u32 },
    /// Refreshes the Time To Run (TTR) of a job reserved by the same client.
    /// Returns `TOUCHED` or `NOT_FOUND`.
    ///
    /// On the wire: `touch <id>`
    Touch { id: u64 },
    /// Adds a tube to the watchlist for this client. Always replies with
    /// `WATCHING <number of watched tubes>`.
    ///
    /// On the wire: `watch <tube>`
    Watch { tube: Vec<u8> },
    /// Reverses the effect of `watch` on this client. Returns `WATCHING <n>` or
    /// `NOT_IGNORED` if this would remove the last queue in the watchlist.
    ///
    /// On the wire: `ignore <tube>`
    Ignore { tube: Vec<u8> },
    /// Returns the data for the job with this ID, regardless of its state.
    /// Response is either `FOUND <id> <bytes>` or `NOT_FOUND`, in common with
    /// all requests in the `peek` family.
    ///
    /// On the wire: `peek <id>`
    Peek { id: u64 },
    /// Returns the data for the next ready job on the currently-used tube.
    ///
    /// On the wire: `peek-ready`
    PeekReady,
    /// Returns the data for the next delayed job that will become ready on the
    /// currently-used tube.
    ///
    /// On the wire: `peek-delayed`
    PeekDelayed,
    /// Returns the data for the first available buried job on the currently-
    /// used tube.
    /// TODO: is this FIFO or FILO?
    ///
    /// On the wire: `peek-buried`
    PeekBuried,
    /// Promotes up to `bound` jobs on the currently-used tube from buried to
    /// the ready states, returning `KICKED <count>` with the actual number of
    /// jobs kicked. If no buried jobs exist, it promotes delayed jobs instead.
    /// In other words, if at least one buried jobs exist, at least two kick
    /// commands must be executed for any delayed jobs to be kicked.
    ///
    /// On the wire: `kick <bound>
    Kick { bound: u64 },
    /// Promotes a single job from buried or delayed to ready by its ID.
    /// Returns `KICKED` if successful, otherwise `NOT_FOUND` if the job ID
    /// doesn't exist or the job is not kickable.
    ///
    /// On the wire: `kick-job <id>`
    KickJob { id: u64 },
    /// Provides information about the job with the given ID, including which
    /// tube it's on, state, priority, timings, and the number of state
    /// transitions it's undergone.
    ///
    /// As with all responses from the `Stats` and `ListTubes` families of
    /// commands, returns an `OK <n_bytes>` response with associated data.
    ///
    /// As with all responses from the `Stats` family of commands, returns a
    /// YAML object.
    ///
    /// On the wire: `stats-job <id>`
    StatsJob { id: u64 },
    /// Returns information about a tube, including the number of jobs in each
    /// state, number of active consumers and producers, total jobs handled, and
    /// pause status.
    ///
    /// On the wire: `stats <tube>`
    StatsTube { tube: Vec<u8> },
    /// Exposes information about the server, including global job counts by
    /// state, number of each command executed, and various internal statuses.
    ///
    /// On the wire: `stats`
    StatsServer,
    /// Returns a list of which tubes currently exist (have been `use`d by any
    /// consumer).
    ///
    /// As for all commands in the `ListTubes` family, returns an `OK <n_bytes>`
    /// response with associated data encoding a YAML-format list.
    ///
    /// On the wire: `list-tubes`
    ListTubes,
    /// Returns the tube name this client is currently using as `USING <tube>`.
    ///
    /// On the wire: `list-tube-used`
    ListTubeUsed,
    /// Returns any tubes this client is currently watching.
    ///
    /// On the wire: `list-tubes-watched`
    ListTubesWatched,
    /// Requests that the server close this connection, releasing any
    /// server-side resources in doing so.
    ///
    /// On the wire: `quit`
    Quit,
    /// Pause a tube for a given period, preventing new jobs being reserved for
    /// `delay` seconds. Returns `PAUSED` or `NOT_FOUND`.
    ///
    /// On the wire: `pause-tube <tube> <delay>`
    PauseTube { tube: Vec<u8>, delay: u32 },
    /// On the wire: `use <tube>`
    Use { tube: Vec<u8> },
}

/// All possible response types to a `BeanstalkRequest`.
pub(crate) enum BeanstalkResponse {
    /// Indicates the server cannot handle a job due to memory pressure. Can be
    /// sent in response to any command.
    ///
    /// On the wire: `OUT_OF_MEMORY`.
    OutOfMemory,
    /// Indicates a server bug. Can be sent in response to any command.
    ///
    /// On the wire: `INTERNAL_ERROR`.
    InternalError,
    /// The client sent a bad request, typically because:
    ///
    /// * The request exceeded 224 bytes , including trailing CRLF.
    /// * A tube name exceeded 200 bytes or was invalid.
    /// * A non-number was provided where a number was expected, or the number
    ///   was out of range.
    ///
    /// On the wire: `BAD_FORMAT`.
    BadFormat,
    /// The client sent a bad request with an unrecognised command.
    ///
    /// On the wire: `UNKNOWN_COMMAND`.
    UnknownCommand,
    /// In response to a `put`, indicates a job was created with the given ID.
    ///
    /// On the wire: `INSERTED <id>`.
    Inserted { id: u64 },
    /// In response to a `put`, indicates the job couldn't be handled due to
    /// memory pressure and so was immediately buried.
    ///
    /// On the wire: `BURIED <id>`.
    BuriedID { id: u64 },
    /// In response to a `put`, indicates the job data was not terminated by a
    /// CRLF sequence.
    ///
    /// On the wire: `EXPECTED_CRLF`.
    ExpectedCRLF,
    /// In response to a `put`, indicates the job body was larger than what the
    /// server is configured to accept.
    ///
    /// On the wire: `JOB_TOO_BIG`.
    JobTooBig,
    /// In response to a `put`, indicates the server is not currently accepting
    /// jobs.
    ///
    /// On the wire: `DRAINING`.
    Draining,
    /// In response to a `use` or `list-tube-used`, indicates the client is
    /// watching this tube.
    ///
    /// On the wire: `USING <tube>`.
    Using { tube: Vec<u8> },
    /// In response to a `reserve` or `reserve-with-timeout`, indicates the
    /// client has reserved a job that will exceed its Time To Run (TTR) in the
    /// next second and so will be released automatically. Can be returned
    /// immediately or after a delay.
    ///
    /// On the wire: `DEADLINE_SOON`.
    DeadlineSoon,
    /// In response to a `reserve-with-timeout`, indicates the timeout provided
    /// expired with no job becoming available.
    ///
    /// On the wire: `TIMED_OUT`.
    TimedOut,
    /// In response to a `reserve`, `reserve-with-timeout`, or `reserve-job`,
    /// provides the ID and data of the job that was just reserved.
    ///
    /// On the wire: `RESERVED <id> <n_bytes>` plus data.
    Reserved { id: u64, data: Vec<u8> },
    /// In response to any of the following commands, indicates a general state
    /// where a specific job isn't known to the server, or doesn't satisfy
    /// a precondition to be returned by the command.
    ///
    /// Specific cases include:
    ///
    /// * `reserve-job`: the job is reserved or unknown.
    /// * `delete`: the job is unknown; or the job isn't ready, buried, or
    ///   reserved by this client.
    /// * `release`, `bury`, or `touch`: the job is unknown or is not reserved
    ///   by this client.
    /// * `peek`: the job is unknown.
    /// * `peek-*` family: no such jobs exist on the currently `use`d tube.
    /// * `kick-job`: the job is unknown or is neither buried nor delayed, or
    ///   allowable if an internal server error occurred preventing the kick.
    /// * `pause-tube`: the tube does not exist.
    ///
    /// On the wire: `NOT_FOUND`.
    NotFound,
    /// In response to a `delete` command, indicates the job was successfully
    /// deleted.
    ///
    /// On the wire: `DELETED`.
    Deleted,
    /// In response to a `release` command, indicates the job was successfully
    /// released back to the ready or delayed states.
    ///
    /// On the wire: `RELEASED`.
    Released,
    /// In response to a `release`, indicates the job couldn't be handled due to
    /// memory pressure and so was immediately buried.
    ///
    /// In response to a `bury`, indicates success.
    ///
    /// On the wire: `BURIED`.
    Buried,
    /// In response to a `touch`, indicates the job's TTR was refreshed.
    ///
    /// On the wire: `TOUCHED`.
    Touched,
    /// In response to a `watch` or `ignore`, indicates success and the number
    /// of tubes currently watched by the client.
    ///
    /// On the wire: `WATCHING <count>`.
    Watching { count: u32 },
    /// In response to an `ignore`, indicates the command failed as it would
    /// leave the client with an empty watchlist.
    ///
    /// On the wire: `NOT_IGNORED`.
    NotIgnored,
    /// In response to a `peek`-family command, indicates success.
    ///
    /// On the wire: `FOUND <id> <n_bytes>` plus data.
    Found { id: u64, data: Vec<u8> },
    /// In response to a `kick`, indicates success with the number of jobs
    /// kicked from the buried xor delayed states.
    ///
    /// On the wire: `KICKED <count>`.
    KickedCount { count: u64 },
    /// In response to a `kick-job`, indicates success.
    ///
    /// On the wire: `KICKED`.
    Kicked,
    /// In response to a `stats-job`, indicates success.
    ///
    /// On the wire: `OK <n_bytes>` plus data in YAML dictionary format.
    OkStatsJob { data: JobStats },
    ///In response to a `stats`, indicates success.
    ///
    /// On the wire: `OK <n_bytes>` plus data in YAML dictionary format.
    OkStats { data: ServerStats },
    ///In response to a `stats-tube`, indicates success.
    ///
    /// On the wire: `OK <n_bytes>` plus data in YAML dictionary format.
    OkStatsTube { data: TubeStats },
    ///In response to a `list-tubes` or `list-tubes-watched`, indicates success.
    ///
    /// On the wire: `OK <n_bytes>` plus data in YAML *list* format.
    OkListTubes { tubes: Vec<Vec<u8>> },
    /// In response to a `pause-tube`, indicates success.
    ///
    /// On the wire: `PAUSED`.
    Paused,
}

impl BeanstalkSerialisable for BeanstalkResponse {
    fn serialise_beanstalk(&self) -> Vec<u8> {
        use BeanstalkResponse::*;

        match self {
            OutOfMemory => b"OUT_OF_MEMORY\r\n".to_vec(),
            InternalError => b"INTERNAL_ERROR\r\n".to_vec(),
            BadFormat => b"BAD_FORMAT\r\n".to_vec(),
            UnknownCommand => b"UNKNOWN_COMMAND\r\n".to_vec(),
            Inserted { id } => format!("INSERTED {id}\r\n").into(),
            BuriedID { id } => format!("BURIED {id}\r\n").into(),
            ExpectedCRLF => b"EXPECTED_CRLF\r\n".to_vec(),
            JobTooBig => b"JOB_TOO_BIG\r\n".to_vec(),
            Draining => b"DRAINING\r\n".to_vec(),
            Using { tube } => {
                [b"USING ".to_vec(), tube.to_owned(), b"\r\n".to_vec()].concat()
            },
            DeadlineSoon => b"DEADLINE_SOON\r\n".to_vec(),
            TimedOut => b"TIMED_OUT\r\n".to_vec(),
            Reserved { id, data } => [
                format!("RESERVED {id} {}\r\n", data.len()).into_bytes(),
                data.to_owned(), // TODO: reduce copying
                b"\r\n".to_vec(),
            ]
            .concat(),
            NotFound => b"NOT_FOUND\r\n".to_vec(),
            Released => b"RELEASED\r\n".to_vec(),
            Watching { count } => format!("WATCHING {count}\r\n").into(),
            NotIgnored => b"NOT_IGNORED\r\n".to_vec(),
            Found { id, data } => {
                [
                    format!("FOUND {id} {}\r\n", data.len()).into(),
                    data.to_owned(), // TODO: reduce copying
                    b"\r\n".to_vec(),
                ]
                .concat()
            },
            KickedCount { count } => format!("KICKED {count}\r\n").into(),
            Kicked => b"KICKED\r\n".to_vec(),
            OkStatsJob { data } => {
                let data = serde_yaml::to_string(data).unwrap();
                format!("OK {}\r\n{data}\r\n", data.len()).into()
            },
            OkStats { data } => {
                let data = serde_yaml::to_string(data).unwrap();
                format!("OK {}\r\n{data}\r\n", data.len()).into()
            },
            OkListTubes { tubes } => {
                let data = serde_yaml::to_string(tubes).unwrap();
                format!("OK {}\r\n{data}\r\n", data.len()).into()
            },
            Paused => b"PAUSED\r\n".to_vec(),
            Deleted => b"DELETED\r\n".to_vec(),
            Buried => b"BURIED\r\n".to_vec(),
            Touched => b"TOUCHED\r\n".to_vec(),
            OkStatsTube { data } => {
                let data = serde_yaml::to_string(data).unwrap();
                format!("OK {}\r\n{data}\r\n", data.len()).into()
            },
        }
    }
}

#[derive(Debug, Serialize)]
pub(crate) struct JobStats {
    /// job ID
    pub(crate) id: u64,
    /// tube containing job
    pub(crate) tube: Vec<u8>,
    /// job state
    pub(crate) state: JobState,
    /// priority set by last put/release/bury
    pub(crate) pri: u32,

    /// time in seconds since creation
    pub(crate) age: u32, // TODO: size
    /// seconds remaining until ready
    pub(crate) delay: u32, // TODO: size
    /// allowed processing time in seconds
    pub(crate) ttr: u32, // TODO: size
    /// time until job returns to ready queue
    #[serde(rename = "time-left")]
    pub(crate) time_left: u32, // TODO: size

    /// earliest binlog file containing job
    pub(crate) file: u32, // TODO: size

    /// number of times job reserved
    pub(crate) reserves: u64, // TODO: size
    /// number of times job timed out
    pub(crate) timeouts: u64, // TODO: size
    /// number of times job released
    pub(crate) releases: u64, // TODO: size
    /// number of times job buried
    pub(crate) buries: u64, // TODO: size
    /// number of times job kicked
    pub(crate) kicks: u64, // TODO: size
}

#[derive(Debug, Serialize)]
pub(crate) struct TubeStats {
    /// tube name
    pub(crate) name: Vec<u8>,
    /// number of jobs in ready state with priority < 1024
    #[serde(rename = "current-jobs-urgent")]
    pub(crate) current_jobs_urgent: u64,
    /// number of jobs in ready state
    #[serde(rename = "current-jobs-ready")]
    pub(crate) current_jobs_ready: u64,
    /// number of jobs reserved by clients
    #[serde(rename = "current-jobs-reserved")]
    pub(crate) current_jobs_reserved: u64,
    /// number of jobs in delayed state
    #[serde(rename = "current-jobs-delayed")]
    pub(crate) current_jobs_delayed: u64,
    /// number of jobs in buried state
    #[serde(rename = "current-jobs-buried")]
    pub(crate) current_jobs_buried: u64,
    /// total jobs created in this tube
    #[serde(rename = "total-jobs")]
    pub(crate) total_jobs: u64,
    /// number of clients that have `use`d this queue
    #[serde(rename = "current-using")]
    pub(crate) current_using: u64,
    /// number of clients that have `watch`ed this queue and are waiting on a
    /// `reserve`
    #[serde(rename = "current-waiting")]
    pub(crate) current_waiting: u64,
    /// number of clients that have `watch`ed this queue
    #[serde(rename = "current-watching")]
    pub(crate) current_watching: u64,
    /// number of seconds this queue has been paused for in total
    pub(crate) pause: u32,
    /// number of `delete` commands issued for this tube
    #[serde(rename = "cmd-delete")]
    pub(crate) cmd_delete: u64,
    /// number of `pause-tube` commands issued for this tube
    #[serde(rename = "cmd-pause-tube")]
    pub(crate) cmd_pause_tube: u64,
    /// seconds remaining until the queue is un-paused.
    #[serde(rename = "pause-time-left")]
    pub(crate) pause_time_left: u32,
}

#[derive(Debug, Serialize)]
pub(crate) struct ServerStats {
    /// number of ready jobs with priority < 1024
    #[serde(rename = "current-jobs-urgent")]
    pub(crate) current_jobs_urgent: u64,
    /// number of jobs in the ready queue
    #[serde(rename = "current-jobs-ready")]
    pub(crate) current_jobs_ready: u64,
    /// number of jobs reserved by all clients
    #[serde(rename = "current-jobs-reserved")]
    pub(crate) current_jobs_reserved: u64,
    /// number of delayed jobs
    #[serde(rename = "current-jobs-delayed")]
    pub(crate) current_jobs_delayed: u64,
    /// number of buried jobs
    #[serde(rename = "current-jobs-buried")]
    pub(crate) current_jobs_buried: u64,

    /// number of X commands
    #[serde(rename = "cmd-put")]
    pub(crate) cmd_put: u64,
    /// number of X commands
    #[serde(rename = "cmd-peek")]
    pub(crate) cmd_peek: u64,
    /// number of X commands
    #[serde(rename = "cmd-peek-ready")]
    pub(crate) cmd_peek_ready: u64,
    /// number of X commands
    #[serde(rename = "cmd-peek-delayed")]
    pub(crate) cmd_peek_delayed: u64,
    /// number of X commands
    #[serde(rename = "cmd-peek-buried")]
    pub(crate) cmd_peek_buried: u64,
    /// number of X commands
    #[serde(rename = "cmd-reserve")]
    pub(crate) cmd_reserve: u64,
    /// number of X commands
    #[serde(rename = "cmd-reserve-with-timeout")]
    pub(crate) cmd_reserve_with_timeout: u64,
    /// number of X commands
    #[serde(rename = "cmd-touch")]
    pub(crate) cmd_touch: u64,
    /// number of X commands
    #[serde(rename = "cmd-use")]
    pub(crate) cmd_use: u64,
    /// number of X commands
    #[serde(rename = "cmd-watch")]
    pub(crate) cmd_watch: u64,
    /// number of X commands
    #[serde(rename = "cmd-ignore")]
    pub(crate) cmd_ignore: u64,
    /// number of X commands
    #[serde(rename = "cmd-delete")]
    pub(crate) cmd_delete: u64,
    /// number of X commands
    #[serde(rename = "cmd-release")]
    pub(crate) cmd_release: u64,
    /// number of X commands
    #[serde(rename = "cmd-bury")]
    pub(crate) cmd_bury: u64,
    /// number of X commands
    #[serde(rename = "cmd-kick")]
    pub(crate) cmd_kick: u64,
    /// number of X commands
    #[serde(rename = "cmd-stats")]
    pub(crate) cmd_stats: u64,
    /// number of X commands
    #[serde(rename = "cmd-stats-job")]
    pub(crate) cmd_stats_job: u64,
    /// number of X commands
    #[serde(rename = "cmd-stats-tube")]
    pub(crate) cmd_stats_tube: u64,
    /// number of X commands
    #[serde(rename = "cmd-list-tubes")]
    pub(crate) cmd_list_tubes: u64,
    /// number of X commands
    #[serde(rename = "cmd-list-tube-used")]
    pub(crate) cmd_list_tube_used: u64,
    /// number of X commands
    #[serde(rename = "cmd-list-tubes-watched")]
    pub(crate) cmd_list_tubes_watched: u64,
    /// number of X commands
    #[serde(rename = "cmd-pause-tube")]
    pub(crate) cmd_pause_tube: u64,

    /// cumulative count of times a job has timed out
    #[serde(rename = "job-timeouts")]
    pub(crate) job_timeouts: u64,
    /// cumulative count of jobs created
    #[serde(rename = "total-jobs")]
    pub(crate) total_jobs: u64,
    /// maximum number of bytes in a job
    #[serde(rename = "max-job-size")]
    pub(crate) max_job_size: u64,
    /// number of currently-existing tubes
    #[serde(rename = "current-tubes")]
    pub(crate) current_tubes: u64,
    /// number of currently open connections
    #[serde(rename = "current-connections")]
    pub(crate) current_connections: u64,
    /// number of open connections that have each issued at least one put command
    #[serde(rename = "current-producers")]
    pub(crate) current_producers: u64,
    /// number of open connections that have each issued at least one reserve command
    #[serde(rename = "current-workers")]
    pub(crate) current_workers: u64,
    /// number of open connections that have issued a reserve command but not yet received a response
    #[serde(rename = "current-waiting")]
    pub(crate) current_waiting: u64,
    /// cumulative count of connections
    #[serde(rename = "total-connections")]
    pub(crate) total_connections: u64,
    /// process id of the server
    pub(crate) pid: u32,
    /// version string of the server
    pub(crate) version: &'static str,
    /// cumulative user CPU time of this process in seconds and microseconds
    #[serde(rename = "rusage-utime")]
    pub(crate) rusage_utime: u64,
    /// cumulative system CPU time of this process in seconds and microseconds
    #[serde(rename = "rusage-stime")]
    pub(crate) rusage_stime: u64,
    /// number of seconds since this server process started running
    pub(crate) uptime: u32,

    /// index of the oldest binlog file needed to store the current jobs
    #[serde(rename = "binlog-oldest-index")]
    pub(crate) binlog_oldest_index: u64,
    /// index of the current binlog file being written to. If binlog is not active this value will be 0
    #[serde(rename = "binlog-current-index")]
    pub(crate) binlog_current_index: u64,
    /// maximum size in bytes a binlog file is allowed to get before a new binlog file is opened
    #[serde(rename = "binlog-max-size")]
    pub(crate) binlog_max_size: u64,
    /// cumulative number of records written to the binlog
    #[serde(rename = "binlog-records-written")]
    pub(crate) binlog_records_written: u64,
    /// cumulative number of records written as part of compaction
    #[serde(rename = "binlog-records-migrated")]
    pub(crate) binlog_records_migrated: u64,

    /// is server is in drain mode
    pub(crate) draining: bool,
    /// random id string for this server process, generated every time beanstalkd process starts
    pub(crate) id: Vec<u8>,
    // hostname of the machine as determined by uname
    pub(crate) hostname: Vec<u8>,
    /// OS version as determined by uname
    pub(crate) os: Vec<u8>,
    // machine architecture as determined by uname
    pub(crate) platform: Vec<u8>,
}
