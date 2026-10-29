mod settings;

fn main() {
    let profile = settings::CaptureProfile::low_end();
    println!("Veyra Replay | development scaffold");
    println!("Capture backend: not implemented");
    println!(
        "Low End: {}x{} at {} FPS, {} kbps video, {} seconds history",
        profile.width, profile.height, profile.fps, profile.video_kbps, profile.history_seconds
    );
    println!(
        "Encoded payload estimate: {:.1} MiB (not total process memory)",
        profile.payload_mib()
    );
    println!("Replay payload budget: {} MiB", profile.replay_budget_mib);
    println!("No capture session has been started.");
}
