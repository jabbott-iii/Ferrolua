//! The child side of a run: the fuzzer starts its own program again as
//! `ferrolua-fuzz --child <target>` for each batch of inputs, so that a crash or a hang ends only
//! the child, and the parent sees it and carries on.
//!
//! The child reads the batch from standard input, in the format of the [`batch`]
//! module. Before it runs each input, it writes a newline to standard output, which the parent
//! sends to a file: the number of newlines is the number of inputs the child has started, so when
//! a child crashes or hangs, the last one it started is the input that did it. When an input
//! panics, the child writes `!` and the panic's message after the newlines and stops, so the
//! message reaches the parent too.

use std::any::Any;
use std::io::{Read, Write};
use std::panic::{self, AssertUnwindSafe};

use crate::batch;
use crate::error::Error;
use crate::target::Target;

/// The status a child process exits with when an input panicked. It is the status that Rust gives
/// a program that a panic ends.
pub(crate) const PANICKED: u8 = 101;

/// How a child's batch ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Ran {
    /// Every input ran without a panic.
    All,
    /// An input panicked, and the child wrote the panic's message.
    Panicked,
}

impl Ran {
    /// The status the child process exits with.
    pub(crate) fn status(self) -> u8 {
        match self {
            Ran::All => 0,
            Ran::Panicked => PANICKED,
        }
    }
}

/// Runs `target` on each input of the batch read from `input`, writing the progress to
/// `progress`, and stops at the first input that panics.
///
/// Each panic still goes to the panic hook first, so the child process replaces the default
/// hook, which would print each panic's message and location, with one that prints nothing.
///
/// # Errors
///
/// Returns [`Error::Batch`] when the batch is cut short, and [`Error::Child`] when the batch can't
/// be read or the progress can't be written.
pub(crate) fn run(
    target: &Target,
    input: &mut impl Read,
    progress: &mut impl Write,
) -> Result<Ran, Error> {
    let mut bytes = Vec::new();
    let inputs = input
        .read_to_end(&mut bytes)
        .map_err(Error::Child)
        .and_then(|_| batch::decode(&bytes).ok_or(Error::Batch))?;
    for input in inputs {
        progress.write_all(b"\n").map_err(Error::Child)?;
        // `AssertUnwindSafe` is sound here: after a panic, the child reads nothing the target
        // touched, and stops.
        if let Err(payload) = panic::catch_unwind(AssertUnwindSafe(|| (target.run)(input))) {
            let message = panic_message(payload.as_ref());
            return progress
                .write_all(b"!")
                .and_then(|()| progress.write_all(message.as_bytes()))
                .and_then(|()| progress.flush())
                .map(|()| Ran::Panicked)
                .map_err(Error::Child);
        }
    }
    progress.flush().map(|()| Ran::All).map_err(Error::Child)
}

/// Describes a panic from its payload: the message of `panic!`, which is a `&str` or a `String`.
fn panic_message(payload: &(dyn Any + Send)) -> String {
    if let Some(message) = payload.downcast_ref::<&str>() {
        (*message).to_owned()
    } else if let Some(message) = payload.downcast_ref::<String>() {
        message.clone()
    } else {
        String::from("a panic whose payload isn't a string")
    }
}

#[cfg(test)]
mod tests {
    use std::io::{self, Read, Write};

    use super::{PANICKED, Ran, panic_message, run};
    use crate::batch;
    use crate::error::Error;
    use crate::target;

    /// A stream whose every read and write fails, as a closed pipe's does.
    struct Broken;

    impl Read for Broken {
        fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
            Err(io::Error::new(io::ErrorKind::BrokenPipe, "closed"))
        }
    }

    impl Write for Broken {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            Err(io::Error::new(io::ErrorKind::BrokenPipe, "closed"))
        }

        fn flush(&mut self) -> io::Result<()> {
            Err(io::Error::new(io::ErrorKind::BrokenPipe, "closed"))
        }
    }

    fn panic_target() -> &'static target::Target {
        target::find("fixture-panic").unwrap()
    }

    #[test]
    fn writes_a_newline_for_each_input_it_starts() {
        let batch = batch::encode(&[b"a".to_vec(), Vec::new(), b"FUZ".to_vec()]);
        let mut progress = Vec::new();
        let ran = run(panic_target(), &mut batch.as_slice(), &mut progress).unwrap();
        assert_eq!(ran, Ran::All);
        assert_eq!(progress, b"\n\n\n");
    }

    #[test]
    fn stops_at_the_first_panic_and_writes_its_message() {
        let batch = batch::encode(&[b"a".to_vec(), b"FUZZ".to_vec(), b"b".to_vec()]);
        let mut progress = Vec::new();
        let ran = run(panic_target(), &mut batch.as_slice(), &mut progress).unwrap();
        assert_eq!(ran, Ran::Panicked);
        assert_eq!(progress, b"\n\n!the input starts with FUZZ");
    }

    #[test]
    fn rejects_a_batch_cut_short() {
        let mut progress = Vec::new();
        let error = run(panic_target(), &mut [1, 0].as_slice(), &mut progress).unwrap_err();
        assert!(matches!(error, Error::Batch));
        assert_eq!(progress, b"");
    }

    #[test]
    fn fails_when_the_batch_cannot_be_read() {
        let error = run(panic_target(), &mut Broken, &mut Vec::new()).unwrap_err();
        assert!(
            matches!(error, Error::Child(source) if source.kind() == io::ErrorKind::BrokenPipe)
        );
    }

    #[test]
    fn fails_when_the_progress_cannot_be_written() {
        let batch = batch::encode(&[b"a".to_vec()]);
        let error = run(panic_target(), &mut batch.as_slice(), &mut Broken).unwrap_err();
        assert!(
            matches!(error, Error::Child(source) if source.kind() == io::ErrorKind::BrokenPipe)
        );
    }

    #[test]
    fn fails_when_the_progress_cannot_be_flushed() {
        let batch = batch::encode(&[]);
        let error = run(panic_target(), &mut batch.as_slice(), &mut Broken).unwrap_err();
        assert!(
            matches!(error, Error::Child(source) if source.kind() == io::ErrorKind::BrokenPipe)
        );
    }

    /// A stream that takes `room` bytes and then fails, as a full disk does. Flushing it fails
    /// once it is full.
    struct Full {
        room: usize,
    }

    impl Write for Full {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if self.room == 0 {
                return Err(io::Error::new(io::ErrorKind::StorageFull, "full"));
            }
            let written = bytes.len().min(self.room);
            self.room -= written;
            Ok(written)
        }

        fn flush(&mut self) -> io::Result<()> {
            if self.room == 0 {
                Err(io::Error::new(io::ErrorKind::StorageFull, "full"))
            } else {
                Ok(())
            }
        }
    }

    /// What the child writes for a batch whose only input panics.
    const PANIC_PROGRESS: &[u8] = b"\n!the input starts with FUZZ";

    #[test]
    fn writes_the_panic_message_to_a_stream_with_room_for_it() {
        let batch = batch::encode(&[b"FUZZ".to_vec()]);
        let mut full = Full {
            room: PANIC_PROGRESS.len() + 1,
        };
        let ran = run(panic_target(), &mut batch.as_slice(), &mut full).unwrap();
        assert_eq!(ran, Ran::Panicked);
        assert_eq!(full.room, 1);
    }

    #[test]
    fn fails_when_the_panic_message_cannot_be_written() {
        let batch = batch::encode(&[b"FUZZ".to_vec()]);
        let error = run(panic_target(), &mut batch.as_slice(), &mut Full { room: 1 }).unwrap_err();
        assert!(
            matches!(error, Error::Child(source) if source.kind() == io::ErrorKind::StorageFull)
        );
    }

    #[test]
    fn fails_when_the_panic_message_cannot_be_flushed() {
        let batch = batch::encode(&[b"FUZZ".to_vec()]);
        let mut full = Full {
            room: PANIC_PROGRESS.len(),
        };
        let error = run(panic_target(), &mut batch.as_slice(), &mut full).unwrap_err();
        assert!(
            matches!(error, Error::Child(source) if source.kind() == io::ErrorKind::StorageFull)
        );
    }

    #[test]
    fn exits_with_status_0_or_the_status_of_a_panic() {
        assert_eq!(Ran::All.status(), 0);
        assert_eq!(Ran::Panicked.status(), PANICKED);
    }

    #[test]
    fn reads_the_message_of_each_kind_of_payload() {
        assert_eq!(panic_message(&"static"), "static");
        assert_eq!(panic_message(&String::from("formatted")), "formatted");
        assert_eq!(
            panic_message(&42_u8),
            "a panic whose payload isn't a string"
        );
    }
}
