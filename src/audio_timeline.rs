//! Stable sample-position alignment. QPC anchors the stream, not every packet.
#[derive(Default)]
pub struct PacketClock {
    anchor: Option<(u64, i64)>,
}
impl PacketClock {
    pub fn target(
        &mut self,
        device_position: u64,
        qpc_frame: i64,
        timestamp_valid: bool,
        fallback: i64,
    ) -> i64 {
        if !timestamp_valid {
            return fallback;
        }
        let predicted = self.anchor.and_then(|(device, frame)| {
            device_position
                .checked_sub(device)
                .and_then(|delta| i64::try_from(delta).ok())
                .map(|delta| frame.saturating_add(delta))
        });
        // Endpoint restarts and genuine silent intervals need a new wall-clock
        // anchor. Normal packet timestamp jitter must not edit PCM samples.
        if let Some(predicted) = predicted {
            if predicted.abs_diff(qpc_frame) <= 2400 {
                return predicted;
            }
        }
        self.anchor = Some((device_position, qpc_frame));
        qpc_frame
    }
}
/// Synthesize idle-render silence only after a real quiet interval, never
/// while captured packets are still waiting to be consumed.
pub fn should_fill_silence(backlogged: bool, since_packet_ms: u128) -> bool {
    !backlogged && since_packet_ms >= 150
}
/// Mix samples aligned to the same PCM frame, clipping at PCM16 limits.
pub fn mix_pcm16(samples: impl Iterator<Item = i16>) -> i16 {
    samples
        .map(i32::from)
        .sum::<i32>()
        .clamp(i16::MIN as i32, i16::MAX as i32) as i16
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn aligned_mix_preserves_single_source_and_clips_without_wrapping() {
        assert_eq!(mix_pcm16([1234].into_iter()), 1234);
        assert_eq!(mix_pcm16([1234, 0].into_iter()), 1234);
        assert_eq!(mix_pcm16([20000, 20000].into_iter()), i16::MAX);
        assert_eq!(mix_pcm16([-20000, -20000].into_iter()), i16::MIN);
        assert_eq!(mix_pcm16([-9000, 10000].into_iter()), 1000);
    }
    #[test]
    fn independent_endpoint_clocks_align_to_common_origin() {
        let mut output = PacketClock::default();
        let mut microphone = PacketClock::default();
        assert_eq!(output.target(10000, 480, true, 0), 480);
        assert_eq!(microphone.target(90000, 960, true, 0), 960);
        assert_eq!(output.target(10480, 959, true, 0), 960);
        assert_eq!(microphone.target(90480, 1441, true, 0), 1440);
    }
    #[test]
    fn delayed_real_packets_are_never_replaced_with_silence() {
        assert!(!should_fill_silence(true, 500));
        assert!(!should_fill_silence(false, 149));
        assert!(should_fill_silence(false, 150));
    }
    #[test]
    fn timestamp_rounding_does_not_insert_or_remove_samples() {
        let mut clock = PacketClock::default();
        assert_eq!(clock.target(0, 1000, true, 0), 1000);
        for n in 1..1000u64 {
            let expected = 1000 + n as i64 * 480;
            let jitter = if n % 2 == 0 { 1 } else { -1 };
            assert_eq!(clock.target(n * 480, expected + jitter, true, 0), expected);
        }
    }
    #[test]
    fn missing_device_frames_preserve_real_gap() {
        let mut c = PacketClock::default();
        c.target(0, 0, true, 0);
        assert_eq!(c.target(960, 961, true, 480), 960);
    }
    #[test]
    fn resume_after_silence_reanchors_once() {
        let mut c = PacketClock::default();
        c.target(0, 0, true, 0);
        assert_eq!(c.target(480, 48000, true, 48000), 48000);
        assert_eq!(c.target(960, 48481, true, 48480), 48480);
    }
    #[test]
    fn reset_and_invalid_timestamps_are_bounded() {
        let mut c = PacketClock::default();
        c.target(9600, 10000, true, 0);
        assert_eq!(c.target(0, 20000, true, 0), 20000);
        assert_eq!(c.target(480, 0, false, 20480), 20480);
    }
}

/// Immediate peak attack and time-based release, independent of polling rate.
pub fn meter_envelope(previous: f32, peak: u16, elapsed_seconds: f32) -> f32 {
    let incoming = (f32::from(peak) / 32768.0).min(1.0);
    let released = previous * (-elapsed_seconds.max(0.0) / 0.35).exp();
    let level = incoming.max(released).clamp(0.0, 1.0);
    if level < 0.001 {
        0.0
    } else {
        level
    }
}

#[cfg(test)]
mod meter_tests {
    use super::meter_envelope;
    #[test]
    fn full_scale_silence_and_signed_minimum_are_safe() {
        assert_eq!(meter_envelope(0.0, i16::MIN.unsigned_abs(), 0.005), 1.0);
        assert_eq!(meter_envelope(0.0, 0, 0.005), 0.0);
        assert_eq!(meter_envelope(1.0, 0, 5.0), 0.0);
        assert!(meter_envelope(0.0, 16384, 0.005) >= 0.5);
    }
    #[test]
    fn release_is_independent_of_poll_frequency() {
        let single = meter_envelope(1.0, 0, 0.1);
        let mut frequent = 1.0;
        for _ in 0..20 {
            frequent = meter_envelope(frequent, 0, 0.005);
        }
        assert!((single - frequent).abs() < 0.00001);
        assert!(meter_envelope(0.1, 32768, 0.005) > 0.99);
    }
}
