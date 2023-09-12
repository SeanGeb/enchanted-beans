/// A command sent by the client to the server.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum BeanstalkCommand {
    /// `put <pri> <delay> <ttr>`
    Put {
        pri: u32,
        delay: u32,
        ttr: u32,
        n_bytes: u32,
    },
    /// `reserve`
    Reserve,
    /// `reserve-with-timeout <seconds>`
    ReserveWithTimeout { timeout: u32 }, // TODO: check
    /// `reserve-job <id>`
    ReserveJob { id: u64 },
    /// `release <id> <pri> <delay>`
    Release { id: u64, pri: u32, delay: u32 },
    /// `delete <id>`
    Delete { id: u64 },
    /// `bury <id>`
    Bury { id: u64, pri: u32 },
    /// `touch <id>`
    Touch { id: u64 },
    /// `watch <tube>`
    Watch { tube: Vec<u8> }, // TODO: "at most 200 bytes"
    /// `ignore <tube>`
    Ignore { tube: Vec<u8> },
    /// `peek <id>`
    Peek { id: u64 },
    /// `peek-ready`
    PeekReady,
    /// `peek-delayed`
    PeekDelayed,
    /// `peek-buried`
    PeekBuried,
    /// `kick <bound>
    Kick { bound: u64 },
    /// `kick-job <id>`
    KickJob { id: u64 },
    /// `stats-job <id>`
    StatsJob { id: u64 },
    /// `stats <tube>`
    StatsTube { tube: Vec<u8> },
    /// `stats`
    StatsServer,
    /// `list-tubes`
    ListTubes,
    /// `list-tube-used`
    ListTubeUsed,
    /// `list-tubes-watched`
    ListTubesWatched,
    /// `quit`
    Quit,
    /// `pause-tube`
    PauseTube { tube: Vec<u8>, delay: u32 },
    /// `use <tube>`
    Use { tube: Vec<u8> },
}
