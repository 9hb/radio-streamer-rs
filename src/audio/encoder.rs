use std::ffi::c_int;

pub struct Mp3Encoder {
    lame: *mut mp3lame_sys::lame_global_flags,
}

// LAME flags pointer can be safely sent across threads when wrapped
unsafe impl Send for Mp3Encoder {}

impl Mp3Encoder {
    pub fn new(in_samplerate: u32, in_channels: u16, bitrate_kbps: u64) -> Result<Self, String> {
        unsafe {
            let lame = mp3lame_sys::lame_init();
            if lame.is_null() {
                return Err("Failed to initialize LAME encoder context".to_string());
            }

            mp3lame_sys::lame_set_in_samplerate(lame, in_samplerate as c_int);
            mp3lame_sys::lame_set_num_channels(lame, in_channels as c_int);
            mp3lame_sys::lame_set_out_samplerate(lame, 44100);
            mp3lame_sys::lame_set_brate(lame, bitrate_kbps as c_int);
            mp3lame_sys::lame_set_quality(lame, 2); // 2 = high quality

            // Set CBR (Constant Bit Rate)
            mp3lame_sys::lame_set_VBR(lame, mp3lame_sys::vbr_mode::vbr_off);

            let res = mp3lame_sys::lame_init_params(lame);
            if res < 0 {
                mp3lame_sys::lame_close(lame);
                return Err(format!("lame_init_params returned error code {}", res));
            }

            Ok(Self { lame })
        }
    }

    pub fn encode_interleaved_pcm(&mut self, pcm: &[i16]) -> Vec<u8> {
        let num_samples = (pcm.len() / 2) as c_int;
        if num_samples == 0 {
            return Vec::new();
        }

        // Worst-case MP3 buffer size according to LAME docs: 1.25 * num_samples + 7200
        let buf_size = (1.25 * num_samples as f64) as usize + 7200;
        let mut mp3_buf = vec![0u8; buf_size];

        unsafe {
            let bytes_written = mp3lame_sys::lame_encode_buffer_interleaved(
                self.lame,
                pcm.as_ptr() as *mut i16,
                num_samples,
                mp3_buf.as_mut_ptr(),
                buf_size as c_int,
            );

            if bytes_written > 0 {
                mp3_buf.truncate(bytes_written as usize);
                mp3_buf
            } else {
                Vec::new()
            }
        }
    }

    pub fn flush(&mut self) -> Vec<u8> {
        let buf_size = 7200;
        let mut mp3_buf = vec![0u8; buf_size];

        unsafe {
            let bytes_written =
                mp3lame_sys::lame_encode_flush(self.lame, mp3_buf.as_mut_ptr(), buf_size as c_int);

            if bytes_written > 0 {
                mp3_buf.truncate(bytes_written as usize);
                mp3_buf
            } else {
                Vec::new()
            }
        }
    }
}

impl Drop for Mp3Encoder {
    fn drop(&mut self) {
        unsafe {
            if !self.lame.is_null() {
                mp3lame_sys::lame_close(self.lame);
                self.lame = std::ptr::null_mut();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mp3_encoder_init_and_encode() {
        let mut encoder = Mp3Encoder::new(44100, 2, 320).expect("Encoder init should succeed");
        // Generate 1 second of stereo silence (44100 samples * 2 channels)
        let silence = vec![0i16; 44100 * 2];
        let bytes = encoder.encode_interleaved_pcm(&silence);
        assert!(!bytes.is_empty(), "Encoded MP3 bytes should not be empty");

        let flushed = encoder.flush();
        assert!(!flushed.is_empty(), "Flush should return remaining frames");
    }
}
