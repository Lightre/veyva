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
#[cfg(test)]
mod tests {
    use super::*;
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
