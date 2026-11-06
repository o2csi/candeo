//! The frames both outputs render: fixed times and inputs, so that two runs of
//! the same effect must give the same bytes.

use crate::engine::FrameInputs;

/// The parameters an effect gets when nobody changed them: each `default` its
/// manifest declares.
pub fn default_params(manifest: &str) -> String {
    let value: serde_json::Value = serde_json::from_str(manifest).unwrap_or_default();
    let mut params = serde_json::Map::new();
    if let Some(specs) = value.get("params").and_then(|p| p.as_object()) {
        for (name, spec) in specs {
            if let Some(default) = spec.get("default") {
                params.insert(name.clone(), default.clone());
            }
        }
    }
    serde_json::Value::Object(params).to_string()
}

/// A few frames with no input, as a swatch samples them.
pub fn at_rest(params: &str) -> Vec<FrameInputs> {
    [0.0, 0.25, 1.0, 2.5, 7.5]
        .iter()
        .enumerate()
        .map(|(i, &time)| FrameInputs {
            time,
            frame_index: i as u32,
            params: params.to_owned(),
            ..FrameInputs::default()
        })
        .collect()
}

/// Three seconds at 30 frames per second, rendered in order in one context, with
/// every input an effect may read: presses, a clock, signals, sound, system.
/// Each effect reads only those it declares; the bootstrap gives the others
/// their neutral value.
pub fn active(params: &str) -> Vec<FrameInputs> {
    const PRESSES: [(u32, f64); 5] = [(3, 0.2), (17, 0.7), (40, 1.1), (60, 1.9), (5, 2.4)];
    // 2026-11-05 12:34:56.789 UTC: any fixed instant.
    const CLOCK_MS: f64 = 1_793_882_096_789.0;
    (0..90u32)
        .map(|i| {
            let time = f64::from(i) / 30.0;
            let presses: Vec<String> = PRESSES
                .iter()
                .filter(|(_, at)| *at <= time)
                .map(|(k, at)| format!(r#"{{"k":{k},"at":{at}}}"#))
                .collect();
            let beat = i % 15 == 0;
            let bands: Vec<String> = (0..16)
                .map(|b| {
                    let v = 0.5 + 0.5 * (time * 3.0 + f64::from(b) * 0.7).sin();
                    format!("{:.4}", v * if b < 4 && beat { 1.0 } else { 0.7 })
                })
                .collect();
            let level = 0.4 + 0.3 * (time * 2.0).sin();
            FrameInputs {
                time,
                frame_index: i,
                params: params.to_owned(),
                presses: format!("[{}]", presses.join(",")),
                clock_ms: CLOCK_MS + time * 1000.0,
                signals: format!(
                    concat!(
                        r#"{{"cs2.health":{health},"cs2.ammo":40,"cs2.flashed":{flash},"#,
                        r#""cs2.burning":0,"cs2.bomb":"planted","#,
                        r#""dota2.health":55,"dota2.mana":80,"dota2.respawn":"","#,
                        r#""dota2.stunned":"yes","dota2.smoked":"no","dota2.daytime":"night","#,
                        r#""build":"failed","deploy":"running","tests":"passed","load":0.7}}"#
                    ),
                    health = 100 - i,
                    flash = if (30..45).contains(&i) { 80 } else { 0 },
                ),
                audio: format!(
                    r#"{{"level":{level:.4},"peak":{:.4},"bands":[{}],"beat":{beat},"onset":{}}}"#,
                    level + 0.1,
                    bands.join(","),
                    if beat { 0.8 } else { 0.1 },
                ),
                system: format!(
                    concat!(
                        r#"{{"cpu":{cpu:.4},"memory":0.63,"gpu":0.2,"cpuTemp":0.5,"gpuTemp":0.4,"#,
                        r#""cpuPower":0.3,"cpuCelsius":67.5,"gpuCelsius":52,"cpuWatts":45}}"#
                    ),
                    cpu = 0.5 + 0.4 * (time).sin(),
                ),
            }
        })
        .collect()
}
