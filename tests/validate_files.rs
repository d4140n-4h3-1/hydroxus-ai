//! A tuning read from a file: every number in it checked, and a misspelt name refused.
#![cfg(feature = "serde")]

use hydroxus_ai::Tuning;
use serde_json::Value;

/// Every decimal number in `value`, by the name `validate` gives it, and where it is in the JSON.
fn leaves(value: &Value, name: String, path: Vec<String>, out: &mut Vec<(String, Vec<String>)>) {
    match value {
        Value::Object(fields) => {
            for (key, field) in fields {
                let name = if name.is_empty() { key.clone() } else { format!("{name}.{key}") };
                leaves(field, name, [path.clone(), vec![key.clone()]].concat(), out);
            }
        }
        Value::Array(items) => {
            for (n, item) in items.iter().enumerate() {
                // A list of pairs is `name[n]`; a pair's own two are `name.0` and `name.1`.
                let name = if item.is_array() { format!("{name}[{n}]") } else { format!("{name}.{n}") };
                leaves(item, name, [path.clone(), vec![n.to_string()]].concat(), out);
            }
        }
        Value::Number(number) if number.is_f64() => out.push((name, path)),
        _ => (),
    }
}

fn at<'a>(value: &'a mut Value, path: &[String]) -> &'a mut Value {
    path.iter().fold(value, |value, step| match value {
        Value::Array(items) => &mut items[step.parse::<usize>().unwrap()],
        other => &mut other[step.as_str()],
    })
}

#[test]
fn every_number_in_a_tuning_is_checked() {
    let defaults = serde_json::to_value(Tuning::DEFAULT).unwrap();
    let mut found = Vec::new();
    leaves(&defaults, String::new(), Vec::new(), &mut found);
    assert!(found.len() > 200, "{}", found.len());
    let mut checked = 0;
    for (name, path) in found {
        // How far to a leader's right may well be less than none: to its left.
        if name.starts_with("group.about_leader[") && name.ends_with(".0") {
            continue;
        }
        let mut file = defaults.clone();
        // Past -1: wrong for every number, the regard thresholds below neutral too.
        *at(&mut file, &path) = Value::from(-5.0);
        let tuning: Tuning = serde_json::from_value(file).unwrap();
        let invalid = tuning.validate().expect_err(&name);
        assert_eq!(invalid.problems.iter().map(|p| p.field.clone()).collect::<Vec<_>>(), vec![name.clone()]);
        checked += 1;
    }
    assert!(checked > 200, "{checked}");
}

#[test]
fn a_misspelt_name_is_refused_saying_which() {
    let error = serde_json::from_str::<Tuning>(r#"{"detection": {"fil": 2.0}}"#).unwrap_err().to_string();
    assert!(error.contains("unknown field `fil`") && error.contains("`fill`"), "{error}");
    let error = serde_json::from_str::<Tuning>(r#"{"detectoin": {}}"#).unwrap_err().to_string();
    assert!(error.contains("unknown field `detectoin`"), "{error}");
    let error = serde_json::from_str::<Tuning>(r#"{"sight": {"rnage": 10.0}}"#).unwrap_err().to_string();
    assert!(error.contains("unknown field `rnage`"), "{error}");
}

#[test]
fn what_a_file_leaves_out_keeps_its_default_and_what_it_sets_is_checked() {
    let tuning: Tuning = serde_json::from_str(r#"{"patrol": {"engage": 15.0}, "alert": {"caution": 90.0}}"#).unwrap();
    assert_eq!(tuning.alert.caution, 90.0);
    assert_eq!(tuning.alert.evasion, Tuning::DEFAULT.alert.evasion);
    assert_eq!(tuning.validate(), Ok(()));
    let tuning: Tuning = serde_json::from_str(r#"{"patrol": {"engage": 25.0}}"#).unwrap();
    assert_eq!(tuning.validate().unwrap_err().to_string(), "patrol.engage is 25, but must be no more than patrol.fire_range (20)");
}
