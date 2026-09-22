//! Deterministic answer proofs (REQ-022).
//!
//! An item's `AnswerProof::Executable` carries an `expression` and an `answer`.
//! This module is the *independent* route that checks the second against the
//! first: it parses the expression text and re-evaluates it without calling any
//! of the generator's arithmetic. If the two disagree, the item is not proven --
//! `QUESTION_FACTORY.md` step 7 requires the verifier to use a distinct route and
//! not simply echo the generator, and a verifier that read the generator's own
//! intermediate values would do exactly that.
//!
//! The evaluator is integer-only over `i128`. Two properties matter for use as a
//! proof:
//!
//! * **Division must be exact.** `7/2` is an error rather than `3`, because a
//!   silently truncating proof proves nothing about the intended answer. A
//!   template that needs a fraction must express it as `(a * b) / c` with `c`
//!   dividing the product, which is checkable.
//! * **Overflow is an error.** A wrapped `i128` would happily equal a wrong
//!   answer.
//!
//! Grammar:
//!
//! ```text
//! expr    := term (('+' | '-') term)*
//! term    := unary (('*' | '/') unary)*
//! unary   := '-' unary | primary
//! primary := number | '(' expr ')'
//! ```

use std::fmt;

/// Why an expression could not be evaluated as a proof.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProofError {
    /// Nothing to evaluate.
    Empty,
    /// A character that is not part of the grammar.
    UnexpectedCharacter { character: char, position: usize },
    /// Input ended where a term was required, e.g. `1 +`.
    UnexpectedEnd,
    /// A closing parenthesis was missing.
    UnclosedParenthesis,
    /// A closing parenthesis appeared with no matching opener, e.g. `1)`.
    UnmatchedClosingParenthesis,
    /// A number literal too large for `i128`.
    NumberTooLarge { literal: String },
    /// Division by zero.
    DivisionByZero,
    /// Division that does not divide exactly. A truncating proof is not a proof.
    NonExactDivision { numerator: i128, denominator: i128 },
    /// Arithmetic overflowed `i128`.
    Overflow,
    /// The declared answer is not an integer literal.
    AnswerNotAnInteger { literal: String },
    /// The declared answer disagrees with a fresh evaluation of the expression.
    AnswerMismatch { declared: i128, recomputed: i128 },
}

impl fmt::Display for ProofError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProofError::Empty => write!(f, "proof expression is empty"),
            ProofError::UnexpectedCharacter {
                character,
                position,
            } => write!(
                f,
                "unexpected character {character:?} at position {position}"
            ),
            ProofError::UnexpectedEnd => write!(f, "proof expression ended early"),
            ProofError::UnclosedParenthesis => write!(f, "unclosed parenthesis"),
            ProofError::UnmatchedClosingParenthesis => {
                write!(f, "closing parenthesis with no opener")
            }
            ProofError::NumberTooLarge { literal } => {
                write!(f, "number literal {literal:?} does not fit in i128")
            }
            ProofError::DivisionByZero => write!(f, "division by zero"),
            ProofError::NonExactDivision {
                numerator,
                denominator,
            } => write!(
                f,
                "{numerator} is not exactly divisible by {denominator}, so the \
                 proof would rely on truncation"
            ),
            ProofError::Overflow => write!(f, "proof expression overflowed i128"),
            ProofError::AnswerNotAnInteger { literal } => {
                write!(f, "declared answer {literal:?} is not an integer")
            }
            ProofError::AnswerMismatch {
                declared,
                recomputed,
            } => write!(
                f,
                "declared answer {declared} disagrees with the expression, which \
                 evaluates to {recomputed}"
            ),
        }
    }
}

impl std::error::Error for ProofError {}

struct Parser<'a> {
    input: &'a [u8],
    position: usize,
}

impl<'a> Parser<'a> {
    fn new(input: &'a str) -> Self {
        Self {
            input: input.as_bytes(),
            position: 0,
        }
    }

    fn peek(&self) -> Option<u8> {
        self.input.get(self.position).copied()
    }

    /// Skip spaces and tabs. Newlines are not permitted: an expression is a
    /// single line, and accepting newlines would let two expressions concatenate
    /// into one that still parses.
    fn skip_whitespace(&mut self) {
        while matches!(self.peek(), Some(b' ') | Some(b'\t')) {
            self.position += 1;
        }
    }

    fn parse_expression(&mut self) -> Result<i128, ProofError> {
        let mut value = self.parse_term()?;
        loop {
            self.skip_whitespace();
            match self.peek() {
                Some(b'+') => {
                    self.position += 1;
                    let right = self.parse_term()?;
                    value = value.checked_add(right).ok_or(ProofError::Overflow)?;
                }
                Some(b'-') => {
                    self.position += 1;
                    let right = self.parse_term()?;
                    value = value.checked_sub(right).ok_or(ProofError::Overflow)?;
                }
                _ => return Ok(value),
            }
        }
    }

    fn parse_term(&mut self) -> Result<i128, ProofError> {
        let mut value = self.parse_unary()?;
        loop {
            self.skip_whitespace();
            match self.peek() {
                Some(b'*') => {
                    self.position += 1;
                    let right = self.parse_unary()?;
                    value = value.checked_mul(right).ok_or(ProofError::Overflow)?;
                }
                Some(b'/') => {
                    self.position += 1;
                    let right = self.parse_unary()?;
                    if right == 0 {
                        return Err(ProofError::DivisionByZero);
                    }
                    if value % right != 0 {
                        return Err(ProofError::NonExactDivision {
                            numerator: value,
                            denominator: right,
                        });
                    }
                    value /= right;
                }
                _ => return Ok(value),
            }
        }
    }

    fn parse_unary(&mut self) -> Result<i128, ProofError> {
        self.skip_whitespace();
        if self.peek() == Some(b'-') {
            self.position += 1;
            let value = self.parse_unary()?;
            return value.checked_neg().ok_or(ProofError::Overflow);
        }
        self.parse_primary()
    }

    fn parse_primary(&mut self) -> Result<i128, ProofError> {
        self.skip_whitespace();
        match self.peek() {
            None => Err(ProofError::UnexpectedEnd),
            Some(b'(') => {
                self.position += 1;
                let value = self.parse_expression()?;
                self.skip_whitespace();
                match self.peek() {
                    Some(b')') => {
                        self.position += 1;
                        Ok(value)
                    }
                    None => Err(ProofError::UnclosedParenthesis),
                    Some(byte) => Err(ProofError::UnexpectedCharacter {
                        character: byte as char,
                        position: self.position,
                    }),
                }
            }
            Some(b')') => Err(ProofError::UnmatchedClosingParenthesis),
            Some(byte) if byte.is_ascii_digit() => {
                let start = self.position;
                while matches!(self.peek(), Some(b) if b.is_ascii_digit()) {
                    self.position += 1;
                }
                let literal =
                    std::str::from_utf8(&self.input[start..self.position]).map_err(|_| {
                        ProofError::UnexpectedCharacter {
                            character: '?',
                            position: start,
                        }
                    })?;
                literal
                    .parse::<i128>()
                    .map_err(|_| ProofError::NumberTooLarge {
                        literal: literal.to_string(),
                    })
            }
            Some(byte) => Err(ProofError::UnexpectedCharacter {
                character: byte as char,
                position: self.position,
            }),
        }
    }
}

/// Evaluate a proof expression to an exact integer.
///
/// Returns an error rather than a rounded or truncated value whenever the
/// expression cannot be evaluated exactly.
pub fn evaluate(expression: &str) -> Result<i128, ProofError> {
    let trimmed = expression.trim();
    if trimmed.is_empty() {
        return Err(ProofError::Empty);
    }
    let mut parser = Parser::new(trimmed);
    let value = parser.parse_expression()?;
    parser.skip_whitespace();
    if parser.position != parser.input.len() {
        let byte = parser.input[parser.position];
        // A stray closing parenthesis parses cleanly as a complete expression and
        // then leaves input behind. Naming it precisely is worth the branch: the
        // two cases need different fixes.
        if byte == b')' {
            return Err(ProofError::UnmatchedClosingParenthesis);
        }
        return Err(ProofError::UnexpectedCharacter {
            character: byte as char,
            position: parser.position,
        });
    }
    Ok(value)
}

/// Check a declared answer against its expression, independently.
///
/// The declared answer is parsed as an integer and compared to a fresh
/// evaluation of the expression. Comparing text would let `"1800"` and
/// `"01800"` disagree, so both sides are normalised through `i128`.
///
/// A disagreement is an error. Returning the recomputed value on mismatch would
/// make this function incapable of failing, which is the opposite of its purpose.
pub fn verify_answer(expression: &str, answer: &str) -> Result<i128, ProofError> {
    let recomputed = evaluate(expression)?;
    let declared = answer
        .trim()
        .parse::<i128>()
        .map_err(|_| ProofError::AnswerNotAnInteger {
            literal: answer.to_string(),
        })?;
    if recomputed != declared {
        return Err(ProofError::AnswerMismatch {
            declared,
            recomputed,
        });
    }
    Ok(recomputed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evaluates_integer_arithmetic_with_precedence() {
        assert_eq!(evaluate("2 + 3 * 4").expect("evaluate"), 14);
        assert_eq!(evaluate("(2 + 3) * 4").expect("evaluate"), 20);
        assert_eq!(evaluate("12 * 150").expect("evaluate"), 1800);
        assert_eq!(evaluate("100 - 40 - 10").expect("evaluate"), 50);
    }

    #[test]
    fn evaluation_respects_left_associativity_for_division() {
        // 100 / 5 / 2 is (100/5)/2 = 10, not 100/(5/2).
        assert_eq!(evaluate("100 / 5 / 2").expect("evaluate"), 10);
    }

    #[test]
    fn unary_minus_is_supported() {
        assert_eq!(evaluate("-5 + 8").expect("evaluate"), 3);
        assert_eq!(evaluate("-(3 + 4)").expect("evaluate"), -7);
    }

    #[test]
    fn inexact_division_is_refused_rather_than_truncated() {
        assert_eq!(
            evaluate("7 / 2"),
            Err(ProofError::NonExactDivision {
                numerator: 7,
                denominator: 2
            })
        );
    }

    #[test]
    fn exact_division_is_accepted() {
        assert_eq!(evaluate("18 / 9").expect("evaluate"), 2);
        assert_eq!(evaluate("(6 * 3) / (6 + 3)").expect("evaluate"), 2);
    }

    #[test]
    fn division_by_zero_is_refused() {
        assert_eq!(evaluate("5 / 0"), Err(ProofError::DivisionByZero));
        assert_eq!(evaluate("5 / (3 - 3)"), Err(ProofError::DivisionByZero));
    }

    #[test]
    fn malformed_expressions_are_refused() {
        assert_eq!(evaluate(""), Err(ProofError::Empty));
        assert_eq!(evaluate("   "), Err(ProofError::Empty));
        assert_eq!(evaluate("1 +"), Err(ProofError::UnexpectedEnd));
        assert_eq!(evaluate("(1 + 2"), Err(ProofError::UnclosedParenthesis));
        assert_eq!(
            evaluate("1 + 2)"),
            Err(ProofError::UnmatchedClosingParenthesis)
        );
        assert!(matches!(
            evaluate("2 ^ 3"),
            Err(ProofError::UnexpectedCharacter { .. })
        ));
        assert!(matches!(
            evaluate("1 + 2 3"),
            Err(ProofError::UnexpectedCharacter { .. })
        ));
        // A newline must not let two expressions concatenate.
        assert!(matches!(
            evaluate("1 + 2\n3 + 4"),
            Err(ProofError::UnexpectedCharacter { .. })
        ));
    }

    #[test]
    fn overflow_is_an_error_not_a_wrap() {
        let huge = "170141183460469231731687303715884105727";
        // i128::MAX * 2 must not silently wrap to a plausible-looking number.
        assert_eq!(evaluate(&format!("{huge} * 2")), Err(ProofError::Overflow));
    }

    #[test]
    fn verify_answer_recomputes_rather_than_trusting_the_declaration() {
        assert_eq!(verify_answer("12 * 150", "1800").expect("match"), 1800);
        // A wrong declaration must be refused, not merely reported.
        assert_eq!(
            verify_answer("12 * 150", "360"),
            Err(ProofError::AnswerMismatch {
                declared: 360,
                recomputed: 1800
            })
        );
        // Non-numeric declarations are refused.
        assert_eq!(
            verify_answer("12 * 150", "eighteen hundred"),
            Err(ProofError::AnswerNotAnInteger {
                literal: "eighteen hundred".to_string()
            })
        );
    }

    #[test]
    fn a_wrong_expression_is_caught_even_when_the_answer_looks_plausible() {
        // The generator claimed 18 pages/min x 150 min = 1800, but wrote the
        // expression for the unconverted form. Verification must not agree.
        assert!(verify_answer("12 * 2", "1800").is_err());
    }
}
