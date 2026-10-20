//! Server-sent-events parsing for streaming completions.
//!
//! Feed it bytes as they arrive; it hands back complete messages.
//! Chunk boundaries never split a message on the way out — partial
//! data waits in an internal buffer until the next `feed`.

/// One decoded SSE message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SseMessage {
    /// The `event:` field, if the server sent one.
    pub event: Option<String>,
    /// All `data:` lines joined with newlines.
    pub data: String,
}

/// An incremental SSE parser.
#[derive(Debug, Default)]
pub struct SseParser {
    buf: String,
}

impl SseParser {
    pub fn new() -> Self {
        SseParser { buf: String::new() }
    }

    /// Feed a chunk; returns any messages that became complete.
    pub fn feed(&mut self, chunk: &[u8]) -> Vec<SseMessage> {
        self.buf.push_str(&String::from_utf8_lossy(chunk));
        // Normalize CRLF to LF so "\r\n\r\n" boundaries split the same
        // way "\n\n" does (a lone trailing "\r" waits for its "\n").
        if self.buf.contains('\r') {
            self.buf = self.buf.replace("\r\n", "\n");
        }
        let mut out = Vec::new();

        // Messages are separated by a blank line.
        while let Some(pos) = self.buf.find("\n\n") {
            let block: String = self.buf.drain(..pos + 2).collect();
            if let Some(msg) = parse_block(&block) {
                out.push(msg);
            }
        }
        out
    }

    /// Flush at end of stream: a trailing block without a final blank
    /// line is still a message if it has data.
    pub fn finish(&mut self) -> Vec<SseMessage> {
        if self.buf.is_empty() {
            return Vec::new();
        }
        let rest = std::mem::take(&mut self.buf);
        match parse_block(&rest) {
            Some(msg) => vec![msg],
            None => Vec::new(),
        }
    }
}

fn parse_block(block: &str) -> Option<SseMessage> {
    let mut event: Option<String> = None;
    let mut data_lines: Vec<String> = Vec::new();
    let mut saw_any = false;

    for line in block.lines() {
        let line = line.trim_end_matches('\r');
        if line.is_empty() || line.starts_with(':') {
            continue; // separator or comment
        }
        saw_any = true;
        if let Some(value) = line.strip_prefix("event:") {
            event = Some(value.trim_start().to_string());
        } else if let Some(value) = line.strip_prefix("data:") {
            data_lines.push(value.trim_start().to_string());
        }
        // id: and retry: are ignored by design
    }

    if !saw_any || data_lines.is_empty() {
        return None;
    }
    Some(SseMessage { event, data: data_lines.join("\n") })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn complete_messages_parse() {
        let mut p = SseParser::new();
        let msgs = p.feed(b"data: {\"a\":1}\n\nevent: done\ndata: bye\n\n");
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].data, "{\"a\":1}");
        assert_eq!(msgs[0].event, None);
        assert_eq!(msgs[1].event.as_deref(), Some("done"));
        assert_eq!(msgs[1].data, "bye");
    }

    #[test]
    fn chunk_boundaries_do_not_split_messages() {
        let mut p = SseParser::new();
        assert!(p.feed(b"data: hel").is_empty());
        assert!(p.feed(b"lo\n").is_empty());
        let msgs = p.feed(b"\ndata: second\n\n");
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].data, "hello");
        assert_eq!(msgs[1].data, "second");
    }

    #[test]
    fn multi_line_data_joins_with_newlines() {
        let mut p = SseParser::new();
        let msgs = p.feed(b"data: line1\ndata: line2\n\n");
        assert_eq!(msgs[0].data, "line1\nline2");
    }

    #[test]
    fn comments_and_crlf_are_tolerated() {
        let mut p = SseParser::new();
        let msgs = p.feed(b": keep-alive\r\ndata: x\r\n\r\n");
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].data, "x");
    }

    #[test]
    fn finish_flushes_trailing_block() {
        let mut p = SseParser::new();
        assert!(p.feed(b"data: tail").is_empty());
        let msgs = p.finish();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].data, "tail");
    }
}
