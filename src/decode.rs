use error::{FlacError, FlacResult};

#[deriving(Copy)]
enum BlockingStrategy {
    Fixed,
    Variable
}

#[deriving(Copy)]
enum ChannelMode {
    /// The channels are coded as-is.
    Raw,
    /// Channel 0 is the left channel, channel 1 is the side channel.
    LeftSideStereo,
    /// Channel 0 is the side channel, channel 1 is the right channel.
    RightSideStero,
    /// Channel 0 is the mid channel, channel 1 is the side channel.
    MidSideStereo
}

#[deriving(Copy)]
struct FrameHeader {
    pub blocking_strategy: BlockingStrategy,
    pub block_size: u16,
    pub sample_rate: Option<u32>,
    pub n_channels: u8,
    pub channel_mode: ChannelMode,
    pub bits_per_sample: Option<u8>
}
