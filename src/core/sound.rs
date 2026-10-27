//! 启动器提示音
//!
//! 声音是自己合成的两段短音：点启动时一声，游戏起来后再一声，
//! 用系统自带的播放接口放，不引第三方音频库，也不带任何音频文件。
//!
//! 播放走的是 WinMM 的 `PlaySound`，从内存里读一段 WAV：
//! 缓冲区一直留着（不回收），播放期间指针始终有效。

use std::sync::OnceLock;

/// 采样率，22 kHz 足够放提示音，体积也小
const RATE: u32 = 22050;

/// 合成一段单声道 16 位的 WAV
///
/// 每段音给一个频率（Hz）与时长（秒），音与音之间自动留一小段空隙。
/// 波形带淡入淡出，听着不刺耳。
fn synth(notes: &[(f32, f32)]) -> Vec<u8> {
    let mut samples: Vec<i16> = Vec::new();
    for (freq, secs) in notes {
        let count = (RATE as f32 * secs) as usize;
        for i in 0..count {
            let t = i as f32 / RATE as f32;
            // 起音 20 毫秒渐入，尾音 60 毫秒渐出
            let env = (t / 0.02).min(1.0) * ((secs - t) / 0.06).min(1.0);
            let wave = (2.0 * std::f32::consts::PI * freq * t).sin();
            let value = wave * env * 0.25;
            samples.push((value * i16::MAX as f32) as i16);
        }
        // 音符之间留 40 毫秒空隙
        samples.extend(std::iter::repeat(0).take((RATE as f32 * 0.04) as usize));
    }

    let data_len = (samples.len() * 2) as u32;
    let mut out: Vec<u8> = Vec::with_capacity(44 + data_len as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVE");
    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // 单声道
    out.extend_from_slice(&RATE.to_le_bytes());
    out.extend_from_slice(&(RATE * 2).to_le_bytes()); // 每秒字节数
    out.extend_from_slice(&2u16.to_le_bytes()); // 块对齐
    out.extend_from_slice(&16u16.to_le_bytes()); // 位深
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for sample in samples {
        out.extend_from_slice(&sample.to_le_bytes());
    }
    out
}

/// 播放一段合成好的声音
///
/// 缓存里的内存不回收：播放接口是异步的，
/// 播放期间那段内存必须一直在。
fn play(cache: &OnceLock<&'static [u8]>, notes: &[(f32, f32)]) {
    #[cfg(windows)]
    {
        use windows_sys::Win32::Media::Audio::{PlaySoundW, SND_ASYNC, SND_MEMORY, SND_NODEFAULT};
        let bytes = cache.get_or_init(|| -> &'static [u8] { Box::leak(synth(notes).into_boxed_slice()) });
        // SAFETY: 指针指向的是泄漏出来的静态内存，播放期间一直有效
        unsafe {
            PlaySoundW(
                bytes.as_ptr() as *const u16,
                std::ptr::null_mut(),
                SND_MEMORY | SND_ASYNC | SND_NODEFAULT,
            );
        }
    }
    #[cfg(not(windows))]
    {
        let _ = (cache, notes);
    }
}

static START_SOUND: OnceLock<&'static [u8]> = OnceLock::new();
static DONE_SOUND: OnceLock<&'static [u8]> = OnceLock::new();

/// 点下启动时的那一声
pub fn play_start() {
    play(&START_SOUND, &[(587.33, 0.08), (880.0, 0.12)]);
}

/// 游戏真的起来之后的那一声
pub fn play_done() {
    play(&DONE_SOUND, &[(880.0, 0.08), (1174.66, 0.15)]);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 合出来的声音要是一段能读的 WAV
    #[test]
    fn synth_makes_a_wav() {
        let bytes = synth(&[(440.0, 0.1)]);
        assert_eq!(&bytes[0..4], b"RIFF");
        assert_eq!(&bytes[8..12], b"WAVE");
        assert_eq!(&bytes[12..16], b"fmt ");
        assert_eq!(&bytes[36..40], b"data");
        // 头 44 字节 + 一秒不到的数据
        assert!(bytes.len() > 44);
    }

    /// 两段音之间要有空隙，总长度比一段长
    #[test]
    fn longer_notes_make_longer_sound() {
        let one = synth(&[(440.0, 0.1)]);
        let two = synth(&[(440.0, 0.1), (660.0, 0.1)]);
        assert!(two.len() > one.len());
    }
}
