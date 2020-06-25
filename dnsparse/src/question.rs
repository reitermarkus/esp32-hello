use core::fmt;
use core::str;

use crate::{ResponseCode, QueryKind, QueryClass};

/// A DNS question.
#[repr(C)]
pub struct Question<'a> {
  buf: &'a [u8],
}

impl fmt::Debug for Question<'_> {
  fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
    fmt.debug_struct("Question")
      .field("name", &self.name())
      .field("kind", &self.kind())
      .field("class", &self.class())
      .finish()
  }
}

/// A DNS question name.
#[derive(Debug)]
pub struct QuestionName<'a> {
  buf: &'a [u8],
}

impl fmt::Display for QuestionName<'_> {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    let mut i = 0;

    while i < self.buf.len() {
      let len = self.buf[i] as usize;

      if len == 0 {
        break
      }

      if i != 0 {
        ".".fmt(f)?;
      }

      i += 1;

      let s = unsafe { str::from_utf8_unchecked(&self.buf[i..(i + len)]) };

      s.fmt(f)?;

      i += len;
    }

    Ok(())
  }
}

impl PartialEq<&str> for QuestionName<'_> {
  fn eq(&self, other: &&str) -> bool {
    let mut i = 0;

    while i < self.buf.len() {
      let len = self.buf[i] as usize;

      if len == 0 {
        break
      }

      if i != 0 && other.get((i - 1)..i) != Some(".") {
        return false
      }

      i += 1;

      let s = unsafe { str::from_utf8_unchecked(&self.buf[i..(i + len)]) };

      if let Some(substring) = other.get((i - 1)..(i - 1 + len)) {
        if s != substring {
          return false
        }
      } else {
        return false
      }

      i += len;
    }

    i == other.len() + 1
  }
}

impl<'a> Question<'a> {
  pub fn name(&self) -> QuestionName<'a> {
    QuestionName { buf: &self.buf[0..(self.buf.len() - 5)] }
  }

  pub fn kind(&self) -> QueryKind {
    let b0 = self.buf.len() - 4;
    let b1 = b0 + 1;
    QueryKind::from(u16::from_be_bytes([self.buf[b0], self.buf[b1]]))
  }

  pub fn class(&self) -> QueryClass {
    let b0 = self.buf.len() - 2;
    let b1 = b0 + 1;
    QueryClass::from(u16::from_be_bytes([self.buf[b0], self.buf[b1]]))
  }

  pub fn as_bytes(&self) -> &'a [u8] {
    self.buf
  }
}

/// Iterator over [`Question`](struct.Question.html)s contained in a [`DnsFrame`](struct.DnsFrame.html).
pub struct Questions<'a> {
  pub(crate) question_count: usize,
  pub(crate) current_question: usize,
  pub(crate) buf: &'a [u8],
  pub(crate) buf_i: usize,
}

impl<'a> Iterator for Questions<'a> {
  type Item = Result<Question<'a>, ResponseCode>;

  fn next(&mut self) -> Option<Self::Item> {
    if self.current_question >= self.question_count {
      return None
    }

    let mut len = 0;

    loop {
      let i = self.buf_i + len;

      let part_len = if let Some(&len) = self.buf.get(i) {
        len as usize
      } else {
        return Some(Err(ResponseCode::FormatError))
      };

      if part_len == 0 {
        if i + 5 > self.buf.len() {
          return Some(Err(ResponseCode::FormatError))
        } else {
          len += 5;
        }

        let question = Question { buf: &self.buf[self.buf_i..(self.buf_i + len)] };

        self.current_question += 1;
        self.buf_i += len;

        return Some(Ok(question))
      } else if i + 1 + part_len > self.buf.len() {
        return Some(Err(ResponseCode::FormatError))
      }

      len += part_len + 1;
    }
  }
}

