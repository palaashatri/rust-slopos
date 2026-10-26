//! Pocket-calculator arithmetic: immediate execution, left to right.
//!
//! `2 + 3 × 4 =` gives 20, like a desk calculator, not 14. Pressing `=` again
//! repeats the last operation.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operator {
    Add,
    Subtract,
    Multiply,
    Divide,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Input {
    Digit(u8),
    Point,
    Operator(Operator),
    Equals,
    Clear,
    Backspace,
    Negate,
    Percent,
}

/// Longest number the display accepts while typing.
const MAX_DIGITS: usize = 12;

#[derive(Debug, Clone)]
pub struct Calculator {
    display: String,
    accumulator: Option<f64>,
    pending: Option<Operator>,
    /// True while the user is typing a number into the display.
    entering: bool,
    /// True while the display holds a computed right operand (a percentage
    /// result, say) that the next operator must still consume.
    operand_ready: bool,
    /// The last `operator operand` pair, for repeated `=`.
    repeat: Option<(Operator, f64)>,
    error: bool,
}

impl Default for Calculator {
    fn default() -> Self {
        Self {
            display: "0".to_owned(),
            accumulator: None,
            pending: None,
            entering: false,
            operand_ready: false,
            repeat: None,
            error: false,
        }
    }
}

impl Calculator {
    pub fn display(&self) -> &str {
        &self.display
    }

    pub fn is_error(&self) -> bool {
        self.error
    }

    /// The operator waiting for its second operand, if any.
    pub fn pending(&self) -> Option<Operator> {
        self.pending
    }

    pub fn press(&mut self, input: Input) {
        if self.error && !matches!(input, Input::Digit(_) | Input::Point | Input::Clear) {
            return;
        }
        match input {
            Input::Digit(digit) => self.digit(digit),
            Input::Point => self.point(),
            Input::Operator(operator) => self.operator(operator),
            Input::Equals => self.equals(),
            Input::Clear => *self = Self::default(),
            Input::Backspace => self.backspace(),
            Input::Negate => self.negate(),
            Input::Percent => self.percent(),
        }
    }

    fn value(&self) -> f64 {
        self.display.parse().unwrap_or(0.0)
    }

    fn digit(&mut self, digit: u8) {
        if self.error {
            *self = Self::default();
        }
        let digit = char::from(b'0' + digit.min(9));
        if !self.entering {
            self.display = digit.to_string();
            self.entering = true;
            return;
        }
        let digits = self.display.chars().filter(char::is_ascii_digit).count();
        if digits >= MAX_DIGITS {
            return;
        }
        if self.display == "0" {
            self.display = digit.to_string();
        } else if self.display == "-0" {
            self.display = format!("-{digit}");
        } else {
            self.display.push(digit);
        }
    }

    fn point(&mut self) {
        if self.error {
            *self = Self::default();
        }
        if !self.entering {
            self.display = "0.".to_owned();
            self.entering = true;
        } else if !self.display.contains('.') {
            self.display.push('.');
        }
    }

    fn operator(&mut self, operator: Operator) {
        let value = self.value();
        match (self.accumulator, self.pending) {
            (Some(accumulator), Some(pending)) if self.entering || self.operand_ready => {
                if !self.show(apply(accumulator, pending, value)) {
                    return;
                }
                self.accumulator = Some(self.value());
            }
            (Some(_), Some(_)) => {} // Changing the operator: keep the left operand.
            _ => self.accumulator = Some(value),
        }
        self.pending = Some(operator);
        self.entering = false;
        self.operand_ready = false;
        self.repeat = None;
    }

    fn equals(&mut self) {
        let (left, operator, right) = match (self.accumulator, self.pending) {
            (Some(accumulator), Some(pending)) => (accumulator, pending, self.value()),
            _ => match self.repeat {
                Some((operator, operand)) => (self.value(), operator, operand),
                None => {
                    self.entering = false;
                    return;
                }
            },
        };
        if self.show(apply(left, operator, right)) {
            self.repeat = Some((operator, right));
        }
        self.accumulator = None;
        self.pending = None;
        self.entering = false;
        self.operand_ready = false;
    }

    fn backspace(&mut self) {
        if !self.entering {
            return;
        }
        self.display.pop();
        if self.display.is_empty() || self.display == "-" {
            self.display = "0".to_owned();
        }
    }

    fn negate(&mut self) {
        if self.display == "0" {
            return;
        }
        if let Some(rest) = self.display.strip_prefix('-') {
            self.display = rest.to_owned();
        } else {
            self.display.insert(0, '-');
        }
    }

    fn percent(&mut self) {
        let value = self.value();
        // "200 + 10 %" means 10 % of 200; on its own, % divides by 100.
        let result = match (self.accumulator, self.pending) {
            (Some(accumulator), Some(Operator::Add | Operator::Subtract)) => {
                accumulator * value / 100.0
            }
            _ => value / 100.0,
        };
        self.show(Some(result));
        self.entering = false;
        self.operand_ready = self.pending.is_some() && !self.error;
    }

    /// Put a result on the display; `None` or a non-finite value is an error.
    fn show(&mut self, result: Option<f64>) -> bool {
        match result.filter(|value| value.is_finite()) {
            Some(value) => {
                self.display = format_number(value);
                true
            }
            None => {
                self.display = "Error".to_owned();
                self.error = true;
                self.accumulator = None;
                self.pending = None;
                self.repeat = None;
                self.entering = false;
                self.operand_ready = false;
                false
            }
        }
    }
}

fn apply(left: f64, operator: Operator, right: f64) -> Option<f64> {
    match operator {
        Operator::Add => Some(left + right),
        Operator::Subtract => Some(left - right),
        Operator::Multiply => Some(left * right),
        Operator::Divide if right == 0.0 => None,
        Operator::Divide => Some(left / right),
    }
}

/// Format a result for a 12-digit display, hiding binary floating-point noise
/// such as 0.1 + 0.2 = 0.30000000000000004.
pub fn format_number(value: f64) -> String {
    let magnitude = value.abs();
    if magnitude != 0.0 && !(1e-9..1e12).contains(&magnitude) {
        let text = format!("{value:.6e}");
        // Trim zeros in the mantissa: 1.500000e13 → 1.5e13.
        return match text.split_once('e') {
            Some((mantissa, exponent)) => {
                let mantissa = mantissa.trim_end_matches('0').trim_end_matches('.');
                format!("{mantissa}e{exponent}")
            }
            None => text,
        };
    }
    let integer_digits = if magnitude < 1.0 {
        1
    } else {
        magnitude.log10().floor() as usize + 1
    };
    let decimals = MAX_DIGITS.saturating_sub(integer_digits).min(10);
    let text = format!("{value:.decimals$}");
    let text = if text.contains('.') {
        text.trim_end_matches('0').trim_end_matches('.').to_owned()
    } else {
        text
    };
    if text == "-0" {
        "0".to_owned()
    } else {
        text
    }
}

#[cfg(test)]
mod tests {
    use super::Operator::{Add, Divide, Multiply, Subtract};
    use super::*;
    use Input::*;

    fn run(inputs: &[Input]) -> Calculator {
        let mut calculator = Calculator::default();
        for input in inputs {
            calculator.press(*input);
        }
        calculator
    }

    #[test]
    fn typing_builds_numbers_without_leading_zeros() {
        assert_eq!(run(&[Digit(0), Digit(0), Digit(7)]).display(), "7");
        assert_eq!(run(&[Point, Digit(5)]).display(), "0.5");
        assert_eq!(run(&[Digit(1), Point, Point, Digit(2)]).display(), "1.2");
    }

    #[test]
    fn operations_execute_left_to_right() {
        let result = run(&[
            Digit(2),
            Operator(Add),
            Digit(3),
            Operator(Multiply),
            Digit(4),
            Equals,
        ]);
        assert_eq!(result.display(), "20");
    }

    #[test]
    fn floating_point_noise_is_hidden() {
        let result = run(&[Point, Digit(1), Operator(Add), Point, Digit(2), Equals]);
        assert_eq!(result.display(), "0.3");
        assert_eq!(
            run(&[Digit(1), Operator(Divide), Digit(3), Equals]).display(),
            "0.3333333333"
        );
    }

    #[test]
    fn repeated_equals_repeats_the_last_operation() {
        let result = run(&[Digit(5), Operator(Add), Digit(2), Equals, Equals, Equals]);
        assert_eq!(result.display(), "11");
    }

    #[test]
    fn changing_the_operator_keeps_the_left_operand() {
        let result = run(&[
            Digit(9),
            Operator(Add),
            Operator(Subtract),
            Digit(4),
            Equals,
        ]);
        assert_eq!(result.display(), "5");
    }

    #[test]
    fn division_by_zero_is_an_error_until_cleared() {
        let mut calculator = run(&[Digit(8), Operator(Divide), Digit(0), Equals]);
        assert!(calculator.is_error());
        assert_eq!(calculator.display(), "Error");
        calculator.press(Operator(Add));
        assert_eq!(calculator.display(), "Error");
        calculator.press(Digit(3));
        assert!(!calculator.is_error());
        assert_eq!(calculator.display(), "3");
    }

    #[test]
    fn negate_backspace_and_percent() {
        assert_eq!(run(&[Digit(4), Negate]).display(), "-4");
        assert_eq!(run(&[Digit(4), Negate, Negate]).display(), "4");
        assert_eq!(
            run(&[Digit(1), Digit(2), Digit(3), Backspace]).display(),
            "12"
        );
        assert_eq!(run(&[Digit(7), Backspace, Backspace]).display(), "0");
        assert_eq!(run(&[Digit(5), Digit(0), Percent]).display(), "0.5");
        let tip = run(&[
            Digit(2),
            Digit(0),
            Digit(0),
            Operator(Add),
            Digit(1),
            Digit(0),
            Percent,
            Equals,
        ]);
        assert_eq!(tip.display(), "220");
    }

    #[test]
    fn percent_results_survive_chaining() {
        let result = run(&[
            Digit(2),
            Digit(0),
            Digit(0),
            Operator(Add),
            Digit(1),
            Digit(0),
            Percent,
            Operator(Add),
            Digit(5),
            Equals,
        ]);
        assert_eq!(result.display(), "225");
        let result = run(&[
            Digit(2),
            Digit(0),
            Digit(0),
            Operator(Subtract),
            Digit(1),
            Digit(0),
            Percent,
            Operator(Subtract),
            Digit(5),
            Equals,
        ]);
        assert_eq!(result.display(), "175");
        // A negated percentage is still a supplied right operand.
        let result = run(&[
            Digit(2),
            Digit(0),
            Digit(0),
            Operator(Add),
            Digit(1),
            Digit(0),
            Percent,
            Negate,
            Operator(Add),
            Digit(5),
            Equals,
        ]);
        assert_eq!(result.display(), "185");
    }

    #[test]
    fn display_length_is_capped_and_large_results_use_exponents() {
        let mut calculator = Calculator::default();
        for _ in 0..20 {
            calculator.press(Digit(9));
        }
        assert_eq!(calculator.display().len(), MAX_DIGITS);
        assert_eq!(format_number(1.5e13), "1.5e13");
        assert_eq!(format_number(-0.0), "0");
        assert_eq!(format_number(123456.0), "123456");
    }
}
