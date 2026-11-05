//! Saving the caption history as text, in the same layout as the Mac app.

use crate::HistoryLine;
use slint::Model;

pub fn text(lines: &impl Model<Data = HistoryLine>) -> String {
    (0..lines.row_count())
        .filter_map(|row| lines.row_data(row))
        .map(|line| format!("[{}] {}", line.time, line.text))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Asks where to save, then writes the file. Says what went wrong, if anything.
pub async fn save(contents: String) -> Result<(), String> {
    let chosen = rfd::AsyncFileDialog::new()
        .set_title("Export captions")
        .set_file_name("Captions.txt")
        .add_filter("Text", &["txt"])
        .save_file()
        .await;
    match chosen {
        Some(file) => std::fs::write(file.path(), contents).map_err(|e| e.to_string()),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use slint::VecModel;

    #[test]
    fn lines_carry_their_time() {
        let lines = VecModel::from(vec![
            HistoryLine {
                time: "00:03".into(),
                text: "First.".into(),
            },
            HistoryLine {
                time: "01:20".into(),
                text: "Second.".into(),
            },
        ]);
        assert_eq!(text(&lines), "[00:03] First.\n[01:20] Second.");
    }
}
