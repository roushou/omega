use super::{Step, Ui};
use serde_json::Value;
use std::collections::BTreeSet;

impl Ui {
    /// Differences from the current shell file to the built configuration.
    /// Paths are JSON pointers; array positions preserve layout ordering.
    pub fn shell_diff(&mut self, current: Option<&Value>, built: &Value) {
        self.shell_change("", current, Some(built));
    }

    fn shell_change(&mut self, path: &str, current: Option<&Value>, built: Option<&Value>) {
        if current == built {
            return;
        }
        match (current, built) {
            (Some(Value::Object(left)), Some(Value::Object(right))) => {
                let keys: BTreeSet<_> = left.keys().chain(right.keys()).collect();
                for key in keys {
                    let segment = key.replace('~', "~0").replace('/', "~1");
                    self.shell_change(&format!("{path}/{segment}"), left.get(key), right.get(key));
                }
            }
            (Some(Value::Array(left)), Some(Value::Array(right))) => {
                for index in 0..left.len().max(right.len()) {
                    self.shell_change(
                        &format!("{path}/{index}"),
                        left.get(index),
                        right.get(index),
                    );
                }
            }
            _ => {
                self.step(
                    Step::Changed,
                    if path.is_empty() { "(document)" } else { path },
                );
                self.detail(format!(
                    "current: {}",
                    current.map_or_else(|| "(absent)".into(), ToString::to_string)
                ));
                self.detail(format!(
                    "built:   {}",
                    built.map_or_else(|| "(absent)".into(), ToString::to_string)
                ));
            }
        }
    }
}
