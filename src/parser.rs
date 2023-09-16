//! implements a parser for the beanstalkd TCP protocol.
use std::fmt;

use crate::types::protocol::BeanstalkCommand;
use crate::types::serialisable::BeanstalkSerialisable;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ParsingError {
    BadFormat,
    UnknownCommand,
}

impl fmt::Display for ParsingError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(match self {
            Self::BadFormat => "bad format",
            Self::UnknownCommand => "unknown command",
        })
    }
}

impl BeanstalkSerialisable for ParsingError {
    fn serialise_beanstalk(&self) -> Vec<u8> {
        match self {
            ParsingError::BadFormat => b"BAD_FORMAT\r\n".to_vec(),
            ParsingError::UnknownCommand => b"UNKNOWN_COMMAND\r\n".to_vec(),
        }
    }
}

/// Provides a custom, minimal, zero-copy parser of byte slices.
struct ParseState<'a> {
    from: &'a [u8],
}

impl ParseState<'_> {
    /// Asserts there's no more input to take, returning `result` if so, and a
    /// `BadFormat` error otherwise.
    fn expect_done_and<R>(&self, result: R) -> Result<R, ParsingError> {
        if self.from.len() == 0 {
            Ok(result)
        } else {
            Err(ParsingError::BadFormat)
        }
    }

    /// Consumes from the input, expecting a token of non-zero length.
    fn expect_next_token(&mut self) -> Result<&[u8], ParsingError> {
        let token = self.next_token().ok_or(ParsingError::BadFormat)?;

        if token.len() == 0 {
            Err(ParsingError::BadFormat)
        } else {
            Ok(token)
        }
    }

    /// Consumes from the input, expecting a space then a u32.
    fn expect_next_u32(&mut self) -> Result<u32, ParsingError> {
        self.expect_space()?;

        let token = self.expect_next_token()?;

        let mut r = 0u32;
        for v in token {
            match v {
                b'0'..=b'9' => {
                    r = r
                        .checked_mul(10)
                        .ok_or(ParsingError::BadFormat)?
                        .checked_add((*v - b'0') as u32)
                        .ok_or(ParsingError::BadFormat)?
                },
                _ => return Err(ParsingError::BadFormat),
            };
        }

        Ok(r)
    }

    /// Consumes from the input, expecting a space then a u64.
    fn expect_next_u64(&mut self) -> Result<u64, ParsingError> {
        self.expect_space()?;

        let token = self.expect_next_token()?;

        let mut r = 0u64;
        for v in token {
            match v {
                b'0'..=b'9' => {
                    r = r
                        .checked_mul(10)
                        .ok_or(ParsingError::BadFormat)?
                        .checked_add((*v - b'0') as u64)
                        .ok_or(ParsingError::BadFormat)?
                },
                _ => return Err(ParsingError::BadFormat),
            };
        }

        Ok(r)
    }

    /// Consumes from the input, expecting a space then a name.
    fn expect_next_name(&mut self) -> Result<Vec<u8>, ParsingError> {
        self.expect_space()?;

        let token = self.expect_next_token()?;
        let r: Vec<u8> = token.iter().map(|v| *v).collect();

        fn char_is_name_safe(c: u8, is_first: bool) -> bool {
            match c {
                b'a'..=b'z' => true,
                b'A'..=b'Z' => true,
                b'0'..=b'9' => true,
                b'+' | b'/' | b';' | b'.' | b'$' | b'_' | b'(' | b')' => true,
                b'-' => !is_first, // - is only name safe outside first position
                _ => false,
            }
        }

        if r.iter()
            .enumerate()
            .all(|(i, c)| char_is_name_safe(*c, i == 0))
            && r.len() <= 200
        {
            Ok(r)
        } else {
            Err(ParsingError::BadFormat)
        }
    }

    /// Consumes a space.
    fn expect_space(&mut self) -> Result<(), ParsingError> {
        match self.from.get(0) {
            Some(b' ') => {
                self.from = &self.from[1..];
                Ok(())
            },
            _ => Err(ParsingError::BadFormat),
        }
    }

    /// Consumes from this ParseState until reaching a space byte or the end of
    /// the input. It returns None at the end of the input. On consecutive space
    /// bytes, it returns a zero-length slice.
    fn next_token(&mut self) -> Option<&[u8]> {
        if self.from.len() == 0 {
            return None;
        }

        let idx = self
            .from
            .iter()
            .position(|c| *c == b' ')
            .unwrap_or(self.from.len());

        let token = &self.from[..idx];
        self.from = &self.from[idx..];

        Some(token)
    }
}

impl<'a> From<&'a [u8]> for ParseState<'a> {
    fn from(from: &'a [u8]) -> Self {
        ParseState { from }
    }
}

// Parsing is implemented to fulfil the TryFrom trait.
impl TryFrom<&[u8]> for BeanstalkCommand {
    type Error = ParsingError;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        use BeanstalkCommand::*;

        let mut ps: ParseState = value.into();

        let cmd = match ps.expect_next_token()? {
            // <cmd>
            b"list-tube-used" => ListTubeUsed,
            b"list-tubes-watched" => ListTubesWatched,
            b"list-tubes" => ListTubes,
            b"peek-buried" => PeekBuried,
            b"peek-delayed" => PeekDelayed,
            b"peek-ready" => PeekReady,
            b"quit" => Quit,
            b"reserve" => Reserve,
            b"stats" => StatsServer,

            // <cmd> <id>
            b"delete" => Delete {
                id: ps.expect_next_u64()?,
            },
            b"kick" => Kick {
                bound: ps.expect_next_u64()?,
            },
            b"kick-job" => KickJob {
                id: ps.expect_next_u64()?,
            },
            b"peek" => Peek {
                id: ps.expect_next_u64()?,
            },
            b"reserve-job" => ReserveJob {
                id: ps.expect_next_u64()?,
            },
            b"stats-job" => StatsJob {
                id: ps.expect_next_u64()?,
            },
            b"touch" => Touch {
                id: ps.expect_next_u64()?,
            },

            // <cmd> <timeout>
            b"reserve-with-timeout" => ReserveWithTimeout {
                timeout: ps.expect_next_u32()?,
            },

            // <cmd> <tube>
            b"use" => Use {
                tube: ps.expect_next_name()?,
            },
            b"watch" => Watch {
                tube: ps.expect_next_name()?,
            },
            b"ignore" => Ignore {
                tube: ps.expect_next_name()?,
            },
            b"stats-tube" => StatsTube {
                tube: ps.expect_next_name()?,
            },

            // <cmd> <id> <pri>
            b"bury" => Bury {
                id: ps.expect_next_u64()?,
                pri: ps.expect_next_u32()?,
            },

            // <cmd> <tube> <delay>
            b"pause-tube" => PauseTube {
                tube: ps.expect_next_name()?,
                delay: ps.expect_next_u32()?,
            },

            // <cmd> <id> <pri> <delay>
            b"release" => Release {
                id: ps.expect_next_u64()?,
                pri: ps.expect_next_u32()?,
                delay: ps.expect_next_u32()?,
            },

            // <cmd> <pri> <delay> <ttr> <n_bytes>
            b"put" => Put {
                pri: ps.expect_next_u32()?,
                delay: ps.expect_next_u32()?,
                ttr: ps.expect_next_u32()?,
                n_bytes: ps.expect_next_u32()?,
            },

            _ => return Err(ParsingError::UnknownCommand),
        };

        ps.expect_done_and(cmd)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_command() {
        use BeanstalkCommand::*;
        use ParsingError::*;

        const U32_MAX_PLUS_1: u128 = 1 << 32 + 1;
        const U64_MAX_PLUS_1: u128 = 1 << 64 + 1;

        // Asserts the line parses into the given command successfully.
        #[track_caller]
        fn ok(line: &[u8], res: BeanstalkCommand) {
            assert_eq!(line.try_into(), Ok(res));
        }

        // Asserts the line fails to parse with a BadFormat error.
        #[track_caller]
        fn bf(line: &[u8]) {
            assert_eq!(
                TryInto::<BeanstalkCommand>::try_into(line),
                Err(BadFormat)
            );
        }

        // Asserts the line fails to parse with an UnknownCommand error.
        #[track_caller]
        fn uc(line: &[u8]) {
            assert_eq!(
                TryInto::<BeanstalkCommand>::try_into(line),
                Err(UnknownCommand)
            );
        }

        let name_200_bytes: String =
            (0..200).into_iter().map(|_| 'a').collect();
        let name_201_bytes: String =
            (0..201).into_iter().map(|_| 'a').collect();

        // Check silly non-commands
        bf(b"");
        bf(b" ");
        uc(b"syntax-error");

        // Check put with overflow protection.
        ok(
            b"put 987 654 321 123",
            Put {
                pri: 987,
                delay: 654,
                ttr: 321,
                n_bytes: 123,
            },
        );
        bf(format!("put {U32_MAX_PLUS_1} 0 0 0").as_bytes());
        bf(format!("put 0 {U32_MAX_PLUS_1} 0 0").as_bytes());
        bf(format!("put 0 0 {U32_MAX_PLUS_1} 0").as_bytes());
        bf(format!("put 0 0 0 {U32_MAX_PLUS_1}").as_bytes());

        // Check use with tube name requirements.
        ok(
            b"use tube_name_here-098+/;.()-",
            Use {
                tube: "tube_name_here-098+/;.()-".into(),
            },
        );
        bf(b"use foo bar");
        bf(b"use -foo");
        bf(b"use -");
        bf(b"use foo#bar");
        ok(
            format!("use {name_200_bytes}").as_bytes(),
            Use {
                tube: name_200_bytes.into(),
            },
        );
        bf(format!("use {name_201_bytes}").as_bytes());

        ok(b"reserve", Reserve);
        bf(b"reserve ");

        ok(
            b"reserve-with-timeout 123",
            ReserveWithTimeout { timeout: 123 },
        );
        bf(format!("reserve-with-timeout {U32_MAX_PLUS_1}").as_bytes());

        ok(b"reserve-job 987", ReserveJob { id: 987 });
        bf(format!("reserve-job {U64_MAX_PLUS_1}").as_bytes());

        ok(b"delete 321", Delete { id: 321 });
        bf(format!("delete {U64_MAX_PLUS_1}").as_bytes());

        ok(
            b"release 987 654 321",
            Release {
                id: 987,
                pri: 654,
                delay: 321,
            },
        );
        ok(b"bury 543 987", Bury { id: 543, pri: 987 });

        ok(b"touch 123", Touch { id: 123 });
        ok(
            b"watch hello_world",
            Watch {
                tube: "hello_world".into(),
            },
        );
        ok(
            b"ignore hello_world",
            Ignore {
                tube: "hello_world".into(),
            },
        );

        ok(b"peek 987", Peek { id: 987 });
        ok(b"peek-ready", PeekReady);
        ok(b"peek-delayed", PeekDelayed);
        ok(b"peek-buried", PeekBuried);

        ok(b"kick 999", Kick { bound: 999 });
        ok(b"kick-job 432", KickJob { id: 432 });

        ok(b"stats-job 432", StatsJob { id: 432 });
        ok(
            b"stats-tube hello_world",
            StatsTube {
                tube: "hello_world".into(),
            },
        );
        ok(b"stats", StatsServer);

        ok(b"list-tubes", ListTubes);
        ok(b"list-tube-used", ListTubeUsed);
        ok(b"list-tubes-watched", ListTubesWatched);

        ok(b"quit", Quit);

        ok(
            b"pause-tube hello_world 62",
            PauseTube {
                tube: "hello_world".into(),
                delay: 62,
            },
        );
    }
}
