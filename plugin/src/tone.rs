pub fn chunk(index: usize) -> Vec<i16> {
    (0..crate::hold::CHUNK)
        .map(|i| {
            let t = (index * crate::hold::CHUNK + i) as f64 / 16_000.0;
            (8000.0 * (2.0 * std::f64::consts::PI * (300.0 + 200.0 * t) * t).sin()) as i16
        })
        .collect()
}
