//! Server-sent events: the `event:` / `data:` frames both providers stream replies in.

use std::io::{self, BufRead, Read};

/// One event: its name (empty when the stream doesn't name events) and its data lines joined.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SseEvent {
    pub event: String,
    pub data: String,
}

/// Reads events from a stream as they arrive.
pub struct SseReader<R> {
    inner: R,
    line: String,
}

/// Longest line accepted (a reply's single chunk is far smaller).
const MAX_LINE: usize = 4 * 1024 * 1024;

impl<R: BufRead> SseReader<R> {
    pub fn new(inner: R) -> Self {
        Self {
            inner,
            line: String::new(),
        }
    }

    /// The next event, or `None` at the end of the stream.
    pub fn next_event(&mut self) -> io::Result<Option<SseEvent>> {
        let mut event = String::new();
        let mut data: Option<String> = None;
        loop {
            self.line.clear();
            let read = Read::take(&mut self.inner, MAX_LINE as u64).read_line(&mut self.line)?;
            if read == 0 {
                // End of stream: a last event without its blank line still counts.
                return Ok(data.map(|data| SseEvent { event, data }));
            }
            let line = self.line.trim_end_matches(['\n', '\r']);
            if line.is_empty() {
                if let Some(data) = data.take() {
                    return Ok(Some(SseEvent { event, data }));
                }
                event.clear();
                continue;
            }
            if line.starts_with(':') {
                continue;
            }
            let (field, value) = match line.split_once(':') {
                Some((field, value)) => (field, value.strip_prefix(' ').unwrap_or(value)),
                None => (line, ""),
            };
            match field {
                "event" => event = value.to_string(),
                "data" => match &mut data {
                    Some(existing) => {
                        existing.push('\n');
                        existing.push_str(value);
                    }
                    None => data = Some(value.to_string()),
                },
                _ => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn events(text: &str) -> Vec<SseEvent> {
        let mut reader = SseReader::new(text.as_bytes());
        let mut out = Vec::new();
        while let Some(event) = reader.next_event().unwrap() {
            out.push(event);
        }
        out
    }

    #[test]
    fn named_events_comments_and_crlf() {
        let got = events(
            "event: message_start\r\ndata: {\"a\":1}\r\n\r\n: keep-alive\n\nevent: ping\ndata: {}\n\n",
        );
        assert_eq!(
            got,
            [
                SseEvent {
                    event: "message_start".into(),
                    data: "{\"a\":1}".into()
                },
                SseEvent {
                    event: "ping".into(),
                    data: "{}".into()
                },
            ]
        );
    }

    #[test]
    fn unnamed_events_multiline_data_and_a_missing_last_blank_line() {
        let got = events("data: one\ndata: two\n\ndata:[DONE]");
        assert_eq!(got[0].data, "one\ntwo");
        assert_eq!(got[0].event, "");
        assert_eq!(got[1].data, "[DONE]");
    }
}
