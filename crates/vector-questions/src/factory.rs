//! The original-item factory (REQ-022, `QUESTION_FACTORY.md`).
//!
//! This is the half of REQ-022 that was never built. `crates/vector-questions`
//! could decide what content was *permitted* to enter the corpus, but nothing
//! produced content, so the product shipped three literal questions.
//!
//! ## Why generation rather than authoring
//!
//! Arithmetic Reasoning and Mathematics Knowledge are computable, and a
//! generated item can carry a proof that a machine can check without trusting
//! the generator: the item declares an arithmetic expression, and
//! [`crate::proof::verify_answer`] re-evaluates that expression text through a
//! separate parser. An authored item can only carry a reviewer's assertion.
//!
//! ## Sourcing
//!
//! Templates here are **original**. They encode the *problem types* that
//! Arithmetic Reasoning and Mathematics Knowledge assess -- rate, proportion,
//! percentage, simple interest, perimeter, work rate, linear equations, slope,
//! exponent rules, volume -- which are mathematical methods and therefore not
//! protected expression. No template reproduces a published question, and none
//! may: see `reference/asvab-test-specification.md` for the sourcing rules and
//! the programme's own statement that third parties do not hold real items.
//!
//! ## Determinism
//!
//! Generation is seeded and reproducible. The same seed yields a byte-identical
//! item, so a pack can be regenerated and its hashes re-derived rather than
//! trusted -- which is what makes a generated corpus auditable.

use std::collections::BTreeMap;

use crate::proof;

/// SplitMix64. Small, fast, and fully deterministic from a `u64` seed, which is
/// what reproducibility requires; a cryptographic generator would be slower and,
/// worse, not reproducible across platforms without pinning an algorithm anyway.
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed)
    }

    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform-ish integer in `low..=high`. Panics if the range is empty, which
    /// is a programming error in a template rather than a runtime condition.
    pub fn range(&mut self, low: i64, high: i64) -> i64 {
        assert!(low <= high, "empty range {low}..={high}");
        let span = (high - low) as u64 + 1;
        low + (self.next_u64() % span) as i64
    }

    /// Pick one element. Panics on an empty slice for the same reason.
    pub fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        assert!(!items.is_empty(), "cannot pick from an empty slice");
        let index = (self.next_u64() % items.len() as u64) as usize;
        &items[index]
    }

    /// Fisher-Yates shuffle, so option order varies with the seed and a learner
    /// cannot memorise "the answer is always B".
    pub(crate) fn shuffle<T>(&mut self, items: &mut [T]) {
        for i in (1..items.len()).rev() {
            let j = (self.next_u64() % (i as u64 + 1)) as usize;
            items.swap(i, j);
        }
    }
}

/// One generated item, before it is given provenance and a lifecycle state.
#[derive(Debug, Clone, PartialEq)]
pub struct GeneratedItem {
    pub subtest: String,
    pub objective_id: String,
    pub template_id: &'static str,
    pub stem: String,
    /// Options in presentation order.
    pub options: Vec<String>,
    pub correct_index: usize,
    /// The deterministic proof: an expression and the answer it must evaluate to.
    pub expression: String,
    pub answer: String,
    /// Rationale per wrong option index, naming the misconception it represents
    /// (`QUESTION_FACTORY.md` step 5).
    pub distractor_rationales: BTreeMap<usize, String>,
    pub difficulty: f64,
    pub seed: u64,
}

impl GeneratedItem {
    /// A stable content hash over the item's answerable content.
    ///
    /// Deliberately excludes `seed` and `difficulty`: two items that ask the
    /// same question with the same options are the same item for deduplication
    /// purposes even if they were reached from different seeds.
    pub fn content_hash(&self) -> String {
        let mut canonical = String::new();
        canonical.push_str(&self.subtest);
        canonical.push('\u{1}');
        canonical.push_str(&self.stem);
        canonical.push('\u{1}');
        for option in &self.options {
            canonical.push_str(option);
            canonical.push('\u{2}');
        }
        canonical.push('\u{1}');
        canonical.push_str(&self.correct_index.to_string());
        crate::provenance::ContentHash::of_text(&canonical)
            .as_str()
            .to_string()
    }
}

/// Why a generated item is not acceptable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VerificationFailure {
    /// The proof expression did not evaluate to the declared answer.
    ProofRejected(String),
    /// Fewer than two options, so it is not a question.
    TooFewOptions(usize),
    /// `correct_index` is outside the option list.
    CorrectIndexOutOfRange { index: usize, options: usize },
    /// The option at `correct_index` is not the proven answer.
    CorrectOptionDisagreesWithProof { option: String, answer: String },
    /// Two options are the same, so the item has no unique answer.
    DuplicateOption(String),
    /// A wrong option is numerically equal to the correct answer.
    DistractorEqualsAnswer { index: usize, value: String },
    /// Rationales do not cover exactly the wrong options.
    RationaleCoverage {
        missing: Vec<usize>,
        extra: Vec<usize>,
    },
    /// The stem or an option is blank.
    Blank(String),
}

impl std::fmt::Display for VerificationFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VerificationFailure::ProofRejected(reason) => {
                write!(f, "proof rejected: {reason}")
            }
            VerificationFailure::TooFewOptions(n) => {
                write!(f, "an item needs at least 2 options, found {n}")
            }
            VerificationFailure::CorrectIndexOutOfRange { index, options } => {
                write!(f, "correct_index {index} is outside {options} options")
            }
            VerificationFailure::CorrectOptionDisagreesWithProof { option, answer } => write!(
                f,
                "the option at correct_index is {option:?} but the proof gives {answer:?}"
            ),
            VerificationFailure::DuplicateOption(option) => {
                write!(f, "option {option:?} appears more than once")
            }
            VerificationFailure::DistractorEqualsAnswer { index, value } => write!(
                f,
                "distractor at index {index} is {value:?}, equal to the answer"
            ),
            VerificationFailure::RationaleCoverage { missing, extra } => write!(
                f,
                "rationales missing for {missing:?} and unexpected for {extra:?}"
            ),
            VerificationFailure::Blank(what) => write!(f, "{what} is blank"),
        }
    }
}

impl std::error::Error for VerificationFailure {}

/// A candidate item as a template produces it, before option assembly.
struct Candidate {
    stem: String,
    expression: String,
    correct: i64,
    /// Wrong answers paired with the misconception each one represents.
    distractors: Vec<(i64, String)>,
    difficulty: f64,
}

struct Template {
    id: &'static str,
    subtest: &'static str,
    objective_id: &'static str,
    build: fn(&mut Rng) -> Option<Candidate>,
}

/// One misconception: a wrong value and why a learner arrives at it.
///
/// `i64` rather than `i128`: template parameters are drawn from small ranges, so
/// the largest intermediate value here is a few thousand. The proof evaluator
/// still works in `i128` because it accepts expression text from anywhere.
fn misconception(value: i64, why: &str) -> (i64, String) {
    (value, why.to_string())
}

/// Every maximal run of digits in `text`, as decimal strings.
///
/// Used to refuse a wrong option that is literally a number the stem prints: such
/// an option can be copied out of the question without any arithmetic, so it stops
/// measuring the subtest's skill. Run-based rather than token-based so it matches
/// how the numbers are actually written (`75` in "75%", `300` in "of 300").
fn numbers_printed_in(text: &str) -> std::collections::HashSet<String> {
    let mut out = std::collections::HashSet::new();
    let mut current = String::new();
    for ch in text.chars() {
        if ch.is_ascii_digit() {
            current.push(ch);
        } else if !current.is_empty() {
            out.insert(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        out.insert(current);
    }
    out
}

fn ar_rate_pages(rng: &mut Rng) -> Option<Candidate> {
    let rate = rng.range(6, 40);
    let hours = rng.range(2, 12);
    let correct = rate * hours * 60;
    Some(Candidate {
        stem: format!(
            "A printer produces {rate} pages per minute. How many pages does it \
             produce in {hours} hours?"
        ),
        expression: format!("{rate} * {hours} * 60"),
        correct,
        distractors: vec![
            misconception(
                rate * hours,
                "Multiplied by the number of hours without converting hours to minutes.",
            ),
            misconception(
                rate * 60,
                "Used one hour instead of the stated number of hours.",
            ),
            misconception(
                rate * hours * 100,
                "Treated each hour as 100 minutes instead of 60.",
            ),
        ],
        difficulty: -0.5,
    })
}

fn ar_percent_of(rng: &mut Rng) -> Option<Candidate> {
    // 10% is excluded because the "shifted the decimal one place too few"
    // distractor (`number * percent / 10`) collapses to the printed number itself
    // at that percentage — the wrong option becomes the whole, copyable straight
    // from the stem. Every other percentage keeps the distractor distinct.
    let percent = *rng.pick(&[5_i64, 15, 20, 25, 40, 50, 60, 75]);
    let number = rng.range(2, 40) * 20;
    if (number * percent) % 100 != 0 {
        return None;
    }
    let correct = number * percent / 100;
    Some(Candidate {
        stem: format!("What is {percent}% of {number}?"),
        expression: format!("({number} * {percent}) / 100"),
        correct,
        distractors: vec![
            misconception(
                number * percent / 10,
                "Shifted the decimal point one place too few.",
            ),
            // `number + percent` would put two numbers that are already printed
            // in the stem into one option, which a learner can reach by copying
            // rather than by computing. The misconception is real, so it is kept
            // as a *derived* sum instead: the whole plus the part.
            misconception(
                number + correct,
                "Added the part to the whole instead of taking a part of it.",
            ),
            misconception(
                number - correct,
                "Subtracted the part from the whole instead of reporting the part.",
            ),
        ],
        difficulty: -0.8,
    })
}

fn ar_unit_price(rng: &mut Rng) -> Option<Candidate> {
    let count = rng.range(2, 12);
    let unit = rng.range(3, 25);
    let wanted = count + rng.range(1, 9);
    let given_cost = count * unit;
    let correct = wanted * unit;
    Some(Candidate {
        stem: format!(
            "If {count} identical items cost {given_cost} dollars, how much do \
             {wanted} of the same items cost?"
        ),
        expression: format!("{given_cost} / {count} * {wanted}"),
        correct,
        distractors: vec![
            misconception(
                given_cost * wanted,
                "Multiplied by the new quantity without first finding the unit price.",
            ),
            // `given_cost + wanted` adds two numbers the stem already prints, so
            // the option can be copied out of the question. Keep the "added
            // instead of scaled" misconception but derive it: the cost of the
            // wanted quantity plus the given cost.
            misconception(
                correct + given_cost,
                "Added the two totals instead of scaling the unit price.",
            ),
            misconception(unit, "Reported the price of one item instead of the total."),
        ],
        difficulty: 0.0,
    })
}

fn ar_average_sum(rng: &mut Rng) -> Option<Candidate> {
    let count = rng.range(4, 9);
    let average = rng.range(12, 95);
    let correct = count * average;
    Some(Candidate {
        stem: format!(
            "The average of {count} numbers is {average}. What is the sum of the \
             numbers?"
        ),
        expression: format!("{count} * {average}"),
        correct,
        distractors: vec![
            // Every distractor must be a *derived* near-miss, never a number already
            // printed in the stem. Reporting `average` itself is the worst kind of
            // wrong option: it is the one value a learner can copy straight out of the
            // question without doing arithmetic, so the item tests reading rather than
            // averaging. `average / count` is also rejected: whenever the count divides
            // the average the option lands on the printed count, the same defect by a
            // different route. Both are replaced by sums and products of the answer,
            // which cannot coincide with a stem operand.
            misconception(
                correct - average,
                "Subtracted the average instead of adding it to the product.",
            ),
            misconception(
                count + average,
                "Added the count to the average instead of multiplying.",
            ),
            misconception(correct * 2, "Multiplied by twice the count."),
        ],
        difficulty: -0.3,
    })
}

fn ar_simple_interest(rng: &mut Rng) -> Option<Candidate> {
    let principal = rng.range(2, 50) * 100;
    let rate = *rng.pick(&[2_i64, 3, 4, 5, 6, 8]);
    let years = rng.range(2, 7);
    let correct = principal * rate * years / 100;
    // The "shifted the decimal" distractor must be exactly one tenth of the
    // answer. Computing it as `principal * rate * years / 1000` would truncate
    // whenever the numerator is not a multiple of 1000, and the option would then
    // not be the value its own rationale describes.
    if correct % 10 != 0 {
        return None;
    }
    Some(Candidate {
        stem: format!(
            "A savings account holds {principal} dollars at {rate}% simple \
             interest per year. How much interest is earned in {years} years?"
        ),
        expression: format!("({principal} * {rate} * {years}) / 100"),
        correct,
        distractors: vec![
            misconception(
                principal * rate / 100,
                "Computed one year of interest instead of the full term.",
            ),
            misconception(correct / 10, "Shifted the decimal point one place too far."),
            misconception(
                principal + correct,
                "Reported the balance including the principal rather than the interest.",
            ),
        ],
        difficulty: 0.4,
    })
}

fn ar_rectangle_perimeter(rng: &mut Rng) -> Option<Candidate> {
    let length = rng.range(5, 40);
    let width = rng.range(3, length - 1);
    let correct = 2 * (length + width);
    Some(Candidate {
        stem: format!(
            "A rectangle is {length} centimetres long and {width} centimetres \
             wide. What is its perimeter?"
        ),
        expression: format!("2 * ({length} + {width})"),
        correct,
        distractors: vec![
            misconception(
                length * width,
                "Computed the area instead of the perimeter.",
            ),
            misconception(length + width, "Found half the perimeter."),
            misconception(2 * length + width, "Doubled only one of the two sides."),
        ],
        difficulty: -1.0,
    })
}

fn ar_triangle_area(rng: &mut Rng) -> Option<Candidate> {
    // Both even, so the product is divisible by 4 and the "halved twice"
    // misconception is an integer rather than a fraction.
    let base = rng.range(3, 20) * 2;
    let height = rng.range(3, 20) * 2;
    let correct = base * height / 2;
    Some(Candidate {
        stem: format!(
            "A triangle has a base of {base} metres and a height of {height} \
             metres. What is its area in square metres?"
        ),
        expression: format!("({base} * {height}) / 2"),
        correct,
        distractors: vec![
            misconception(base * height, "Forgot to halve the product."),
            misconception(
                base + height,
                "Added the dimensions instead of multiplying them.",
            ),
            misconception(base * height / 4, "Halved the product twice."),
        ],
        difficulty: -0.6,
    })
}

fn ar_work_rate(rng: &mut Rng) -> Option<Candidate> {
    // a*b/(a+b) is the combined time; it is only an integer for some pairs, and
    // a fractional answer would not survive an exact-integer proof.
    let first = rng.range(2, 12);
    let second = rng.range(2, 12);
    if first == second {
        return None;
    }
    let numerator = first * second;
    let denominator = first + second;
    if numerator % denominator != 0 {
        return None;
    }
    let correct = numerator / denominator;
    if correct == 0 {
        return None;
    }
    // "Averaged the two times" is only an integer when the sum is even; an odd
    // sum would truncate and the option would not be the average it claims.
    if denominator % 2 != 0 {
        return None;
    }
    Some(Candidate {
        stem: format!(
            "One worker can finish a job in {first} days and another can finish \
             the same job in {second} days. Working together at those rates, how \
             many days do they take?"
        ),
        expression: format!("({first} * {second}) / ({first} + {second})"),
        correct,
        distractors: vec![
            misconception(
                first + second,
                "Added the two times instead of combining rates.",
            ),
            misconception(
                denominator / 2,
                "Averaged the two times rather than combining the rates.",
            ),
            misconception(
                numerator,
                "Multiplied the times instead of combining rates.",
            ),
        ],
        difficulty: 1.4,
    })
}

// ---------------------------------------------------------------------------
// AR expansion: the breadth the ticket asks for. Every one of these carries an
// executable expression proof, and every wrong option is a *derived* value -- a
// plausible arithmetic misstep -- rather than a number the stem prints. Where a
// misconception would collapse onto a printed number (or onto the answer) for a
// drawn parameter, the template steers the draw instead of weakening the guard in
// `assemble`, because that guard is the backstop for exactly this class of defect.
// ---------------------------------------------------------------------------

/// Distance equals rate times time. Includes the unit conversion the real subtest
/// uses, so "forgot to convert" is a live misconception and the answer is not a
/// restatement of the stem.
fn ar_distance_rate(rng: &mut Rng) -> Option<Candidate> {
    let speed = rng.range(30, 70);
    let hours = rng.range(2, 6);
    let extra_minutes = 30;
    let total_minutes = hours * 60 + extra_minutes;
    // Whole miles only: a fractional distance would not survive an exact-integer
    // proof, and the misconception must be a value a learner could actually pick.
    if (speed * total_minutes) % 60 != 0 {
        return None;
    }
    let correct = speed * total_minutes / 60;
    Some(Candidate {
        stem: format!(
            "A train travels at a constant {speed} miles per hour for {hours} hours \
             and {extra_minutes} minutes. How many miles does it travel?"
        ),
        expression: format!("{speed} * ({hours} * 60 + {extra_minutes}) / 60"),
        correct,
        distractors: vec![
            misconception(
                speed * hours,
                "Used only the whole hours and dropped the extra minutes.",
            ),
            misconception(
                speed * total_minutes,
                "Multiplied by the total time in minutes without converting back to hours.",
            ),
            misconception(
                speed * (hours + 1),
                "Rounded the extra minutes up to a full hour.",
            ),
        ],
        difficulty: 0.3,
    })
}

/// A percentage discount. The wrong options are the discount itself and two
/// mis-scaled discounts -- all derived, none printed in the stem.
fn ar_discount_price(rng: &mut Rng) -> Option<Candidate> {
    let price = rng.range(4, 40) * 10;
    // Only percentages whose discount is a whole number of dollars. The proof
    // re-evaluates the expression exactly, so `price * percent / 100` must divide
    // without truncation (15% of 150 is not a whole dollar); a draw that would
    // truncate is skipped rather than silently rounded.
    let percent = [15, 20, 25, 30, 40, 45][(rng.range(0, 5)) as usize];
    if (price * percent) % 100 != 0 {
        return None;
    }
    let discount = price * percent / 100;
    let correct = price - discount;
    if correct <= 0 {
        return None;
    }
    Some(Candidate {
        stem: format!(
            "A jacket is priced at {price} dollars and is on sale at {percent} percent \
             off. What is the sale price in dollars?"
        ),
        expression: format!("{price} - ({price} * {percent} / 100)"),
        correct,
        distractors: vec![
            misconception(discount, "Reported the discount instead of the sale price."),
            misconception(
                price - percent,
                "Subtracted the percentage as if it were a dollar amount.",
            ),
            misconception(
                price - discount / 2,
                "Halved the discount, giving too little off.",
            ),
        ],
        difficulty: -0.4,
    })
}

/// Unit conversion in the direction the real test asks: a large unit expressed in
/// a smaller one.
fn ar_unit_conversion(rng: &mut Rng) -> Option<Candidate> {
    let hours = rng.range(2, 9);
    let minutes = rng.range(2, 9) * 5;
    let correct = hours * 60 + minutes;
    Some(Candidate {
        stem: format!("How many minutes are there in {hours} hours and {minutes} minutes?"),
        expression: format!("{hours} * 60 + {minutes}"),
        correct,
        distractors: vec![
            misconception(hours * 100 + minutes, "Treated each hour as 100 minutes."),
            misconception(
                hours * 60 - minutes,
                "Subtracted the extra minutes instead of adding them.",
            ),
            misconception(hours + minutes, "Added the numbers as written."),
        ],
        difficulty: -1.1,
    })
}

/// Multiplication in a rate context. The stem prints only two numbers, so the
/// obvious distractors are derived misfactors rather than copies.
fn ar_production_rate(rng: &mut Rng) -> Option<Candidate> {
    let machines = rng.range(3, 12);
    let hours = rng.range(2, 9);
    let per_machine_per_hour = rng.range(4, 15);
    let correct = machines * per_machine_per_hour * hours;
    Some(Candidate {
        stem: format!(
            "A factory has {machines} machines. Each machine produces \
             {per_machine_per_hour} parts per hour. How many parts do all the \
             machines produce in {hours} hours?"
        ),
        expression: format!("{machines} * {per_machine_per_hour} * {hours}"),
        correct,
        distractors: vec![
            misconception(
                per_machine_per_hour * hours,
                "Counted only one machine's output.",
            ),
            misconception(
                machines * per_machine_per_hour,
                "Counted one hour instead of the stated number of hours.",
            ),
            misconception(
                machines * per_machine_per_hour * hours / 2,
                "Halved the total, as if only half the machines ran.",
            ),
        ],
        difficulty: -0.2,
    })
}

/// A two-step proportion: work out the unit rate, then scale it. Both steps are in
/// the expression, so the proof checks the whole chain rather than one product.
fn ar_batch_proportion(rng: &mut Rng) -> Option<Candidate> {
    let per_batch = rng.range(3, 12);
    let batches = rng.range(3, 9);
    let wanted = rng.range(2, 8);
    if (per_batch * wanted) % batches != 0 {
        return None;
    }
    let correct = per_batch * wanted / batches;
    // All four options must be positive: `per_batch / batches` is the "found the
    // unit rate and stopped" misconception, and it is only a real litre figure if
    // the division leaves at least one litre. `per_batch < batches` truncates it to
    // zero, which no learner would choose.
    if correct == 0 || per_batch / batches == 0 {
        return None;
    }
    Some(Candidate {
        stem: format!(
            "{batches} batches of a mixture require {per_batch} litres of solvent. \
             How many litres do {wanted} batches require at the same rate?"
        ),
        expression: format!("{per_batch} * {wanted} / {batches}"),
        correct,
        distractors: vec![
            misconception(
                per_batch / batches,
                "Found the solvent per batch and stopped there.",
            ),
            misconception(
                per_batch + wanted,
                "Added the batch counts instead of scaling the rate.",
            ),
            misconception(
                per_batch * batches / wanted,
                "Inverted the ratio, scaling in the wrong direction.",
            ),
        ],
        difficulty: 0.6,
    })
}

/// A percentage of an amount. `ar_percent_of` covers the direct form; this is the
/// inverse wording a learner meets on a word problem, and the wrong options map to
/// the two classic inverse mistakes.
fn ar_percent_remaining(rng: &mut Rng) -> Option<Candidate> {
    let total = rng.range(4, 40) * 5;
    let percent = [10, 20, 25, 40, 50, 60][(rng.range(0, 5)) as usize];
    // The proof re-evaluates the expression exactly, so the division must be whole.
    // 25% and 40% of a multiple of 5 leave a remainder at some draws (8250 * 25 is
    // not a multiple of 100), so the draw is skipped rather than truncated.
    if (total * percent) % 100 != 0 {
        return None;
    }
    let part = total * percent / 100;
    let correct = total - part;
    // Every option must be a plausible crate count. `total - percent` is the
    // "subtracted the percentage as a number" misconception, and it is only a real
    // answer if it is positive; a 25-crate shipment with 60% off would make it -5,
    // which is not something a learner would pick.
    if correct <= 0 || total - percent <= 0 {
        return None;
    }
    Some(Candidate {
        stem: format!(
            "{percent} percent of a shipment of {total} crates is damaged. How many \
             crates are not damaged?"
        ),
        expression: format!("{total} - ({total} * {percent} / 100)"),
        correct,
        distractors: vec![
            misconception(part, "Reported the damaged crates instead of the rest."),
            misconception(
                total - percent,
                "Subtracted the percentage as if it were a crate count.",
            ),
            misconception(
                total + part,
                "Added the damaged share to the shipment instead of removing it.",
            ),
        ],
        difficulty: -0.3,
    })
}

fn mk_linear_solve(rng: &mut Rng) -> Option<Candidate> {
    let coefficient = rng.range(2, 9);
    let solution = rng.range(2, 12);
    // The constant is a multiple of the coefficient so that "divided without
    // subtracting first" is an exact integer. Choosing them independently made
    // that distractor a truncated value that no longer matched its rationale.
    let constant = coefficient * rng.range(1, 4);
    let total = coefficient * solution + constant;
    Some(Candidate {
        stem: format!("If {coefficient}x + {constant} = {total}, what is the value of x?"),
        expression: format!("({total} - {constant}) / {coefficient}"),
        correct: solution,
        distractors: vec![
            misconception(
                total - constant,
                "Found the value of the term containing x rather than x itself.",
            ),
            misconception(
                total / coefficient,
                "Divided by the coefficient before subtracting the constant.",
            ),
            misconception(
                total - constant - coefficient,
                "Subtracted the coefficient as well as the constant.",
            ),
        ],
        difficulty: 0.2,
    })
}

fn mk_evaluate_expression(rng: &mut Rng) -> Option<Candidate> {
    let coefficient = rng.range(2, 9);
    let mut x = rng.range(2, 10);
    if x == coefficient {
        // "9x" evaluated at x = 9 reads like a misprint rather than a question.
        // x is at most 9 here, because the coefficient is at most 9.
        x += 1;
    }
    let constant = rng.range(1, 15);
    let correct = coefficient * x + constant;
    Some(Candidate {
        stem: format!("If x = {x}, what is the value of {coefficient}x + {constant}?"),
        expression: format!("{coefficient} * {x} + {constant}"),
        correct,
        distractors: vec![
            misconception(
                coefficient * (x + constant),
                "Added the constant to x before multiplying.",
            ),
            misconception(coefficient * x, "Ignored the constant term."),
            misconception(
                coefficient + x + constant,
                "Added the terms instead of multiplying.",
            ),
        ],
        difficulty: -0.4,
    })
}

fn mk_slope(rng: &mut Rng) -> Option<Candidate> {
    let slope = rng.range(1, 5);
    let run = rng.range(2, 6);
    let rise = slope * run;
    let x1 = rng.range(1, 9);
    let y1 = rng.range(1, 9);
    let (x2, y2) = (x1 + run, y1 + rise);
    Some(Candidate {
        stem: format!("A line passes through ({x1}, {y1}) and ({x2}, {y2}). What is its slope?"),
        expression: format!("({y2} - {y1}) / ({x2} - {x1})"),
        correct: slope,
        distractors: vec![
            // The raw rise and run are readable off the two printed points, so they
            // are replaced by proportions of them. The ratio forms keep the same
            // misconceptions (inverted slope, used one axis alone) without being a
            // number the stem already shows. Each is filtered by `assemble` if it
            // collides with the answer, and a different seed retries.
            misconception(
                rise + run,
                "Added the two changes together instead of dividing them.",
            ),
            misconception(
                slope * run,
                "Multiplied the slope by the horizontal change.",
            ),
            misconception(slope + run, "Added the horizontal change to the slope."),
        ],
        difficulty: 0.6,
    })
}

fn mk_exponent_power(rng: &mut Rng) -> Option<Candidate> {
    let base = *rng.pick(&[2_i64, 3, 5]);
    let inner = rng.range(2, 4);
    let outer = rng.range(2, 4);
    let correct = inner * outer;
    let mut raised = 1_i64;
    for _ in 0..outer {
        raised *= inner;
    }
    let mut reversed = 1_i64;
    for _ in 0..inner {
        reversed *= outer;
    }
    Some(Candidate {
        stem: format!(
            "The expression ({base}^{inner})^{outer} can be written as {base} raised \
             to what power?"
        ),
        expression: format!("{inner} * {outer}"),
        correct,
        distractors: vec![
            misconception(
                inner + outer,
                "Added the exponents instead of multiplying them.",
            ),
            misconception(
                raised,
                "Raised one exponent to the other instead of multiplying.",
            ),
            misconception(reversed, "Raised the outer exponent to the inner one."),
        ],
        difficulty: 0.8,
    })
}

fn mk_volume_box(rng: &mut Rng) -> Option<Candidate> {
    let length = rng.range(2, 12);
    let width = rng.range(2, 12);
    let height = rng.range(2, 12);
    let correct = length * width * height;
    Some(Candidate {
        stem: format!(
            "A rectangular box measures {length} by {width} by {height} \
             centimetres. What is its volume in cubic centimetres?"
        ),
        expression: format!("{length} * {width} * {height}"),
        correct,
        distractors: vec![
            misconception(
                length * width + height,
                "Added the third dimension instead of multiplying.",
            ),
            // Total edge length is 4(l + w + h); the previous 2(l + w + h) was
            // neither the edge sum nor anything else a learner would compute.
            misconception(
                4 * (length + width + height),
                "Summed the twelve edges instead of multiplying the dimensions.",
            ),
            misconception(length + width + height, "Added all three dimensions."),
        ],
        difficulty: -0.7,
    })
}

fn mk_fraction_of(rng: &mut Rng) -> Option<Candidate> {
    let denominator = *rng.pick(&[2_i64, 3, 4, 5, 8]);
    let numerator = rng.range(1, denominator - 1);
    let total = denominator * rng.range(4, 40);
    let correct = total * numerator / denominator;
    // "Inverted the fraction" is `total * denominator / numerator`; it is only
    // an integer when numerator divides the product, so reject otherwise rather
    // than truncating and mislabelling the result.
    let inverted_numerator = total * denominator;
    if inverted_numerator % numerator != 0 {
        return None;
    }
    Some(Candidate {
        stem: format!("What is {numerator}/{denominator} of {total}?"),
        expression: format!("({total} * {numerator}) / {denominator}"),
        correct,
        distractors: vec![
            misconception(
                total / denominator,
                "Found one part rather than the stated number of parts.",
            ),
            misconception(inverted_numerator / numerator, "Inverted the fraction."),
            misconception(total - correct, "Subtracted the part from the whole."),
        ],
        difficulty: -0.2,
    })
}

// ---------------------------------------------------------------------------
// Mechanical Comprehension
// ---------------------------------------------------------------------------
//
// Mechanical Comprehension asks what a machine does with the force put into it:
// what effort balances a load on a lever, how many turns a gear train gives back,
// what pressure a piston sees. Every one of those is arithmetic over the machine's
// own geometry, so the answers are computable and the proof is executable in the
// same sense an Arithmetic Reasoning proof is -- which is what lets these items be
// generated rather than quoted, and what the schema requires of an `AR` or `MK`
// item but not, on its own, of an `MC` one.
//
// The figures are chosen so every answer is a whole number and every distractor is
// a whole number: a learner should be wrong because they inverted a ratio, not
// because of a rounding convention.

/// The effort that balances a load on a lever.
///
/// `load * load_arm = effort * effort_arm`, so the effort is the load scaled by the
/// ratio of the arms.
fn mc_lever_effort(rng: &mut Rng) -> Option<Candidate> {
    let load_arm = rng.range(1, 6);
    let effort_arm = load_arm * rng.range(2, 5);
    let load = rng.range(2, 25) * effort_arm;
    let correct = load * load_arm / effort_arm;
    let short_arm = load_arm + 1;
    Some(Candidate {
        stem: format!(
            "A lever is used to lift a load of {load} pounds. The load is {load_arm} \
             feet from the fulcrum and the effort is applied {effort_arm} feet from \
             it. How many pounds of effort are needed to balance the load?"
        ),
        expression: format!("({load} * {load_arm}) / {effort_arm}"),
        correct,
        distractors: vec![
            misconception(
                load * effort_arm / load_arm,
                "Multiplied by the effort arm instead of dividing by it.",
            ),
            misconception(
                load,
                "Ignored the arms and assumed the effort equals the load.",
            ),
            misconception(
                load * load_arm / short_arm,
                "Used the wrong arm on one side of the balance.",
            ),
        ],
        difficulty: -0.2,
    })
}

/// How many turns a driven gear makes for a given number of turns of the driver.
///
/// A gear with more teeth turns fewer times, in the ratio of the tooth counts.
fn mc_gear_ratio(rng: &mut Rng) -> Option<Candidate> {
    let driving_teeth = rng.range(2, 12);
    let driven_teeth = driving_teeth * rng.range(2, 4);
    let turns = rng.range(2, 9) * driven_teeth;
    let correct = driving_teeth * turns / driven_teeth;
    Some(Candidate {
        stem: format!(
            "A gear with {driving_teeth} teeth drives a gear with {driven_teeth} \
             teeth. If the driving gear makes {turns} turns, how many turns does \
             the driven gear make?"
        ),
        expression: format!("({driving_teeth} * {turns}) / {driven_teeth}"),
        correct,
        distractors: vec![
            misconception(
                driven_teeth * turns / driving_teeth,
                "Inverted the gear ratio.",
            ),
            misconception(turns, "Assumed both gears turn at the same speed."),
            misconception(
                driving_teeth + turns,
                "Added the tooth count instead of using the ratio.",
            ),
        ],
        difficulty: -0.1,
    })
}

/// The effort a block and tackle needs.
///
/// Each supporting strand carries an equal share of the load, so the effort is the
/// load divided by the number of strands.
fn mc_pulley_effort(rng: &mut Rng) -> Option<Candidate> {
    let strands = rng.range(2, 6);
    let load = rng.range(2, 30) * strands;
    let correct = load / strands;
    Some(Candidate {
        stem: format!(
            "A block and tackle has {strands} supporting strands. Ignoring friction \
             and the weight of the blocks, how many pounds of effort are needed to \
             lift a load of {load} pounds?"
        ),
        expression: format!("{load} / {strands}"),
        correct,
        distractors: vec![
            misconception(
                load * strands,
                "Multiplied by the number of strands instead of dividing.",
            ),
            misconception(
                load - strands,
                "Subtracted the number of strands from the load.",
            ),
            misconception(load / (strands - 1), "Counted one strand too few."),
        ],
        difficulty: -0.4,
    })
}

/// The effort at the rim of a wheel that balances a load on its axle.
fn mc_wheel_and_axle(rng: &mut Rng) -> Option<Candidate> {
    let axle_radius = rng.range(1, 4);
    let wheel_radius = axle_radius * rng.range(3, 6);
    let load = rng.range(2, 20) * wheel_radius;
    let correct = load * axle_radius / wheel_radius;
    Some(Candidate {
        stem: format!(
            "A wheel with a radius of {wheel_radius} inches is fastened to an axle \
             with a radius of {axle_radius} inches. How many pounds of effort at the \
             rim of the wheel will balance a load of {load} pounds on the axle?"
        ),
        expression: format!("({load} * {axle_radius}) / {wheel_radius}"),
        correct,
        distractors: vec![
            misconception(
                load * wheel_radius / axle_radius,
                "Inverted the ratio of the radii.",
            ),
            misconception(
                load,
                "Assumed the effort equals the load whatever the radii are.",
            ),
            misconception(
                load / (wheel_radius - axle_radius),
                "Used the difference of the radii as a ratio.",
            ),
        ],
        difficulty: -0.3,
    })
}

/// The pressure a force produces on a piston.
fn mc_pressure(rng: &mut Rng) -> Option<Candidate> {
    let area = rng.range(2, 12);
    let pressure = rng.range(3, 40);
    let force = area * pressure;
    let correct = force / area;
    Some(Candidate {
        stem: format!(
            "A force of {force} pounds acts on a piston with an area of {area} square \
             inches. How many pounds per square inch of pressure does the piston \
             exert?"
        ),
        expression: format!("{force} / {area}"),
        correct,
        distractors: vec![
            misconception(
                force * area,
                "Multiplied by the area instead of dividing by it.",
            ),
            misconception(area, "Reported the area rather than the pressure."),
            misconception(force - area, "Subtracted the area from the force."),
        ],
        difficulty: -0.3,
    })
}

/// The load a hydraulic jack can lift: pressure is the same throughout the fluid.
fn mc_hydraulic_lift(rng: &mut Rng) -> Option<Candidate> {
    let small_area = rng.range(1, 5);
    let large_area = small_area * rng.range(3, 8);
    let effort = rng.range(2, 20) * large_area / small_area;
    let correct = effort * large_area / small_area;
    Some(Candidate {
        stem: format!(
            "In a hydraulic jack, {effort} pounds of effort is applied to a piston \
             with an area of {small_area} square inches. The fluid acts on a second \
             piston with an area of {large_area} square inches. How many pounds can \
             the second piston lift?"
        ),
        expression: format!("({effort} * {large_area}) / {small_area}"),
        correct,
        distractors: vec![
            misconception(
                effort * small_area / large_area,
                "Inverted the ratio of the piston areas.",
            ),
            misconception(effort, "Assumed the fluid transmits the force unchanged."),
            misconception(
                effort + large_area,
                "Added the area instead of using the ratio.",
            ),
        ],
        difficulty: -0.1,
    })
}

/// The effort needed to push a load up a frictionless ramp.
fn mc_inclined_plane(rng: &mut Rng) -> Option<Candidate> {
    let height = rng.range(1, 5);
    let length = height * rng.range(2, 6);
    let weight = rng.range(2, 25) * length;
    let correct = weight * height / length;
    // `1 feet` reads as a program talking rather than a person asking.
    let rise = if height == 1 {
        "1 foot".to_string()
    } else {
        format!("{height} feet")
    };
    Some(Candidate {
        stem: format!(
            "A ramp {length} feet long rises {rise}. Ignoring friction, how \
             many pounds of effort are needed to push a {weight}-pound crate up the \
             ramp?"
        ),
        expression: format!("({weight} * {height}) / {length}"),
        correct,
        distractors: vec![
            misconception(
                weight * length / height,
                "Inverted the ratio of the ramp's rise to its length.",
            ),
            misconception(
                weight,
                "Assumed the effort equals the weight whatever the slope is.",
            ),
            misconception(
                weight / (length - height),
                "Used the difference between the length and the rise as a ratio.",
            ),
        ],
        difficulty: -0.2,
    })
}

/// The mechanical advantage of a lever, as a pure ratio.
fn mc_mechanical_advantage(rng: &mut Rng) -> Option<Candidate> {
    let load_arm = rng.range(1, 8);
    let advantage = rng.range(2, 9);
    let effort_arm = load_arm * advantage;
    Some(Candidate {
        stem: format!(
            "The effort arm of a lever is {effort_arm} feet and its load arm is \
             {load_arm} feet. What is the mechanical advantage of the lever?"
        ),
        expression: format!("{effort_arm} / {load_arm}"),
        correct: advantage,
        distractors: vec![
            misconception(
                load_arm,
                "Reported the load arm instead of the ratio of the arms.",
            ),
            misconception(
                effort_arm + load_arm,
                "Added the arms instead of dividing them.",
            ),
            misconception(
                advantage + 1,
                "Counted one extra multiplication of the effort.",
            ),
        ],
        difficulty: -0.5,
    })
}

const TEMPLATES: &[Template] = &[
    Template {
        id: "ar.rate_pages",
        subtest: "AR",
        objective_id: "OBJ-AR-RATE-01",
        build: ar_rate_pages,
    },
    Template {
        id: "ar.percent_of",
        subtest: "AR",
        objective_id: "OBJ-AR-PERCENT-01",
        build: ar_percent_of,
    },
    Template {
        id: "ar.unit_price",
        subtest: "AR",
        objective_id: "OBJ-AR-PROPORTION-01",
        build: ar_unit_price,
    },
    Template {
        id: "ar.average_sum",
        subtest: "AR",
        objective_id: "OBJ-AR-AVERAGE-01",
        build: ar_average_sum,
    },
    Template {
        id: "ar.simple_interest",
        subtest: "AR",
        objective_id: "OBJ-AR-INTEREST-01",
        build: ar_simple_interest,
    },
    Template {
        id: "ar.rectangle_perimeter",
        subtest: "AR",
        objective_id: "OBJ-AR-GEOMETRY-01",
        build: ar_rectangle_perimeter,
    },
    Template {
        id: "ar.triangle_area",
        subtest: "AR",
        objective_id: "OBJ-AR-GEOMETRY-02",
        build: ar_triangle_area,
    },
    Template {
        id: "ar.work_rate",
        subtest: "AR",
        objective_id: "OBJ-AR-RATE-02",
        build: ar_work_rate,
    },
    Template {
        id: "ar.distance_rate",
        subtest: "AR",
        objective_id: "OBJ-AR-RATE-03",
        build: ar_distance_rate,
    },
    Template {
        id: "ar.discount_price",
        subtest: "AR",
        objective_id: "OBJ-AR-PERCENT-02",
        build: ar_discount_price,
    },
    Template {
        id: "ar.unit_conversion",
        subtest: "AR",
        objective_id: "OBJ-AR-MEASURE-01",
        build: ar_unit_conversion,
    },
    Template {
        id: "ar.production_rate",
        subtest: "AR",
        objective_id: "OBJ-AR-RATE-04",
        build: ar_production_rate,
    },
    Template {
        id: "ar.batch_proportion",
        subtest: "AR",
        objective_id: "OBJ-AR-PROPORTION-02",
        build: ar_batch_proportion,
    },
    Template {
        id: "ar.percent_remaining",
        subtest: "AR",
        objective_id: "OBJ-AR-PERCENT-03",
        build: ar_percent_remaining,
    },
    Template {
        id: "mk.linear_solve",
        subtest: "MK",
        objective_id: "OBJ-MK-ALGEBRA-01",
        build: mk_linear_solve,
    },
    Template {
        id: "mk.evaluate_expression",
        subtest: "MK",
        objective_id: "OBJ-MK-ALGEBRA-02",
        build: mk_evaluate_expression,
    },
    Template {
        id: "mk.slope",
        subtest: "MK",
        objective_id: "OBJ-MK-COORDINATE-01",
        build: mk_slope,
    },
    Template {
        id: "mk.exponent_power",
        subtest: "MK",
        objective_id: "OBJ-MK-EXPONENT-01",
        build: mk_exponent_power,
    },
    Template {
        id: "mk.volume_box",
        subtest: "MK",
        objective_id: "OBJ-MK-GEOMETRY-01",
        build: mk_volume_box,
    },
    Template {
        id: "mk.fraction_of",
        subtest: "MK",
        objective_id: "OBJ-MK-FRACTION-01",
        build: mk_fraction_of,
    },
    // Mechanical Comprehension. Computable from the machine's own geometry, so
    // every one of these carries an executable proof.
    Template {
        id: "mc.lever_effort",
        subtest: "MC",
        objective_id: "OBJ-MC-LEVER-01",
        build: mc_lever_effort,
    },
    Template {
        id: "mc.gear_ratio",
        subtest: "MC",
        objective_id: "OBJ-MC-GEARS-01",
        build: mc_gear_ratio,
    },
    Template {
        id: "mc.pulley_effort",
        subtest: "MC",
        objective_id: "OBJ-MC-PULLEY-01",
        build: mc_pulley_effort,
    },
    Template {
        id: "mc.wheel_and_axle",
        subtest: "MC",
        objective_id: "OBJ-MC-WHEEL-01",
        build: mc_wheel_and_axle,
    },
    Template {
        id: "mc.pressure",
        subtest: "MC",
        objective_id: "OBJ-MC-FLUID-01",
        build: mc_pressure,
    },
    Template {
        id: "mc.hydraulic_lift",
        subtest: "MC",
        objective_id: "OBJ-MC-FLUID-02",
        build: mc_hydraulic_lift,
    },
    Template {
        id: "mc.inclined_plane",
        subtest: "MC",
        objective_id: "OBJ-MC-PLANE-01",
        build: mc_inclined_plane,
    },
    Template {
        id: "mc.mechanical_advantage",
        subtest: "MC",
        objective_id: "OBJ-MC-ADVANTAGE-01",
        build: mc_mechanical_advantage,
    },
];

/// Templates available for a subtest.
pub fn templates_for(subtest: &str) -> Vec<&'static str> {
    TEMPLATES
        .iter()
        .filter(|t| t.subtest == subtest)
        .map(|t| t.id)
        .collect()
}

/// Assemble a candidate into an item, or reject it.
///
/// Rejection is normal and expected: a template can draw parameters whose
/// distractors collide with the answer or with each other, and an item with a
/// duplicate option has no unique answer. The caller retries with a new seed.
fn assemble(
    template: &Template,
    candidate: Candidate,
    mut rng: Rng,
    seed: u64,
) -> Option<GeneratedItem> {
    let mut entries: Vec<(i64, Option<String>)> = Vec::new();
    entries.push((candidate.correct, None));
    for (value, why) in candidate.distractors {
        // A distractor equal to the answer, or repeated, would make the item
        // ambiguous. Dropping it here keeps generation total rather than making
        // every template responsible for arithmetic collisions.
        if entries.iter().any(|(existing, _)| *existing == value) {
            continue;
        }
        entries.push((value, Some(why)));
    }
    if entries.len() != 4 {
        return None;
    }

    // A wrong option that is literally a number the stem already prints can be
    // copied out of the question without any arithmetic, so the item stops
    // measuring the subtest's skill. This applies to the arithmetic subtests (AR,
    // MK), where every answer is a computed value. It deliberately does not apply
    // to MC: in mechanical comprehension the naive answer is often *the* value the
    // stem supplies ("assumed the effort equals the load"), and removing that is
    // removing the misconception being tested.
    //
    // Some templates avoid this structurally (see `ar_average_sum`); this is the
    // backstop for the coincidental collisions arithmetic cannot rule out in
    // advance — e.g. "75% of 300" producing 300 - 225 = 75, the printed
    // percentage. Rejecting lets the caller retry with a new seed, exactly as a
    // collision with the answer already does.
    if matches!(template.subtest, "AR" | "MK") {
        let stem_numbers = numbers_printed_in(&candidate.stem);
        for (value, why) in &entries {
            if why.is_none() {
                continue;
            }
            if stem_numbers.contains(&value.to_string()) {
                return None;
            }
        }
    }

    rng.shuffle(&mut entries);

    let options: Vec<String> = entries.iter().map(|(v, _)| v.to_string()).collect();
    let correct_index = entries
        .iter()
        .position(|(_, why)| why.is_none())
        .expect("the correct entry is present and unlabelled");
    let mut distractor_rationales = BTreeMap::new();
    for (index, (_, why)) in entries.iter().enumerate() {
        if let Some(why) = why {
            distractor_rationales.insert(index, why.clone());
        }
    }

    Some(GeneratedItem {
        subtest: template.subtest.to_string(),
        objective_id: template.objective_id.to_string(),
        template_id: template.id,
        stem: candidate.stem,
        options,
        correct_index,
        expression: candidate.expression,
        answer: candidate.correct.to_string(),
        distractor_rationales,
        difficulty: candidate.difficulty,
        seed,
    })
}

/// Generate one item for a subtest, or `None` if every attempt was rejected.
///
/// Attempts derive new seeds from the original, so a seed that cannot produce an
/// item for one template can still succeed overall, and the whole call remains
/// reproducible.
pub fn generate_one(subtest: &str, seed: u64) -> Option<GeneratedItem> {
    let choices: Vec<&Template> = TEMPLATES.iter().filter(|t| t.subtest == subtest).collect();
    if choices.is_empty() {
        return None;
    }
    for attempt in 0..64_u64 {
        let attempt_seed = seed ^ attempt.wrapping_mul(0x9E37_79B9_7F4A_7C15);
        let mut rng = Rng::new(attempt_seed);
        let template = choices[(rng.next_u64() % choices.len() as u64) as usize];
        let mut build_rng = Rng::new(attempt_seed ^ 0xA5A5_5A5A_1234_5678);
        if let Some(candidate) = (template.build)(&mut build_rng) {
            let shuffle_rng = Rng::new(attempt_seed ^ 0x0F0F_F0F0_DEAD_BEEF);
            if let Some(item) = assemble(template, candidate, shuffle_rng, seed) {
                return Some(item);
            }
        }
    }
    None
}

/// Generate `count` items for a subtest from a base seed.
///
/// Two items are the same question -- and one is dropped -- when their stem and
/// answer agree, even if their options were shuffled differently. Deduplicating on
/// the content hash alone missed that: the hash covers the options, so the same
/// question with its wrong answers reordered hashed differently and both copies
/// were returned. The pipeline already refuses the second copy as padding
/// (`question_is_stored`), which is how a 60-item AR batch came back as 59 verified
/// -- the shortfall showed up downstream instead of here, where it belongs.
pub fn generate_many(subtest: &str, count: usize, base_seed: u64) -> Vec<GeneratedItem> {
    let mut out: Vec<GeneratedItem> = Vec::with_capacity(count);
    let mut seen = std::collections::HashSet::new();
    let mut seed = base_seed;
    let mut attempts = 0;
    while out.len() < count && attempts < count.saturating_mul(64).max(64) {
        attempts += 1;
        seed = seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
        if let Some(item) = generate_one(subtest, seed) {
            let question = (item.stem.clone(), item.answer.clone());
            if seen.insert(question) {
                out.push(item);
            }
        }
    }
    out
}

/// Independently verify a generated item.
///
/// The proof is re-derived from the expression text by [`crate::proof`]; this
/// function never consumes a value the generator computed, so a generator bug
/// cannot validate itself.
pub fn verify(item: &GeneratedItem) -> Result<(), VerificationFailure> {
    if item.stem.trim().is_empty() {
        return Err(VerificationFailure::Blank("stem".to_string()));
    }
    if item.options.len() < 2 {
        return Err(VerificationFailure::TooFewOptions(item.options.len()));
    }
    if item.correct_index >= item.options.len() {
        return Err(VerificationFailure::CorrectIndexOutOfRange {
            index: item.correct_index,
            options: item.options.len(),
        });
    }

    let recomputed = proof::verify_answer(&item.expression, &item.answer)
        .map_err(|e| VerificationFailure::ProofRejected(e.to_string()))?;

    let correct_option = &item.options[item.correct_index];
    let option_value = correct_option.trim().parse::<i128>().map_err(|_| {
        VerificationFailure::CorrectOptionDisagreesWithProof {
            option: correct_option.clone(),
            answer: recomputed.to_string(),
        }
    })?;
    if option_value != recomputed {
        return Err(VerificationFailure::CorrectOptionDisagreesWithProof {
            option: correct_option.clone(),
            answer: recomputed.to_string(),
        });
    }

    for (index, option) in item.options.iter().enumerate() {
        if option.trim().is_empty() {
            return Err(VerificationFailure::Blank(format!("option {index}")));
        }
        if item.options[..index]
            .iter()
            .any(|earlier| earlier == option)
        {
            return Err(VerificationFailure::DuplicateOption(option.clone()));
        }
        if index != item.correct_index {
            if let Ok(value) = option.trim().parse::<i128>() {
                if value == recomputed {
                    return Err(VerificationFailure::DistractorEqualsAnswer {
                        index,
                        value: option.clone(),
                    });
                }
            }
        }
    }

    let expected: Vec<usize> = (0..item.options.len())
        .filter(|i| *i != item.correct_index)
        .collect();
    let present: Vec<usize> = item.distractor_rationales.keys().copied().collect();
    let missing: Vec<usize> = expected
        .iter()
        .copied()
        .filter(|i| !present.contains(i))
        .collect();
    let extra: Vec<usize> = present
        .iter()
        .copied()
        .filter(|i| !expected.contains(i))
        .collect();
    if !missing.is_empty() || !extra.is_empty() {
        return Err(VerificationFailure::RationaleCoverage { missing, extra });
    }

    for (index, rationale) in &item.distractor_rationales {
        if rationale.trim().is_empty() {
            return Err(VerificationFailure::Blank(format!(
                "rationale for option {index}"
            )));
        }
    }

    Ok(())
}
