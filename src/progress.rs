//! Progress bars on standard error while a snapshot downloads and loads.

use std::sync::{Mutex, PoisonError};
use std::time::Duration;

use indicatif::{ProgressBar, ProgressDrawTarget, ProgressStyle};
use originet::fetch::{Progress, Step};

/// Draws one bar at a time on standard error and clears it when the step ends, so
/// nothing is left behind in the terminal.
#[derive(Debug, Default)]
pub struct Bars {
    current: Mutex<Option<ProgressBar>>,
    colored: bool,
}

impl Bars {
    /// Bars that are colored, or not (`--color never`).
    pub fn new(colored: bool) -> Self {
        Self {
            current: Mutex::new(None),
            colored,
        }
    }

    fn swap(&self, bar: Option<ProgressBar>) -> Option<ProgressBar> {
        let mut current = self.current.lock().unwrap_or_else(PoisonError::into_inner);
        std::mem::replace(&mut current, bar)
    }
}

impl Progress for Bars {
    fn start(&self, step: Step<'_>) {
        let (length, template, message) = match step {
            Step::Download {
                file,
                total: Some(total),
            } => (
                Some(total),
                if self.colored {
                    "{msg} [{bar:30.cyan/dim}] {bytes}/{total_bytes} {bytes_per_sec} eta {eta}"
                } else {
                    "{msg} [{bar:30}] {bytes}/{total_bytes} {bytes_per_sec} eta {eta}"
                },
                format!("downloading {file}"),
            ),
            Step::Download { file, total: None } => (
                None,
                "{spinner} {msg} {bytes} {bytes_per_sec}",
                format!("downloading {file}"),
            ),
            Step::Load { file } => (None, "{spinner} {msg}", format!("loading {file}")),
        };
        let bar = ProgressBar::with_draw_target(length, ProgressDrawTarget::stderr())
            .with_style(style(template))
            .with_message(message);
        bar.enable_steady_tick(Duration::from_millis(100));
        if let Some(previous) = self.swap(Some(bar)) {
            previous.finish_and_clear();
        }
    }

    fn advance(&self, bytes: u64) {
        if let Some(bar) = &*self.current.lock().unwrap_or_else(PoisonError::into_inner) {
            bar.inc(bytes);
        }
    }

    fn finish(&self) {
        if let Some(bar) = self.swap(None) {
            bar.finish_and_clear();
        }
    }
}

fn style(template: &str) -> ProgressStyle {
    ProgressStyle::with_template(template)
        .expect("the templates are literals checked by the unit test")
        .progress_chars("=> ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_step_has_a_valid_template() {
        for bars in [Bars::new(true), Bars::new(false)] {
            exercise(&bars);
        }
    }

    fn exercise(bars: &Bars) {
        for step in [
            Step::Download {
                file: "a.gz",
                total: Some(10),
            },
            Step::Download {
                file: "a.gz",
                total: None,
            },
            Step::Load { file: "a.gz" },
        ] {
            bars.start(step);
            bars.advance(5);
            bars.finish();
        }
        assert!(bars.swap(None).is_none());
    }
}
