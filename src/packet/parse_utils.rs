//! Helper functions and macros related to parsing.

/// Consume an entire SCTP Tag-Length-Value (TLV) structure and return a processed value according
/// to the provided dispatch function.  (A closure doesn't seem to work due to lifetime issues.)
macro_rules! parse_tlv (
    ($i:expr, $dispatch_function:expr) => ({
        use nom::error::ErrorKind;
        use nom::Err;
        use nom::number::complete::be_u16;
        const TAG_LENGTH_HEADER_SIZE: usize = 4;
        let input = $i;
        if input.len() < TAG_LENGTH_HEADER_SIZE {
            // underrun TODO: real error
            Err(Err::Error((input,ErrorKind::Count)))
        } else {
            // Parse tag
            match be_u16(input) {
                Err(e) => Err(e),
                Ok((i, tag)) => {
                    // Parse length
                    match be_u16(i) {
                        Err(e) => Err(e),
                        Ok((i, length)) => {
                            // Validate length
                            if (length as usize) < TAG_LENGTH_HEADER_SIZE {
                                // invalid length field TODO: real error
                                Err(Err::Error((i,ErrorKind::Count)))
                            } else {
                                // Subtract the header size to get the value length
                                let length = length as usize - TAG_LENGTH_HEADER_SIZE;
                                // Account for padding
                                let padding = (4 - length % 4) % 4;
                                let padded_length = length + padding;
                                if length > i.len() {
                                    // not incomplete -- we should always have the full TLV
                                    Err(Err::Error((i,ErrorKind::Count)))
                                } else {
                                    // Split slices into the data which is part of this TLV
                                    // (not including padding) and the rest of the input stream
                                    // which follows any trailing padding.
                                    let value_data = &i[..length];
                                    let total_length = i.len();
                                    let remaining_input = if padded_length <= total_length {
                                        &i[padded_length..]
                                    } else {
                                        // The last item is allowed to omit padding
                                        &i[i.len()..]
                                    };

                                    // Dispatch
                                    match $dispatch_function(tag, value_data) {
                                        Ok((i,value)) => {
                                            // The value data should be completely consumed
                                            if i.len() != 0 {
                                                Err(Err::Error((i,ErrorKind::Count)))
                                            } else {
                                                Ok((remaining_input, value))
                                            }
                                        },
                                        Err(Err::Incomplete(_)) => {
                                            // The TLV parser should always have complete data
                                            Err(Err::Error((i,ErrorKind::Count)))
                                        },
                                        Err(e) => Err(e),
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    });
    ($i:expr,) => ( parse_tlv!($i) );
);
