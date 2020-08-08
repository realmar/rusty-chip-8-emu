use std::io::{Cursor, Read, Seek, SeekFrom};
use std::vec::Vec;

use anyhow::Result;
use twang::Sound;
use wav;

pub struct Audio {
    pub(super) playing: bool,
}

impl Audio {
    pub fn new() -> Audio {
        Audio { playing: false }
    }

    pub fn is_playing(&self) -> bool {
        self.playing
    }
}

pub fn sample(hz: f64) -> Result<Vec<u8>> {
    // 48hz sampling rate
    let sampling_rate = 48000.0;

    let sound = Sound::new(None, hz);
    let wave = sound
        .take(sampling_rate as usize * 10)
        .map(|x| x.sin().into())
        .collect::<Vec<i16>>();

    let mut writer = Cursor::new(Vec::<u8>::new());

    wav::write(
        wav::Header::new(1, 1, sampling_rate as u32, 16),
        wav::BitDepth::Sixteen(wave),
        &mut writer,
    )?;

    let mut out = Vec::new();
    writer.seek(SeekFrom::Start(0))?;
    writer.read_to_end(&mut out)?;

    Ok(out)
}
