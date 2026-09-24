mod helpers;

use helpers::run;
use rint::analysis::AnalysisErrorKind;
use rint::parser::ParseErrorKind;

use crate::helpers::{assert_analysis_error, assert_parse_error};

#[test]
fn function_nonvoid_param() {
    let source = r#"
    int encode(void x, int y) {
        return y * 2 + 9;
    }

    int main() {
        return encode(2, 7);
    }
    "#;

    assert_parse_error(source, ParseErrorKind::InvalidType);
}

#[test]
fn function_parameters_are_passed_in_order() {
    let source = r#"
    int encode(int tens, int ones) {
        return tens * 10 + ones;
    }

    int main() {
        return encode(2, 7);
    }
    "#;

    assert_eq!(run(source), 27);
}

#[test]
fn nested_function_calls() {
    let source = r#"
    int add(int x, int y) {
        return x + y;
    }

    int double_value(int x) {
        return add(x, x);
    }

    int main() {
        return double_value(6);
    }
    "#;

    assert_eq!(run(source), 12);
}

#[test]
fn caller_values_survive_a_call() {
    let source = r#"
    int add(int x, int y) {
        return x + y;
    }

    int main() {
        int first = 10;
        int second = 20;
        int result = add(2, 3);
        return first + second + result;
    }
    "#;

    assert_eq!(run(source), 35);
}

#[test]
fn stack_arguments_are_passed() {
    let source = r#"
    int sum_seven(int a, int b, int c, int d, int e, int f, int g) {
        return a + b + c + d + e + f + g;
    }

    int main() {
        return sum_seven(1, 2, 3, 4, 5, 6, 7);
    }
    "#;

    assert_eq!(run(source), 28);
}

#[test]
fn multiple_stack_arguments_keep_their_order() {
    let source = r#"
    int encode_eight(int a, int b, int c, int d, int e, int f, int g, int h) {
        return g * 10 + h;
    }

    int main() {
        return encode_eight(1, 2, 3, 4, 5, 6, 7, 8);
    }
    "#;

    assert_eq!(run(source), 78);
}

#[test]
fn many_parameters_can_spill() {
    let source = r#"
    int sum_fourteen(
        int a, int b, int c, int d, int e, int f, int g,
        int h, int i, int j, int k, int l, int m, int n
    ) {
        return a + b + c + d + e + f + g + h + i + j + k + l + m + n;
    }

    int main() {
        return sum_fourteen(1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14);
    }
    "#;

    assert_eq!(run(source), 105);
}

#[test]
fn register_pressure_across_a_call_spills_safely() {
    let source = r#"
    int identity(int x) {
        return x;
    }

    int main() {
        int a = 1;
        int b = 2;
        int c = 3;
        int d = 4;
        int e = 5;
        int f = 6;
        int g = 7;
        int h = 8;
        return identity(0) + a + b + c + d + e + f + g + h;
    }
    "#;

    assert_eq!(run(source), 36);
}

#[test]
fn recursive_function_calls() {
    let source = r#"
    int factorial(int n) {
        if (n == 0) {
            return 1;
        }
        return n * factorial(n - 1);
    }

    int main() {
        return factorial(5);
    }
    "#;

    assert_eq!(run(source), 120);
}

#[test]
fn calling_an_unknown_function_is_rejected() {
    let source = r#"
    int main() {
        return missing(1);
    }
    "#;

    assert_analysis_error(source, AnalysisErrorKind::FunctionNotFound);
}

#[test]
fn wrong_function_argument_count_is_rejected() {
    let source = r#"
    int add(int x, int y) {
        return x + y;
    }

    int main() {
        return add(1);
    }
    "#;

    assert_analysis_error(source, AnalysisErrorKind::FunctionArgsCountMismatch);
}
