#![no_std]

//! See [RFC 1035](https://tools.ietf.org/rfc/rfc1035).

mod frame;
pub use frame::DnsFrame;

mod header;
pub use header::{DnsHeader, HeaderKind, ResponseCode, OpCode, QueryKind, QueryClass};

mod question;
pub use question::{Question, QuestionName, Questions};
