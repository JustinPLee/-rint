mod helpers;

use helpers::run;

#[test]
fn addition() {
    let source = r#"
    int main() {
        int x = 2 + 2;
        return x;
    }
    "#;

    assert_eq!(run(source), 4);
}

#[test]
fn precedence() {
    let source = r#"
    int main() {
        int x = 2 + 3 * 4;
        return x;
    }
    "#;

    assert_eq!(run(source), 14);
}

#[test]
fn parentheses() {
    let source = r#"
    int main() {
        int x = (2 + 3) * 4;
        return x;
    }
    "#;

    assert_eq!(run(source), 20);
}

#[test]
fn unary_minus() {
    let source = r#"
    int main() {
        int x = -1 + 2;
        return x;
    }
    "#;

    assert_eq!(run(source), 1);
}

#[test]
fn long_expression() {
    let source = r#"
    int main() {
        int x = 1 + 2 + 3 + 4 + 5 + 6 + 7 + 8 + 9 + 10;
        return x;
    }
    "#;

    assert_eq!(run(source), 55);
}

#[test]
fn complex_expression() {
    let source = r#"
    int main() {
        int a = 2;
        int b = 3;
        int c = 4;
        int d = 5;
        int e = 6;
        int f = 7;
        int g = 8;
        int h = 9;

        int x = -a + b * c;
        int y = d * e + f;
        int z = g + h * 2;

        return x + y / z;
    }
    "#;

    assert_eq!(run(source), 11);
}

#[test]
fn comparison_equal() {
    let source = r#"
    int main() {
        int x = 5;

        if (x == 5) {
            return 1;
        } else {
            return 0;
        }
    }
    "#;

    assert_eq!(run(source), 1);
}

#[test]
fn ternary() {
    let source = r#"
    int main() {
        int x = 10;
        int y = 20;
        int z;

        z = x < y ? x : y;

        return z;
    }
    "#;

    assert_eq!(run(source), 10);
}

#[test]
fn ternary_in_if() {
    let source = r#"
    int main() {
        int x = 5;
        int y = 10;
        int z = 0;

        if (x < y) {
            z = x == 5 ? 100 : 200;
        } else {
            z = 300;
        }

        return z;
    }
    "#;

    assert_eq!(run(source), 100);
}

#[test]
fn ternary_evaluate_true() {
    let source = r#"
    int main() {
        int zero = 0;
        return true ? 7 : 1 / 0;
    }
    "#;

    assert_eq!(run(source), 7);
}

#[test]
fn ternary_evaluate_false() {
    let source = r#"
    int main() {
        int zero = 0;
        return false ? 1 / 0 : 9;
    }
    "#;

    assert_eq!(run(source), 9);
}

#[test]
fn all_integer_comparisons() {
    let source = r#"
    int main() {
        int result = 0;

        if (1 < 2) {
            result += 1;
        }
        if (2 <= 2) {
            result += 2;
        }
        if (3 > 2) {
            result += 4;
        }
        if (3 >= 3) {
            result += 8;
        }
        if (3 == 3) {
            result += 16;
        }
        if (3 != 4) {
            result += 32;
        }

        return result;
    }
    "#;

    assert_eq!(run(source), 63);
}

#[test]
fn bitwise_operators() {
    let source = r#"
    int main() {
        int x = 13;
        return (x & 3) | (2 ^ 1);
    }
    "#;

    assert_eq!(run(source), 3);
}

#[test]
fn negative_division_and_remainder() {
    let source = r#"
    int main() {
        return (-7 / 3) * -1 + (-7 % 3) * -1;
    }
    "#;

    assert_eq!(run(source), 3);
}

#[test]
fn logical_and_and_not() {
    let source = r#"
    int main() {
        if (true && !false) {
            return 1;
        }
        return 0;
    }
    "#;

    assert_eq!(run(source), 1);
}

#[test]
fn logical_or() {
    let source = r#"
    int main() {
        if (false || true) {
            return 1;
        }
        return 0;
    }
    "#;

    assert_eq!(run(source), 1);
}

#[test]
fn logical_and_short_circuits_rhs() {
    let source = r#"
    int main() {
        int zero = 0;

        if (false && (1 / 0 == 0)) {
            return 1;
        }
        return 2;
    }
    "#;

    assert_eq!(run(source), 2);
}

#[test]
fn logical_or_short_circuits_rhs() {
    let source = r#"
    int main() {
        int zero = 0;

        if (true || (1 / 0 == 0)) {
            return 1;
        }
        return 2;
    }
    "#;

    assert_eq!(run(source), 1);
}

#[test]
fn function_call() {
    let source = r#"
    int add(int a, int b) {
        return a + b;
    }

    int main() {
        return add(2, 3);
    }
    "#;

    assert_eq!(run(source), 5);
}
