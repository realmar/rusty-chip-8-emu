use std::io::{Cursor, Read, Seek, SeekFrom};
use std::vec::Vec;

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

pub fn sample(hz: f64) -> Result<Vec<u8>, String> {
    // 48hz sampling rate
    let sampling_rate = 48000.0;

    let sound = Sound::new(None, hz);
    let wave = sound
        .take(sampling_rate as usize * 10)
        .map(|x| x.sin().into())
        .collect::<Vec<i16>>();

    let mut writer = Cursor::new(Vec::<u8>::new());

    if let Err(..) = wav::write(
        wav::Header::new(1, 1, sampling_rate as u32, 16),
        wav::BitDepth::Sixteen(wave),
        &mut writer,
    ) {
        Err(String::from("Failed to write waveform"))
    } else {
        let mut out = Vec::new();
        if let Err(..) = writer.seek(SeekFrom::Start(0)) {
            Err(String::from("Failed to seek waveform"))
        } else {
            if let Err(..) = writer.read_to_end(&mut out) {
                Err(String::from("Failed to convert waveform to wav"))
            } else {
                Ok(out)
            }
        }
    }
}
