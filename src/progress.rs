use console::style;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use std::time::Duration;

const LABEL_WIDTH: usize = 16;

pub struct Row {
    label: String,
    sp: ProgressBar,
    is_stat: bool,
}

impl Row {
    pub fn new(mp: &MultiProgress, label: &str) -> Self {
        let bar = mp.add(ProgressBar::new_spinner());
        bar.set_style(
            ProgressStyle::default_spinner()
                .template("  {spinner:.cyan} {msg}")
                .unwrap(),
        );
        bar.enable_steady_tick(Duration::from_millis(80));
        bar.set_message(format!("{:<LABEL_WIDTH$}", label));
        Self {
            label: label.to_string(),
            sp: bar,
            is_stat: false,
        }
    }

    pub fn new_stat(mp: &MultiProgress, label: &str) -> Self {
        let bar = mp.add(ProgressBar::new_spinner());
        bar.set_style(
            ProgressStyle::default_spinner()
                .template("      {msg}")
                .unwrap(),
        );
        bar.set_message(format!("{:<LABEL_WIDTH$}", label));
        Self {
            label: label.to_string(),
            sp: bar,
            is_stat: true,
        }
    }

    pub fn update(&self, value: &str) {
        self.sp
            .set_message(format!("{:<LABEL_WIDTH$} {}", self.label, value));
    }

    pub fn finish(self, value: &str) {
        if self.is_stat {
            self.sp
                .finish_with_message(format!("{:<LABEL_WIDTH$} {}", self.label, value,));
        } else {
            self.sp.set_style(
                ProgressStyle::default_spinner()
                    .template("  {msg}")
                    .unwrap(),
            );
            self.sp.finish_with_message(format!(
                "{} {:<LABEL_WIDTH$} {}",
                style("✓").green(),
                self.label,
                value,
            ));
        }
    }
}
