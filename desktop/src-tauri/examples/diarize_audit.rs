//! Read-only native smoke check. stdout is metadata/turns, never transcript text.
#[path = "../src/speaker_attribution.rs"]
mod speaker_attribution;
fn main() -> anyhow::Result<()> {
    // Exercise the real JSON codec, export and AI context builders.
    let legacy: solflow_lib::meetings::Segment = serde_json::from_str(r#"{"s":0,"e":1,"text":"Legacy"}"#)?;
    assert!(!legacy.spk_review && legacy.voices.is_empty());
    let review: solflow_lib::meetings::Segment = serde_json::from_str(r#"{"s":1,"e":3,"text":"Uncertain","spk_review":true,"voices":[0,1]}"#)?;
    let encoded = serde_json::to_value(&review)?;
    assert_eq!(encoded["spk_review"], true);
    assert_eq!(encoded["voices"], serde_json::json!([0, 1]));
    let known: solflow_lib::meetings::Segment = serde_json::from_str(r#"{"s":0,"e":1,"text":"Known","spk":0}"#)?;
    let meta = solflow_lib::meetings::Meta { speakers: 2, ..Default::default() };
    let sample = vec![known, review];
    let prompt = solflow_lib::meetings::timed_text(&meta, &sample);
    assert!(prompt.contains("\n[0:01] [Несколько голосов"));
    let exported = solflow_lib::meetings::as_text("", "", "", &sample, &meta.names);
    assert!(exported.contains("Несколько голосов — проверьте"));
    let args: Vec<_> = std::env::args().skip(1).collect();
    anyhow::ensure!(args.len() >= 4, "audio.wav segmentation.onnx embedding.onnx transcript.json [speakers]");
    let segments: Vec<solflow_lib::meetings::Segment> = serde_json::from_slice(&std::fs::read(&args[3])?)?;
    let started = std::time::Instant::now();
    let turns = solflow_lib::diarize_file(args[0].as_ref(), args[1].as_ref(), args[2].as_ref(),
        args.get(4).and_then(|s| s.parse().ok()).unwrap_or(0))?;
    let bounds: Vec<_> = segments.iter().map(|s| (s.s, s.e)).collect();
    let (labels, count) = speaker_attribution::assign(&bounds, &turns);
    println!("{}", serde_json::to_string_pretty(&serde_json::json!({
        "speakers": count, "segments": segments.len(), "turns": turns,
        "mixed": labels.iter().filter(|s| s.voices.len() > 1).count(),
        "unassigned": labels.iter().filter(|s| s.speaker.is_none() && s.voices.is_empty()).count(),
        "elapsed_seconds": started.elapsed().as_secs_f32(),
    }))?);
    Ok(())
}
