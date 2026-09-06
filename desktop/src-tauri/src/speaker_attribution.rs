//! Assign acoustic turns to text without inventing word timestamps.
//! Keep IDs in acoustic order, including voices inside mixed text segments.
#[derive(Debug, PartialEq)]
pub struct Attribution {
    pub speaker: Option<u32>,
    pub voices: Vec<u32>,
}

pub fn assign(bounds: &[(f32, f32)], turns: &[(f32, f32, usize)]) -> (Vec<Attribution>, usize) {
    let mut turns: Vec<_> = turns.iter().copied()
        .filter(|(s, e, _)| s.is_finite() && e.is_finite() && *s >= 0.0 && e > s).collect();
    turns.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.2.cmp(&b.2)));
    let mut order = Vec::new();
    for &(_, _, speaker) in &turns {
        if !order.contains(&speaker) { order.push(speaker); }
    }
    let result = bounds.iter().map(|&(start, end)| {
        let mut spans = vec![0.0_f32; order.len()];
        if start.is_finite() && end.is_finite() && end > start {
            for &(s, e, speaker) in &turns {
                let overlap = end.min(e) - start.max(s);
                if overlap > 0.0 {
                    spans[order.iter().position(|x| *x == speaker).unwrap()] += overlap;
                }
            }
        }
        let total: f32 = spans.iter().sum();
        let mut ranked: Vec<_> = spans.iter().enumerate().filter(|(_, span)| **span > 0.0).collect();
        ranked.sort_by(|a, b| b.1.total_cmp(a.1).then(a.0.cmp(&b.0)));
        // Ignore a tiny boundary spill; this is a review heuristic, not
        // model confidence or a claim about precise word boundaries.
        let mixed = ranked.iter().skip(1).any(|(_, span)| **span >= 0.3 && **span >= total * 0.1);
        let voices = if mixed { ranked.iter().map(|(i, _)| *i as u32).collect() } else { Vec::new() };
        Attribution { speaker: if mixed { None } else { ranked.first().map(|(i, _)| *i as u32) }, voices }
    }).collect();
    (result, order.len())
}
