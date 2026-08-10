//! Character dictionary and greedy CTC decoding for the recognition model.
//!
//! The label layout matches PaddleOCR's `CTCLabelDecode`:
//! `labels = ["blank"] + <dictionary lines> + [" "]`.

use crate::error::VisionError;
use std::fs;
use std::path::Path;

/// The character dictionary used by the recognition model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CharDict {
    /// `labels[0]` is the CTC blank label; the rest are real characters.
    pub labels: Vec<String>,
}

impl CharDict {
    /// Number of classes the recognition model outputs.
    pub fn num_classes(&self) -> usize {
        self.labels.len()
    }

    /// Load from a PaddleOCR `ppocr_keys_v1.txt`-style file: one character
    /// per line. An empty line is treated as a space character.
    pub fn load(path: &Path) -> Result<Self, VisionError> {
        let text = fs::read_to_string(path).map_err(|err| {
            VisionError::Io(std::io::Error::new(
                err.kind(),
                format!("failed to read dictionary {}: {err}", path.display()),
            ))
        })?;
        Ok(Self::from_lines(&text))
    }

    /// Build from the dictionary file text.
    pub fn from_lines(text: &str) -> Self {
        let mut labels = vec!["blank".to_string()];
        for line in text.split('\n') {
            let line = line.strip_suffix('\r').unwrap_or(line);
            if line.is_empty() {
                // A trailing newline produces an empty final entry; only a
                // literal blank line (two consecutive newlines) is a space.
                continue;
            }
            labels.push(line.to_string());
        }
        labels.push(" ".to_string());
        Self { labels }
    }
}

/// Greedy CTC decode of one recognition-model output.
///
/// `logits` holds `time_steps * num_classes` values in row-major order. The
/// model's class index `0` is the CTC blank.
///
/// Returns `(text, mean_confidence)`. The confidence is the mean of the
/// argmax probabilities over the emitted characters, and is `0.0` when no
/// character was emitted.
pub fn decode_ctc(
    logits: &[f32],
    time_steps: usize,
    num_classes: usize,
    dict: &CharDict,
) -> (String, f32) {
    assert_eq!(
        logits.len(),
        time_steps * num_classes,
        "logits length mismatch"
    );
    let mut out = String::new();
    let mut probs: Vec<f32> = Vec::new();
    let mut prev_idx: i64 = -1;
    for t in 0..time_steps {
        let base = t * num_classes;
        let (mut best_idx, mut best_prob) = (0usize, logits[base]);
        for c in 1..num_classes {
            let p = logits[base + c];
            if p > best_prob {
                best_idx = c;
                best_prob = p;
            }
        }
        if best_idx == 0 {
            prev_idx = 0;
            continue;
        }
        if best_idx as i64 == prev_idx {
            continue;
        }
        prev_idx = best_idx as i64;
        if let Some(ch) = dict.labels.get(best_idx) {
            out.push_str(ch);
            probs.push(best_prob);
        }
    }
    let score = if probs.is_empty() {
        0.0
    } else {
        probs.iter().sum::<f32>() / probs.len() as f32
    };
    (out, score)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tiny dictionary: blank + "ab c".
    fn tiny_dict() -> CharDict {
        CharDict {
            labels: vec![
                "blank".into(),
                "a".into(),
                "b".into(),
                " ".into(),
                "c".into(),
            ],
        }
    }

    #[test]
    fn labels_match_paddleocr_layout() {
        let dict = CharDict::from_lines("'\nab\ncd\n");
        // blank, ', ab, cd, then a trailing space.
        assert_eq!(dict.labels, vec!["blank", "'", "ab", "cd", " "]);
        assert_eq!(dict.num_classes(), 5);
    }

    #[test]
    fn decode_simple_word() {
        let dict = tiny_dict();
        // 4 time steps for "ab c": a, b, (blank), c
        let logits = vec![
            0.1, 0.9, 0.0, 0.0, 0.0, // a
            0.1, 0.0, 0.9, 0.0, 0.0, // b
            0.9, 0.0, 0.0, 0.0, 0.1, // blank
            0.1, 0.0, 0.0, 0.0, 0.9, // c
        ];
        let (text, score) = decode_ctc(&logits, 4, 5, &dict);
        assert_eq!(text, "abc");
        assert!((score - 0.9).abs() < 1e-6);
    }

    #[test]
    fn decode_collapses_repeats() {
        let dict = tiny_dict();
        // "a a" -> after collapsing "aa" is not collapsed because there is a
        // blank between them. Here: a, a (no blank) -> collapse to "a".
        let logits = vec![
            0.1, 0.9, 0.0, 0.0, 0.0, // a
            0.1, 0.9, 0.0, 0.0, 0.0, // a (repeat, collapsed)
            0.9, 0.0, 0.0, 0.0, 0.1, // blank
        ];
        let (text, _) = decode_ctc(&logits, 3, 5, &dict);
        assert_eq!(text, "a");
    }

    #[test]
    fn decode_empty_returns_zero_score() {
        let dict = tiny_dict();
        let logits = vec![0.9, 0.0, 0.0, 0.0, 0.1, 0.9, 0.0, 0.0, 0.0, 0.1];
        let (text, score) = decode_ctc(&logits, 2, 5, &dict);
        assert_eq!(text, "");
        assert_eq!(score, 0.0);
    }

    #[test]
    fn decode_keeps_spaces() {
        let dict = tiny_dict();
        // a (space) b
        let logits = vec![
            0.1, 0.9, 0.0, 0.0, 0.0, // a
            0.1, 0.0, 0.0, 0.9, 0.0, // space
            0.1, 0.0, 0.9, 0.0, 0.0, // b
        ];
        let (text, _) = decode_ctc(&logits, 3, 5, &dict);
        assert_eq!(text, "a b");
    }
}
