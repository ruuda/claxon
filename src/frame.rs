use std::num::{Int, UnsignedInt};
use bitstream::Bitstream;
use crc::Crc8Reader;
use error::{FlacError, FlacResult};
use subframe::SubframeDecoder;

#[deriving(Copy)]
enum BlockingStrategy {
    Fixed,
    Variable
}

#[deriving(Copy)]
enum BlockTime {
    FrameNumber(u32),
    SampleNumber(u64)
}

#[deriving(Copy, Show)] // TODO: should not derive show.
enum ChannelMode {
    /// The channels are coded as-is.
    Raw,
    /// Channel 0 is the left channel, channel 1 is the side channel.
    LeftSideStereo,
    /// Channel 0 is the side channel, channel 1 is the right channel.
    RightSideStereo,
    /// Channel 0 is the mid channel, channel 1 is the side channel.
    MidSideStereo
}

#[deriving(Copy)]
struct FrameHeader {
    pub block_time: BlockTime,
    pub block_size: u16,
    pub sample_rate: Option<u32>,
    pub n_channels: u8,
    pub channel_mode: ChannelMode,
    pub bits_per_sample: Option<u8>
}

/// Reads a variable-length integer encoded as what is called "UTF-8" coding
/// in the specification. (It is not real UTF-8.) This function can read
/// integers encoded in this way up to 36-bit integers.
fn read_var_length_int(input: &mut Reader) -> FlacResult<u64> {
    use std::iter::range_step_inclusive;
    // The number of consecutive 1s followed by a 0 is the number of additional
    // bytes to read.
    let first = try!(input.read_byte());
    let mut read_additional = 0u;
    let mut mask_data = 0b0111_1111u8;
    let mut mask_mark = 0b1000_0000u8;

    // Determine the number of leading 1s.
    while first & mask_mark != 0 {
        read_additional = read_additional + 1;
        mask_data = mask_data >> 1;
        mask_mark = mask_mark >> 1;
    }

    // A single leading 1 is a follow-up byte and thus invalid.
    if read_additional > 0 {
        if read_additional == 1 {
            return Err(FlacError::InvalidVarLengthInt);
        } else {
            // The number of 1s (if > 1) is the total number of bytes, not the
            // number of additional bytes.
            read_additional = read_additional - 1;
        }
    }

    // Each additional byte will yield 6 extra bits, so shift the most
    // significant bits into the correct position.
    let mut result = (first & mask_data) as u64 << (6 * read_additional);
    for i in range_step_inclusive(read_additional as int - 1, 0, -1) {
        let byte = try!(input.read_byte());

        // The two most significant bits _must_ be 10.
        if byte & 0b1100_0000 != 0b1000_0000 {
            return Err(FlacError::InvalidVarLengthInt);
        }

        result = result | ((byte & 0b0011_1111) as u64 << (6 * i as uint));
    }

    Ok(result)
}

#[test]
fn verify_read_var_length_int() {
    use std::io::MemReader;

    let mut reader = MemReader::new(vec!(0x24, 0xc2, 0xa2, 0xe2, 0x82, 0xac,
                                         0xf0, 0x90, 0x8d, 0x88, 0xc2, 0x00,
                                         0x80));
    assert_eq!(read_var_length_int(&mut reader).unwrap(), 0x24);
    assert_eq!(read_var_length_int(&mut reader).unwrap(), 0xa2);
    assert_eq!(read_var_length_int(&mut reader).unwrap(), 0x20ac);
    assert_eq!(read_var_length_int(&mut reader).unwrap(), 0x010348);
    // Two-byte integer with invalid continuation byte should fail.
    assert_eq!(read_var_length_int(&mut reader).err().unwrap(),
               FlacError::InvalidVarLengthInt);
    // Continuation byte can never be the first byte.
    assert_eq!(read_var_length_int(&mut reader).err().unwrap(),
               FlacError::InvalidVarLengthInt);
}

fn read_frame_header(input: &mut Reader) -> FlacResult<FrameHeader> {
    // The frame header includes a CRC-8 at the end. It can be computed
    // automatically while reading, by wrapping the input reader in a reader
    // that computes the CRC.
    let mut crc_input = Crc8Reader::new(input);

    // First are 14 bits frame sync code, a reserved bit, and blocking stategy.
    let sync_res_block = try!(crc_input.read_be_u16());

    // The first 14 bits must be 11111111111110.
    let sync_code = sync_res_block & 0b1111_1111_1111_1100;
    if sync_code != 0b1111_1111_1111_1000 {
        return Err(FlacError::MissingFrameSyncCode);
    }

    // The next bit has a mandatory value of 0 (at the moment of writing, if
    // the bit has a different value, it could be a future stream that we
    // cannot read).
    if sync_res_block & 0b0000_0000_0000_0010 != 0 {
        return Err(FlacError::InvalidFrameHeader);
    }

    // The final bit determines the blocking strategy.
    let blocking_strategy = if sync_res_block & 0b0000_0000_0000_0001 == 0 {
        BlockingStrategy::Fixed
    } else {
        BlockingStrategy::Variable
    };

    // Next are 4 bits block size and 4 bits sample rate.
    let bs_sr = try!(crc_input.read_byte());
    let mut block_size = 0u16;
    let mut read_8bit_bs = false;
    let mut read_16bit_bs = false;

    // There are some pre-defined bit patterns. Some mean 'get from end of
    // header instead'.
    match bs_sr >> 4 {
        // The value 0000 is reserved.
        0b0000 => return Err(FlacError::InvalidFrameHeader),
        0b0001 => block_size = 192,
        n if 0b0010 <= n && n <= 0b0101 => block_size = 576 * (1 << (n - 2) as uint),
        0b0110 => read_8bit_bs = true,
        0b0111 => read_16bit_bs = true,
        n => block_size = 256 * (1 << (n - 8) as uint)
    }

    // For the sample rate there is a number of pre-defined bit patterns as
    // well. Again, some mean 'get from end of header instead'.
    let mut sample_rate = None;
    let mut read_8bit_sr = false;
    let mut read_16bit_sr = false;
    let mut read_16bit_sr_ten = false;

    match bs_sr & 0b0000_1111 {
        0b0000 => sample_rate = None, // 0000 means 'get from streaminfo block'.
        0b0001 => sample_rate = Some( 88_200),
        0b0010 => sample_rate = Some(176_400),
        0b0011 => sample_rate = Some(192_000),
        0b0100 => sample_rate = Some(  8_000),
        0b0101 => sample_rate = Some( 16_000),
        0b0110 => sample_rate = Some( 22_050),
        0b0111 => sample_rate = Some( 24_000),
        0b1000 => sample_rate = Some( 32_000),
        0b1001 => sample_rate = Some( 44_100),
        0b1010 => sample_rate = Some( 48_000),
        0b1011 => sample_rate = Some( 96_000),
        0b1100 => read_8bit_sr = true, // Read Hz from end of header.
        0b1101 => read_16bit_sr = true, // Read Hz from end of header.
        0b1110 => read_16bit_sr_ten = true, // Read tens of Hz from end of header.
        // 1111 is invalid to prevent sync-fooling.
        // Other values are impossible at this point.
        _ => return Err(FlacError::InvalidFrameHeader)
    }

    // Next are 4 bits channel assignment, 3 bits sample size, and 1 reserved bit.
    let chan_bps_res = try!(crc_input.read_byte());

    // The most significant 4 bits determine channel assignment.
    let (n_channels, channel_mode) = match chan_bps_res >> 4 {
        // Values 0 through 7 indicate n + 1 channels without mixing.
        n if n < 8 => (n + 1, ChannelMode::Raw),
        0b1000 => (2, ChannelMode::LeftSideStereo),
        0b1001 => (2, ChannelMode::RightSideStereo),
        0b1010 => (2, ChannelMode::MidSideStereo),
        // Values 1011 through 1111 are reserved and thus invalid.
        _ => return Err(FlacError::InvalidFrameHeader)
    };

    // The next three bits indicate bits per sample.
    let bits_per_sample = match (chan_bps_res & 0b0000_1110) >> 1 {
        0b000 => None, // 000 means 'get from streaminfo block'.
        0b001 => Some(8),
        0b010 => Some(12),
        0b100 => Some(16),
        0b101 => Some(20),
        0b110 => Some(24),
        // Values 011 and 111 are reserved. Other values are impossible.
        _ => return Err(FlacError::InvalidFrameHeader)
    };

    // The final bit has a mandatory value of 0.
    if chan_bps_res & 0b0000_0001 != 0 {
        return Err(FlacError::InvalidFrameHeader);
    }

    let block_time = match blocking_strategy {
        BlockingStrategy::Variable => {
            // The sample number is encoded in 8-56 bits, at most a 36-bit int.
            let sample = try!(read_var_length_int(&mut crc_input));
            BlockTime::SampleNumber(sample)
        },
        BlockingStrategy::Fixed => {
            // The frame number is encoded in 8-48 bits, at most a 31-bit int.
            let frame = try!(read_var_length_int(&mut crc_input));
            // A frame number larger than 31 bits is therefore invalid.
            if frame > 0x7fffffff {
                return Err(FlacError::InvalidFrameHeader);
            }
            BlockTime::FrameNumber(frame as u32)
        }
    };

    if read_8bit_bs {
        // 8 bit block size - 1 is stored.
        let bs = try!(crc_input.read_byte());
        block_size = bs as u16 + 1;
    }
    if read_16bit_bs {
        // 16-bit block size - 1 is stored. Note that the max block size that
        // can be indicated in the streaminfo block is a 16-bit number, so a
        // value of 0xffff would be invalid because it exceeds the max block
        // size, though this is not mentioned explicitly in the specification.
        let bs = try!(crc_input.read_be_u16());
        if bs == 0xffff { return Err(FlacError::InvalidBlockSize); }
        block_size = bs + 1;
    }

    if read_8bit_sr {
        let sr = try!(crc_input.read_byte());
        sample_rate = Some(sr as u32);
    }
    if read_16bit_sr {
        let sr = try!(crc_input.read_be_u16());
        sample_rate = Some(sr as u32);
    }
    if read_16bit_sr_ten {
        let sr_ten = try!(crc_input.read_be_u16());
        sample_rate = Some(sr_ten as u32 * 10);
    }

    // Next is an 8-bit CRC that is computed over the entire header so far.
    let computed_crc = crc_input.crc();
    let presumed_crc = try!(crc_input.read_byte());

    if computed_crc != presumed_crc {
        return Err(FlacError::FrameHeaderCrcMismatch);
    }

    let frame_header = FrameHeader {
       block_time: block_time,
       block_size: block_size,
       sample_rate: sample_rate,
       n_channels: n_channels,
       channel_mode: channel_mode,
       bits_per_sample: bits_per_sample
    };
    Ok(frame_header)
}

pub struct Block<'b, Sample> where Sample: 'b {
    /// The sample number of the first sample in the this block.
    first_sample_number: u64,
    /// The number of samples in the block.
    block_size: u16,
    /// The number of channels in the block.
    n_channels: u8,
    /// The decoded samples, the channels stored consecutively.
    samples: &'b [Sample]
}

impl <'b, Sample> Block<'b, Sample> where Sample: UnsignedInt {
    fn new(time: u64, bs: u16, buffer: &'b [Sample]) -> Block<'b, Sample> {
        Block {
            first_sample_number: time,
            block_size: bs,
            n_channels: (buffer.len() / bs as uint) as u8,
            samples: buffer
        }
    }

    /// Returns the sample number of the first sample in the block.
    pub fn time(&self) -> u64 {
        self.first_sample_number
    }

    /// Returns the number of inter-channel samples in the block.
    pub fn len(&self) -> u16 {
        self.block_size
    }

    /// Returns the number of channels in the block.
    pub fn channels(&self) -> u8 {
        self.n_channels
    }

    /// Returns the (zero-based) `ch`-th channel as a slice.
    ///
    /// # Panics
    /// Panics if `ch` is larger than `channels()`.
    pub fn channel(&'b self, ch: u8) -> &'b [Sample] {
        self.samples[ch as uint * self.block_size as uint
                     .. (ch as uint + 1) * self.block_size as uint]
    }
}

/// Reads frames from a stream and exposes them as an iterator.
///
/// TODO: for now, it is assumes that the reader starts at a frame header;
/// no searching for a sync code is performed at the moment.
pub struct FrameReader<'r, Sample> {
    input: &'r mut (Reader + 'r),
    buffer: Vec<Sample>
}

/// Either a `Block` or a `FlacError`.
pub type FrameResult<'b, Sample> = FlacResult<Block<'b, Sample>>;

impl<'r, Sample> FrameReader<'r, Sample> where Sample: UnsignedInt {

    /// Creates a new frame reader that will yield at least one element.
    pub fn new(input: &'r mut Reader) -> FrameReader<'r, Sample> {
        // TODO: a hit for the vector size can be provided.
        FrameReader { input: input, buffer: Vec::new() }
    }

    fn read_next<'s>(&'s mut self) -> FrameResult<'s, Sample> {
        let header = try!(read_frame_header(self.input));

        // TODO: remove this print.
        println!("Frame: bs = {}, sr = {}, n_ch = {}, cm = {}, bps = {}",
                 header.block_size,
                 header.sample_rate,
                 header.n_channels,
                 header.channel_mode,
                 header.bits_per_sample);

        // TODO: verify that `Sample` is wide enough, fail otherwise.

        // We must allocate enough space for all channels in the block to be
        // decoded.
        let total_samples = header.n_channels as uint * header.block_size as uint;
        if self.buffer.len() < total_samples {
            // Previous data will be overwritten, so instead of resizing the
            // vector if it is too small, we might as well allocate a new one.
            if self.buffer.capacity() < total_samples {
                self.buffer = Vec::with_capacity(total_samples);
            }
            let len = self.buffer.len();
            self.buffer.grow(total_samples - len, Int::zero());
        }

        // TODO: if the bps is missing from the header, we must get it from
        // the streaminfo block.
        let bps = header.bits_per_sample.unwrap();

        // In the next part of the stream, nothing is byte-aligned any more,
        // we need a bitstream. Then we can decode subframes from the bitstream.
        {
            let mut bitstream = Bitstream::new(self.input);
            let mut decoder = SubframeDecoder::new(bps, &mut bitstream);

            for ch in range(0, header.n_channels) {
                try!(decoder.decode(self.buffer[mut
                     ch as uint * header.block_size as uint
                     .. (ch as uint + 1) * header.block_size as uint]));
            }

            // When the bitstream goes out of scope, we can use the `input`
            // reader again, which will be byte-aligned. The specification
            // dictates that padding should consist of zero bits, but we do not
            // enforce this here.
        }

        // TODO: constant block size should be verified if a frame number is
        // encountered.
        let time = match header.block_time {
            BlockTime::FrameNumber(fnr) => header.block_size as u64 * fnr as u64,
            BlockTime::SampleNumber(snr) => snr
        };

        let block = Block::new(time, header.block_size,
                               self.buffer[0 .. total_samples]);

        // TODO: read frame footer

        Ok(block)
    }
}

impl<'r, 'b, Sample> Iterator<FrameResult<'b, Sample>>
    for FrameReader<'r, Sample>
    where Sample: UnsignedInt {
    fn next(&mut self) -> Option<FrameResult<'b, Sample>> {
        // TODO: there needs to be a way to determine whether stream has ended.
        // In that case, we need to know the stream lengh, so we need to know
        // the streaminfo (which we might need anyway) ...
        Some(self.read_next())
    }

    // TODO: it would be possible to give quite an accurate size hint.
}
