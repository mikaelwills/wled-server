use crate::config::PatternType;
use rand::seq::SliceRandom;

#[derive(Debug, Clone)]
pub struct PatternStep {
    pub board_ids: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct PatternSequence {
    pub steps: Vec<PatternStep>,
    pub total_duration_ms: u64,
}

pub fn transform_board_order(members: &[String], pattern: &PatternType) -> Vec<Vec<String>> {
    let n = members.len();
    match pattern {
        PatternType::Wave => members.iter().map(|b| vec![b.clone()]).collect(),
        PatternType::WaveReverse => members.iter().rev().map(|b| vec![b.clone()]).collect(),
        PatternType::Alternate => {
            let odds: Vec<String> = members.iter().step_by(2).cloned().collect();
            let evens: Vec<String> = members.iter().skip(1).step_by(2).cloned().collect();
            vec![odds, evens]
        }
        PatternType::OutsideIn => {
            let mut steps = Vec::new();
            for i in 0..((n + 1) / 2) {
                let mut step = vec![members[i].clone()];
                if i != n - 1 - i {
                    step.push(members[n - 1 - i].clone());
                }
                steps.push(step);
            }
            steps
        }
        PatternType::CenterOut => {
            let mut steps = transform_board_order(members, &PatternType::OutsideIn);
            steps.reverse();
            steps
        }
        PatternType::Random => {
            let mut shuffled: Vec<_> = members.iter().map(|b| vec![b.clone()]).collect();
            shuffled.shuffle(&mut rand::rng());
            shuffled
        }
    }
}

pub fn generate_sequence(
    members: &[String],
    pattern: &PatternType,
    bpm: f64,
    sync_rate: f64,
) -> PatternSequence {
    let board_groups = transform_board_order(members, pattern);
    let beat_duration_ms = 60_000.0 / bpm;
    let total_duration_ms = (beat_duration_ms / sync_rate) as u64;

    let steps = board_groups.into_iter().map(|board_ids| PatternStep {
        board_ids,
    })
    .collect();

    PatternSequence { steps, total_duration_ms }
}
