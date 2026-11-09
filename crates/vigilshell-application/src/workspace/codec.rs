use encoding_rs::{CoderResult, Decoder, EncoderResult, Encoding, UTF_8};
use vigilshell_domain::workspace::WorkspaceError;
fn encoding(label: &str) -> Result<&'static Encoding, WorkspaceError> {
    Encoding::for_label(label.as_bytes()).ok_or(WorkspaceError::EncodingUnsupported)
}
pub(super) fn encode_text(label: &str, text: &str) -> Result<Vec<u8>, WorkspaceError> {
    let mut encoder = encoding(label)?.new_encoder();
    let mut bytes = Vec::with_capacity(text.len().saturating_mul(4).saturating_add(32));
    let (result, _) = encoder.encode_from_utf8_to_vec_without_replacement(text, &mut bytes, true);
    match result {
        EncoderResult::InputEmpty => Ok(bytes),
        EncoderResult::Unmappable(_) => Err(WorkspaceError::UnmappableInput),
        EncoderResult::OutputFull => Err(WorkspaceError::StreamUnavailable),
    }
}
pub(super) struct Codec {
    input: Decoder,
    output: Decoder,
    encoding: String,
}
impl Codec {
    pub fn new(label: &str) -> Result<Self, WorkspaceError> {
        Ok(Self {
            input: UTF_8.new_decoder_without_bom_handling(),
            output: encoding(label)?.new_decoder_without_bom_handling(),
            encoding: label.into(),
        })
    }
    pub fn input(&mut self, bytes: &[u8]) -> Result<Vec<u8>, WorkspaceError> {
        let mut text = String::with_capacity(bytes.len() * 3 + 16);
        let (result, _, errors) = self.input.decode_to_string(bytes, &mut text, false);
        if errors || result == CoderResult::OutputFull {
            return Err(WorkspaceError::StreamUnavailable);
        }
        encode_text(&self.encoding, &text)
    }
    pub fn output(&mut self, bytes: &[u8], last: bool) -> Result<Vec<u8>, WorkspaceError> {
        let mut text = String::with_capacity(bytes.len() * 3 + 16);
        let (result, _, _) = self.output.decode_to_string(bytes, &mut text, last);
        if result == CoderResult::OutputFull {
            return Err(WorkspaceError::StreamUnavailable);
        }
        Ok(text.into_bytes())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn encodings_roundtrip_split_bytes() {
        for label in ["utf-8", "gb18030", "gbk", "big5"] {
            let text = "中文測試";
            let bytes = encode_text(label, text).unwrap();
            let mut codec = Codec::new(label).unwrap();
            let mut out = Vec::new();
            for b in bytes {
                out.extend(codec.output(&[b], false).unwrap());
            }
            out.extend(codec.output(&[], true).unwrap());
            assert_eq!(String::from_utf8(out).unwrap(), text);
        }
    }
    #[test]
    fn rejects_unmappable_instead_of_sending_html_entities() {
        assert!(matches!(
            encode_text("gbk", "😀"),
            Err(WorkspaceError::UnmappableInput)
        ));
    }
    #[test]
    fn input_utf8_can_cross_frames() {
        let mut c = Codec::new("gbk").unwrap();
        let mut out = Vec::new();
        for b in "中文".as_bytes() {
            out.extend(c.input(&[*b]).unwrap());
        }
        assert_eq!(out, encode_text("gbk", "中文").unwrap());
    }
}
