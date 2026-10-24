use super::*;

// Locate one complete frame without allocating values or rescanning earlier chunks.
pub(super) struct Frame {
    position: usize,
    line_search: usize,
    bulk_end: Option<usize>,
    remaining: Vec<i64>,
    #[cfg(test)]
    examined: usize,
}
impl Default for Frame {
    fn default() -> Self {
        Self {
            position: 0,
            line_search: 0,
            bulk_end: None,
            remaining: vec![1],
            #[cfg(test)]
            examined: 0,
        }
    }
}
impl Frame {
    pub(super) fn advance(&mut self, bytes: &[u8]) -> Result<Option<usize>> {
        loop {
            if let Some(end) = self.bulk_end {
                if bytes.len() < end {
                    return Ok(None);
                }
                if bytes[end - 2..end] != *b"\r\n" {
                    bail!("RESP 数据未正确结束");
                }
                self.position = end;
                self.bulk_end = None;
                self.line_search = end;
            }
            while self.remaining.last() == Some(&0) {
                self.remaining.pop();
            }
            if self.remaining.is_empty() {
                return Ok(Some(self.position));
            }
            if self.remaining.len() > 129 {
                bail!("RESP 响应嵌套过深");
            }
            if self.position == bytes.len() {
                return Ok(None);
            }
            let start = self.line_search.max(self.position + 1);
            let next = bytes[start..].iter().position(|byte| *byte == b'\n');
            #[cfg(test)]
            {
                self.examined += next.map_or(bytes.len() - start, |n| n + 1);
            }
            let Some(newline) = next.map(|n| start + n) else {
                self.line_search = bytes.len();
                return Ok(None);
            };
            if newline == self.position + 1 || bytes[newline - 1] != b'\r' {
                bail!("RESP 响应行未正确结束");
            }
            let tag = bytes[self.position];
            let line = &bytes[self.position + 1..newline - 1];
            self.position = newline + 1;
            self.line_search = self.position;
            *self.remaining.last_mut().unwrap() -= 1;
            match tag {
                b'+' | b'-' | b':' | b'_' | b'#' | b',' | b'(' => {}
                b'$' | b'!' | b'=' => {
                    let size: i64 = std::str::from_utf8(line)?.parse()?;
                    if size == -1 {
                        continue;
                    }
                    if !(0..=512 * 1024 * 1024).contains(&size) {
                        bail!("RESP 字符串长度异常");
                    }
                    self.bulk_end = Some(self.position + size as usize + 2);
                }
                b'*' | b'~' | b'>' | b'%' | b'|' => {
                    let mut size: i64 = std::str::from_utf8(line)?.parse()?;
                    if size == -1 {
                        continue;
                    }
                    if tag == b'%' || tag == b'|' {
                        size = size
                            .checked_mul(2)
                            .ok_or_else(|| anyhow!("RESP 数组长度异常"))?;
                    }
                    if !(0..=10_000_000).contains(&size) {
                        bail!("RESP 数组长度异常");
                    }
                    self.remaining.push(size + i64::from(tag == b'|'));
                }
                _ => bail!("未知 RESP 类型：{tag}"),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fragmented_frames_match_the_decoder_and_scan_linearly() {
        let payload = b"|1\r\n+meta\r\n+value\r\n*3\r\n$5\r\na\0b\r\n\r\n%1\r\n+k\r\n:9\r\n*0\r\n";
        let expected = read_value(&mut io::Cursor::new(payload)).unwrap();
        for chunk in 1..=payload.len() {
            let mut frame = Frame::default();
            let mut complete = None;
            for end in (chunk..payload.len())
                .step_by(chunk)
                .chain(std::iter::once(payload.len()))
            {
                complete = frame.advance(&payload[..end]).unwrap();
            }
            assert_eq!(complete, Some(payload.len()));
            assert!(frame.examined <= payload.len());
            assert_eq!(
                read_value(&mut io::Cursor::new(&payload[..complete.unwrap()])).unwrap(),
                expected
            );
        }
        for invalid in [
            b"*9223372036854775807\r\n".as_slice(),
            b"$-2\r\n",
            b"$1\r\naXX",
            b"?bad\r\n",
        ] {
            assert!(Frame::default().advance(invalid).is_err());
        }
    }
    #[test]
    fn array_fragment_work_is_bounded_by_input_length() {
        for count in [8192, 16384, 32768] {
            let mut bytes = format!("*{count}\r\n").into_bytes();
            for _ in 0..count {
                bytes.extend_from_slice(b"$4\r\ntest\r\n");
            }
            let mut frame = Frame::default();
            for end in (1..bytes.len()).step_by(17) {
                frame.advance(&bytes[..end]).unwrap();
            }
            assert_eq!(frame.advance(&bytes).unwrap(), Some(bytes.len()));
            assert!(frame.examined <= bytes.len());
        }
    }
}
