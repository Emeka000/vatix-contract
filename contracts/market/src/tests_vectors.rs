//! Regression corpus for share/fee/payout math (#504).
//!
//! Loads `test-vectors/fee-math.json` at test time and replays every case
//! through `validation::calculate_fee`, so a reintroduced rounding/overflow
//! bug fails this test instead of surfacing later in production.

use crate::error::ContractError;
use crate::validation::calculate_fee;
use serde::Deserialize;

#[derive(Deserialize)]
struct Corpus {
    vectors: std::vec::Vec<Vector>,
}

#[derive(Deserialize)]
struct Vector {
    id: std::string::String,
    #[allow(dead_code)]
    description: std::string::String,
    amount: std::string::String,
    fee_rate_bps: i128,
    expected: Expected,
}

#[derive(Deserialize)]
struct Expected {
    ok: Option<std::string::String>,
    error: Option<std::string::String>,
}

fn error_name(err: ContractError) -> &'static str {
    match err {
        ContractError::InvalidQuantity => "InvalidQuantity",
        ContractError::InvalidPrice => "InvalidPrice",
        ContractError::ArithmeticOverflow => "ArithmeticOverflow",
        _ => "Other",
    }
}

#[test]
fn fee_math_regression_corpus() {
    let raw = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../test-vectors/fee-math.json"
    ))
    .expect("read test-vectors/fee-math.json");
    let corpus: Corpus = serde_json::from_str(&raw).expect("parse fee-math.json");

    assert!(
        corpus.vectors.len() >= 5,
        "corpus must document at least 5 cases"
    );

    for vector in &corpus.vectors {
        let amount: i128 = vector
            .amount
            .parse()
            .unwrap_or_else(|_| panic!("vector {}: invalid amount", vector.id));
        let result = calculate_fee(amount, vector.fee_rate_bps);

        match (&vector.expected.ok, &vector.expected.error) {
            (Some(expected_ok), None) => {
                let expected: i128 = expected_ok
                    .parse()
                    .unwrap_or_else(|_| panic!("vector {}: invalid expected.ok", vector.id));
                assert_eq!(
                    result,
                    Ok(expected),
                    "vector {}: unexpected fee result",
                    vector.id
                );
            }
            (None, Some(expected_err)) => {
                let err = result.expect_err(&std::format!(
                    "vector {}: expected error {}, got Ok",
                    vector.id,
                    expected_err
                ));
                assert_eq!(
                    error_name(err),
                    expected_err.as_str(),
                    "vector {}: wrong error variant",
                    vector.id
                );
            }
            _ => panic!(
                "vector {}: expected must have exactly one of ok/error",
                vector.id
            ),
        }
    }
}
