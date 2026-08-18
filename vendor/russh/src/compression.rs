use std::convert::TryFrom;

use delegate::delegate;
use ssh_encoding::Encode;

#[derive(Debug, Clone)]
pub enum Compression {
    None,
}

#[derive(Debug)]
pub enum Compress {
    None,
}

#[derive(Debug)]
pub enum Decompress {
    None,
}

#[derive(Debug, PartialEq, Eq, Copy, Clone, Hash)]
pub struct Name(&'static str);

impl AsRef<str> for Name {
    fn as_ref(&self) -> &str {
        self.0
    }
}

impl Encode for Name {
    delegate! { to self.as_ref() {
        fn encoded_len(&self) -> Result<usize, ssh_encoding::Error>;
        fn encode(&self, writer: &mut impl ssh_encoding::Writer) -> Result<(), ssh_encoding::Error>;
    }}
}

impl TryFrom<&str> for Name {
    type Error = ();

    fn try_from(s: &str) -> Result<Name, ()> {
        ALL_COMPRESSION_ALGORITHMS
            .iter()
            .find(|x| x.0 == s)
            .map(|x| **x)
            .ok_or(())
    }
}

pub const NONE: Name = Name("none");

pub const ALL_COMPRESSION_ALGORITHMS: &[&Name] = &[&NONE];

impl Compression {
    pub fn new(_name: &Name) -> Self {
        Compression::None
    }

    pub fn init_compress(&self, _: &mut Compress) {}

    pub fn init_decompress(&self, _: &mut Decompress) {}

    pub fn is_deferred(&self) -> bool {
        false
    }
}

impl Compress {
    pub fn compress<'a>(
        &mut self,
        input: &'a [u8],
        _: &'a mut Vec<u8>,
    ) -> Result<&'a [u8], crate::Error> {
        Ok(input)
    }

    pub fn compress_into(
        &mut self,
        input: &[u8],
        output: &mut Vec<u8>,
        start_len: usize,
    ) -> Result<usize, crate::Error> {
        output.truncate(start_len);
        output.extend_from_slice(input);
        Ok(input.len())
    }
}

impl Decompress {
    pub fn decompress<'a>(
        &mut self,
        input: &'a [u8],
        _: &'a mut Vec<u8>,
    ) -> Result<&'a [u8], crate::Error> {
        Ok(input)
    }
}
