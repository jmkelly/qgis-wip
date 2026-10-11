//! Line-based diff with unified rendering.
//!
//! Classic LCS dynamic program over lines, grouped into hunks with
//! three context lines, rendered in unified diff format. Pure Rust,
//! no dependencies.

/// A single line in a diff.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Line {
    /// A line present in both outputs.
    Context(String),
    /// A line present only in the old output.
    Removed(String),
    /// A line present only in the new output.
    Added(String),
}

/// A unified diff between two texts.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Diff {
    hunks: Vec<Hunk>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Hunk {
    /// 0-based index into the old text of the hunk's first old line.
    old_start: usize,
    /// 0-based index into the new text of the hunk's first new line.
    new_start: usize,
    old_len: usize,
    new_len: usize,
    /// Inclusive start index into the ops list.
    start: usize,
    /// Exclusive end index into the ops list.
    end: usize,
    lines: Vec<Line>,
}

/// Context lines shown around each change when rendering.
const CONTEXT: usize = 3;

/// Maximum LCS table size (in cells) before falling back to a
/// whole-document replacement diff. Fixtures are small; this only
/// guards against pathological inputs.
const MAX_LCS_CELLS: u64 = 1_000_000;

impl Diff {
    /// Computes the diff between `old` and `new`.
    pub fn between(old: &str, new: &str) -> Diff {
        Self::between_limited(old, new, MAX_LCS_CELLS)
    }

    /// [`Diff::between`] with an explicit LCS cell budget, so the
    /// pathological-input fallback is testable with tiny inputs.
    pub fn between_limited(old: &str, new: &str, max_cells: u64) -> Diff {
        let old_lines: Vec<&str> = old.lines().collect();
        let new_lines: Vec<&str> = new.lines().collect();
        let ops = lcs_ops(&old_lines, &new_lines, max_cells);
        if !ops.iter().any(|l| !matches!(l, Line::Context(_))) {
            return Diff::default();
        }
        let old_prefix = prefix_counts(&ops, |l| {
            matches!(l, Line::Context(_) | Line::Removed(_))
        });
        let new_prefix = prefix_counts(&ops, |l| matches!(l, Line::Context(_) | Line::Added(_)));
        let hunks = group_hunks(&ops, &old_prefix, &new_prefix);
        Diff { hunks }
    }

    /// True when there are no differences.
    pub fn is_empty(&self) -> bool {
        self.hunks.is_empty()
    }

    /// Renders the diff in unified format. `old_label`/`new_label` are
    /// the file labels in the `---`/`+++` header lines.
    pub fn render(&self, old_label: &str, new_label: &str) -> String {
        let mut out = String::new();
        if self.hunks.is_empty() {
            return out;
        }
        out.push_str(&format!("--- {old_label}\n"));
        out.push_str(&format!("+++ {new_label}\n"));
        for hunk in &self.hunks {
            out.push_str(&hunk_header(
                hunk.old_start, hunk.old_len, hunk.new_start, hunk.new_len,
            ));
            out.push('\n');
            for line in &hunk.lines {
                match line {
                    Line::Context(s) => {
                        out.push(' ');
                        out.push_str(s);
                    }
                    Line::Removed(s) => {
                        out.push('-');
                        out.push_str(s);
                    }
                    Line::Added(s) => {
                        out.push('+');
                        out.push_str(s);
                    }
                }
                out.push('\n');
            }
        }
        out
    }
}

/// Per-side prefix counts: `prefix[i]` = number of old (or new) lines
/// in `ops[..i]`.
fn prefix_counts(ops: &[Line], counts: impl Fn(&Line) -> bool) -> Vec<usize> {
    let mut prefix = vec![0usize; ops.len() + 1];
    for (i, line) in ops.iter().enumerate() {
        prefix[i + 1] = prefix[i] + usize::from(counts(line));
    }
    prefix
}

/// LCS-based edit script. Falls back to a whole-document replacement
/// when the DP table would exceed `max_cells`.
fn lcs_ops(old: &[&str], new: &[&str], max_cells: u64) -> Vec<Line> {
    let cells = (old.len() as u64) * (new.len() as u64);
    if cells > max_cells {
        let mut ops: Vec<Line> = old
            .iter()
            .map(|l| Line::Removed((*l).to_string()))
            .collect();
        ops.extend(new.iter().map(|l| Line::Added((*l).to_string())));
        return ops;
    }

    // dp[i][j] = LCS length of old[i..] and new[j..].
    let mut dp = vec![vec![0u32; new.len() + 1]; old.len() + 1];
    for i in (0..old.len()).rev() {
        for j in (0..new.len()).rev() {
            dp[i][j] = if old[i] == new[j] {
                dp[i + 1][j + 1] + 1
            } else {
                dp[i + 1][j].max(dp[i][j + 1])
            };
        }
    }

    let mut ops = Vec::new();
    let (mut i, mut j) = (0, 0);
    while i < old.len() && j < new.len() {
        if old[i] == new[j] {
            ops.push(Line::Context(old[i].to_string()));
            i += 1;
            j += 1;
        } else if dp[i + 1][j] >= dp[i][j + 1] {
            ops.push(Line::Removed(old[i].to_string()));
            i += 1;
        } else {
            ops.push(Line::Added(new[j].to_string()));
            j += 1;
        }
    }
    while i < old.len() {
        ops.push(Line::Removed(old[i].to_string()));
        i += 1;
    }
    while j < new.len() {
        ops.push(Line::Added(new[j].to_string()));
        j += 1;
    }
    ops
}

/// Groups the edit script into hunks: each maximal run of changes plus
/// up to [`CONTEXT`] context lines on each side, merging hunks whose
/// padding overlaps or touches.
fn group_hunks(ops: &[Line], old_prefix: &[usize], new_prefix: &[usize]) -> Vec<Hunk> {
    let mut hunks: Vec<Hunk> = Vec::new();
    let mut i = 0;
    while i < ops.len() {
        if matches!(ops[i], Line::Context(_)) {
            i += 1;
            continue;
        }
        let run_start = i;
        while i < ops.len() && !matches!(ops[i], Line::Context(_)) {
            i += 1;
        }
        let run_end = i;
        let start = run_start.saturating_sub(CONTEXT);
        let end = (run_end + CONTEXT).min(ops.len());
        if let Some(last) = hunks.last_mut() {
            // Merge when the padded regions overlap or touch.
            if start <= last.end {
                last.end = end;
                continue;
            }
        }
        hunks.push(Hunk {
            old_start: 0,
            new_start: 0,
            old_len: 0,
            new_len: 0,
            start,
            end,
            lines: Vec::new(),
        });
    }
    for hunk in &mut hunks {
        hunk.old_start = old_prefix[hunk.start];
        hunk.new_start = new_prefix[hunk.start];
        hunk.old_len = old_prefix[hunk.end] - old_prefix[hunk.start];
        hunk.new_len = new_prefix[hunk.end] - new_prefix[hunk.start];
        hunk.lines = ops[hunk.start..hunk.end].to_vec();
    }
    hunks
}

/// Standard `@@ -old +new @@` hunk header. Single-position hunks omit
/// the length; zero-length sides show the position before the hunk.
fn hunk_header(old_start: usize, old_len: usize, new_start: usize, new_len: usize) -> String {
    let (os, ol) = if old_len == 0 {
        (old_start, 0)
    } else {
        (old_start + 1, old_len)
    };
    let (ns, nl) = if new_len == 0 {
        (new_start, 0)
    } else {
        (new_start + 1, new_len)
    };
    let o = if ol == 1 {
        format!("{os}")
    } else {
        format!("{os},{ol}")
    };
    let n = if nl == 1 {
        format!("{ns}")
    } else {
        format!("{ns},{nl}")
    };
    format!("@@ -{o} +{n} @@")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_inputs_produce_empty_diff() {
        let diff = Diff::between("a\nb\nc\n", "a\nb\nc\n");
        assert!(diff.is_empty());
        assert_eq!(diff.render("a", "b"), "");
    }

    #[test]
    fn empty_old_is_all_additions() {
        let diff = Diff::between("", "x\ny\nz\n");
        assert!(!diff.is_empty());
        let rendered = diff.render("old", "new");
        assert!(rendered.contains("--- old\n"));
        assert!(rendered.contains("+++ new\n"));
        assert!(rendered.contains("@@ -0,0 +1,3 @@"), "rendered:\n{rendered}");
        assert!(rendered.contains("+x\n"));
        assert!(rendered.contains("+y\n"));
        assert!(rendered.contains("+z\n"));
    }

    #[test]
    fn empty_new_is_all_removals() {
        let diff = Diff::between("x\ny\n", "");
        let rendered = diff.render("old", "new");
        assert!(rendered.contains("@@ -1,2 +0,0 @@"), "rendered:\n{rendered}");
        assert!(rendered.contains("-x\n"));
        assert!(rendered.contains("-y\n"));
    }

    #[test]
    fn single_line_change_shows_context() {
        let old = "one\ntwo\nthree\nfour\nfive\n";
        let new = "one\ntwo\nTHREE\nfour\nfive\n";
        let diff = Diff::between(old, new);
        assert!(!diff.is_empty());
        let rendered = diff.render("old", "new");
        assert!(rendered.contains("@@ -1,5 +1,5 @@"), "rendered:\n{rendered}");
        assert!(rendered.contains(" one\n"));
        assert!(rendered.contains("-three\n"));
        assert!(rendered.contains("+THREE\n"));
        assert!(rendered.contains(" four\n"));
    }

    #[test]
    fn distant_changes_produce_two_hunks() {
        let old = "a\nb\nc\nd\ne\nf\ng\nh\ni\nj\n";
        let new = "A\nb\nc\nd\ne\nf\ng\nh\ni\nJ\n";
        let diff = Diff::between(old, new);
        assert_eq!(diff.hunks.len(), 2, "rendered:\n{}", diff.render("old", "new"));
        let rendered = diff.render("old", "new");
        assert!(rendered.contains("@@ -1,4 +1,4 @@"), "rendered:\n{rendered}");
        assert!(rendered.contains("@@ -7,4 +7,4 @@"), "rendered:\n{rendered}");
    }

    #[test]
    fn nearby_changes_merge_into_one_hunk() {
        // Two changes separated by 4 context lines (<= 2*CONTEXT) merge.
        let old = "a\nb\nc\nd\ne\nf\ng\nh\n";
        let new = "A\nb\nc\nd\ne\nf\ng\nH\n";
        let diff = Diff::between(old, new);
        let rendered = diff.render("old", "new");
        assert_eq!(diff.hunks.len(), 1, "rendered:\n{rendered}");
        assert!(rendered.contains("@@ -1,8 +1,8 @@"), "rendered:\n{rendered}");
    }

    #[test]
    fn merge_boundary_is_six_context_lines() {
        // Changes 7 lines apart (6 context lines between the runs)
        // merge: the 3+3 padding exactly covers the gap.
        let old = "a\nb\nc\nd\ne\nf\ng\nh\ni\nj\nk\n";
        let new = "A\nb\nc\nd\ne\nf\ng\nH\ni\nj\nk\n";
        let diff = Diff::between(old, new);
        assert_eq!(diff.hunks.len(), 1, "rendered:\n{}", diff.render("o", "n"));

        // Changes 8 lines apart (7 context lines between the runs) stay split.
        let new = "A\nb\nc\nd\ne\nf\ng\nh\nI\nj\nk\n";
        let diff = Diff::between(old, new);
        assert_eq!(diff.hunks.len(), 2, "rendered:\n{}", diff.render("o", "n"));
    }

    #[test]
    fn changed_line_at_start() {
        let diff = Diff::between("old\nb\n", "new\nb\n");
        let rendered = diff.render("o", "n");
        assert!(rendered.contains("@@ -1,2 +1,2 @@"), "rendered:\n{rendered}");
        assert!(rendered.contains("-old\n"));
        assert!(rendered.contains("+new\n"));
        assert!(rendered.contains(" b\n"));
    }

    #[test]
    fn changed_line_at_end() {
        let diff = Diff::between("a\nold\n", "a\nnew\n");
        let rendered = diff.render("o", "n");
        assert!(rendered.contains("@@ -1,2 +1,2 @@"), "rendered:\n{rendered}");
        assert!(rendered.contains(" a\n"));
        assert!(rendered.contains("-old\n"));
        assert!(rendered.contains("+new\n"));
    }

    #[test]
    fn crlf_is_normalized() {
        let diff = Diff::between("a\r\nb\r\n", "a\nb\n");
        assert!(diff.is_empty());
    }

    #[test]
    fn pathological_input_falls_back_to_replacement() {
        let old = "a\nb\nc\n";
        let new = "x\ny\nz\n";
        // Cap of 1 cell forces the fallback path.
        let diff = Diff::between_limited(old, new, 1);
        assert!(!diff.is_empty());
        let rendered = diff.render("old", "new");
        assert!(rendered.contains("-a\n"));
        assert!(rendered.contains("+x\n"));
    }

    #[test]
    fn render_labels_appear_in_header() {
        let diff = Diff::between("a\n", "b\n");
        let rendered = diff.render("goldens/trivial.expected", "actual");
        assert!(rendered.starts_with("--- goldens/trivial.expected\n+++ actual\n"));
    }
}
