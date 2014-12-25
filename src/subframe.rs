use error::{FlacError, FlacResult};

#[deriving(Copy)]
enum SubframeType {
    Constant,
    Verbatim,
    Fixed(u8),
    Lpc(u8)
}

#[deriving(Copy)]
struct SubframeHeader {
    sf_type: SubframeType,
    wasted_bits_per_sample: u8
}

fn read_subframe_header(input: &mut Reader) -> FlacResult<SubframeHeader> {
    // TODO
    let subframe_header = SubframeHeader {
        sf_type: SubframeType::Constant,
        wasted_bits_per_sample: 0
    };
    Ok(subframe_header)
}
