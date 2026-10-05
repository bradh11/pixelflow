//! Show-wide frame buffer shared between one producer (patterns, sequencer) and one
//! consumer (the output thread) without locks.
//!
//! Triple buffering: the producer always has a buffer to write, the consumer always has
//! the most recent complete frame, and neither ever waits for the other.

use triple_buffer::{Input, Output, triple_buffer};

/// Creates a connected writer/reader pair for frames of `len` bytes, initially all zero.
pub fn frame_buffers(len: usize) -> (FrameWriter, FrameReader) {
    let (input, output) = triple_buffer(&vec![0u8; len]);
    (FrameWriter { input }, FrameReader { output })
}

/// Producer side. Write a full frame into [`FrameWriter::frame_mut`], then [`FrameWriter::publish`].
pub struct FrameWriter {
    input: Input<Vec<u8>>,
}

impl FrameWriter {
    /// The buffer to fill for the next frame.
    ///
    /// Its contents are stale (an older frame), so producers must write every byte
    /// they care about each frame.
    pub fn frame_mut(&mut self) -> &mut [u8] {
        self.input.input_buffer_mut()
    }

    /// Makes the written frame the latest one visible to the reader.
    pub fn publish(&mut self) {
        self.input.publish();
    }
}

/// Consumer side.
pub struct FrameReader {
    output: Output<Vec<u8>>,
}

impl FrameReader {
    /// The most recently published frame (or zeros if nothing has been published yet).
    pub fn latest(&mut self) -> &[u8] {
        self.output.update();
        self.output.output_buffer()
    }

    /// True when a frame newer than the last [`FrameReader::latest`] call is waiting.
    pub fn has_new_frame(&self) -> bool {
        self.output.updated()
    }
}

impl std::fmt::Debug for FrameWriter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FrameWriter").finish_non_exhaustive()
    }
}

impl std::fmt::Debug for FrameReader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FrameReader").finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn reader_sees_zeros_before_first_publish() {
        let (_writer, mut reader) = frame_buffers(6);
        assert_eq!(reader.latest(), &[0; 6]);
        assert!(!reader.has_new_frame());
    }

    #[test]
    fn reader_sees_published_frame_but_not_unpublished_writes() {
        let (mut writer, mut reader) = frame_buffers(3);
        writer.frame_mut().copy_from_slice(&[1, 2, 3]);
        assert_eq!(reader.latest(), &[0, 0, 0]);
        writer.publish();
        assert!(reader.has_new_frame());
        assert_eq!(reader.latest(), &[1, 2, 3]);
        assert!(!reader.has_new_frame());
    }

    #[test]
    fn reader_gets_latest_of_several_publishes() {
        let (mut writer, mut reader) = frame_buffers(1);
        for value in 1..=5u8 {
            writer.frame_mut()[0] = value;
            writer.publish();
        }
        assert_eq!(reader.latest(), &[5]);
    }

    #[test]
    fn reader_never_sees_torn_or_older_frames_while_writer_runs() {
        let (mut writer, mut reader) = frame_buffers(64);
        let producer = thread::spawn(move || {
            for value in 1..=200u8 {
                writer.frame_mut().fill(value);
                writer.publish();
            }
        });
        let mut last = 0u8;
        loop {
            // Sampled before the read, so a finished producer's last frame is visible to it.
            let finished = producer.is_finished();
            let frame = reader.latest();
            let first = frame[0];
            assert!(frame.iter().all(|&b| b == first), "torn frame: {frame:?}");
            assert!(first >= last, "frame went backwards: {first} after {last}");
            last = first;
            if last == 200 || finished {
                break;
            }
        }
        producer.join().unwrap();
        assert!(reader.latest().iter().all(|&b| b == 200));
    }
}
