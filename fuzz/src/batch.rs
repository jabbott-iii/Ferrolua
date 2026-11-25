//! The format of a batch, the inputs that the fuzzer hands one child process at a time: for each
//! input, its length as 8 bytes in little-endian order, then its bytes.

/// Encodes `inputs` as one batch.
pub(crate) fn encode(inputs: &[Vec<u8>]) -> Vec<u8> {
    let mut batch = Vec::with_capacity(inputs.iter().map(|input| 8 + input.len()).sum());
    for input in inputs {
        let length = u64::try_from(input.len()).expect("a length fits in 64 bits on every target");
        batch.extend_from_slice(&length.to_le_bytes());
        batch.extend_from_slice(input);
    }
    batch
}

/// Decodes a batch into its inputs, or returns `None` when a length or an input is cut short.
pub(crate) fn decode(mut batch: &[u8]) -> Option<Vec<&[u8]>> {
    let mut inputs = Vec::new();
    while !batch.is_empty() {
        let (length, rest) = batch.split_first_chunk::<8>()?;
        let length = u64::from_le_bytes(*length);
        // A length that doesn't fit in `usize` is longer than any input in memory, so it is cut
        // short too.
        let input = usize::try_from(length)
            .ok()
            .and_then(|length| rest.get(..length))?;
        inputs.push(input);
        batch = &rest[input.len()..];
    }
    Some(inputs)
}

#[cfg(test)]
mod tests {
    use super::{decode, encode};

    #[test]
    fn encodes_each_input_after_its_length() {
        let batch = encode(&[b"ab".to_vec(), Vec::new(), b"c".to_vec()]);
        assert_eq!(
            batch,
            [
                &[2, 0, 0, 0, 0, 0, 0, 0][..],
                b"ab",
                &[0; 8],
                &[1, 0, 0, 0, 0, 0, 0, 0],
                b"c"
            ]
            .concat()
        );
    }

    #[test]
    fn decodes_what_it_encodes() {
        let inputs = vec![b"FUZZ".to_vec(), Vec::new(), vec![0; 300]];
        let batch = encode(&inputs);
        assert_eq!(
            decode(&batch),
            Some(inputs.iter().map(Vec::as_slice).collect())
        );
    }

    #[test]
    fn decodes_an_empty_batch_as_no_inputs() {
        assert_eq!(decode(&[]), Some(Vec::new()));
    }

    #[test]
    fn rejects_a_length_cut_short() {
        assert_eq!(decode(&[1, 0, 0]), None);
    }

    #[test]
    fn rejects_an_input_cut_short() {
        let mut batch = encode(&[b"FUZZ".to_vec()]);
        batch.pop();
        assert_eq!(decode(&batch), None);
    }

    #[test]
    fn rejects_a_length_longer_than_any_input() {
        assert_eq!(decode(&u64::MAX.to_le_bytes()), None);
    }
}
