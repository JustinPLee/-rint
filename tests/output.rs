use rint::driver::{compile, run as crun};

fn run(source: &str) -> i32 {
    crun(&compile(source.as_bytes()))
}
#[test]
fn addition() {
    let source = "
    int main() {
        int x = 2 + 2;
        return x;
    }
    ";

    assert_eq!(run(source), 4);
}

#[test]
fn precedence() {
    let source = "
    int main() {
        int x = 2 + 3 * 4;
        return x;
    }
    ";

    assert_eq!(run(source), 14);
}

#[test]
fn parentheses() {
    let source = "
    int main() {
        int x = (2 + 3) * 4;
        return x;
    }
    ";

    assert_eq!(run(source), 20);
}

#[test]
fn unary_minus() {
    let source = "
    int main() {
        int x = -1 + 2;
        return x;
    }
    ";

    assert_eq!(run(source), 1);
}

#[test]
fn variables() {
    let source = "
    int main() {
        int a = 10;
        int b = 20;
        int c = a + b;
        return c;
    }
    ";

    assert_eq!(run(source), 30);
}

#[test]
fn variable_reuse() {
    let source = "
    int main() {
        int x = 10;
        int y = 20;
        int z = x + y;
        int w = z + x;
        return w;
    }
    ";

    assert_eq!(run(source), 40);
}

#[test]
fn long_expression() {
    let source = "
    int main() {
        int x = 1 + 2 + 3 + 4 + 5 + 6 + 7 + 8 + 9 + 10;
        return x;
    }
    ";

    assert_eq!(run(source), 55);
}

#[test]
fn many_live_values() {
    let source = "
    int main() {
        int a = 1;
        int b = 2;
        int c = 3;
        int d = 4;
        int e = 5;
        int f = 6;
        int g = 7;
        int h = 8;
        int i = 9;
        int j = 10;

        int x = a + b;
        int y = c + d;
        int z = e + f;
        int w = g + h;

        return x + y + z + w + i + j;
    }
    ";

    assert_eq!(run(source), 55);
}

#[test]
fn repeated_assignment() {
    let source = "
    int main() {
        int x = 1;
        x = x + 2;
        x = x + 3;
        x = x + 4;
        return x;
    }
    ";

    assert_eq!(run(source), 10);
}

#[test]
fn complex_expression() {
    let source = "
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
    ";

    assert_eq!(run(source), 11);
}

#[test]
fn comparison_equal() {
    let source = "
    int main() {
        int x = 5;

        if (x == 5) {
            return 1;
        } else {
            return 0;
        }
    }
    ";

    assert_eq!(run(source), 1);
}

#[test]
fn nested_if() {
    let source = "
    int main() {
        int x = 10;

        if (x > 5) {
            if (x > 8) {
                return 10;
            } else {
                return 8;
            }
        } else {
            return 5;
        }
    }
    ";

    assert_eq!(run(source), 10);
}

#[test]
fn while_loop() {
    let source = "
    int main() {
        int x = 0;

        while (x < 5) {
            x = x + 1;
        }

        return x;
    }
    ";

    assert_eq!(run(source), 5);
}

#[test]
fn while_with_if() {
    let source = "
    int main() {
        int x = 0;
        int sum = 0;

        while (x < 10) {
            if (x == 5) {
                sum = sum + 10;
            } else {
                sum = sum + 1;
            }

            x = x + 1;
        }

        return sum;
    }
    ";

    assert_eq!(run(source), 19);
}

#[test]
fn for_loop() {
    let source = "
    int main() {
        int sum = 0;

        for (int i = 0; i < 10; i = i + 1) {
            sum = sum + i;
        }

        return sum;
    }
    ";

    assert_eq!(run(source), 45);
}

#[test]
fn for_with_if() {
    let source = "
    int main() {
        int sum = 0;

        for (int i = 0; i < 10; i = i + 1) {
            if (i % 2 == 0) {
                sum = sum + i;
            } else {
                sum = sum + 1;
            }
        }

        return sum;
    }
    ";

    assert_eq!(run(source), 25);
}

#[test]
fn ternary() {
    let source = "
    int main() {
        int x = 10;
        int y = 20;
        int z;

        z = x < y ? x : y;

        return z;
    }
    ";

    assert_eq!(run(source), 10);
}

#[test]
fn ternary_in_if() {
    let source = "
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
    ";

    assert_eq!(run(source), 100);
}

#[test]
fn nested_control_flow() {
    let source = "
    int main() {
        int sum = 0;

        for (int i = 0; i < 5; i = i + 1) {
            int x = i > 2 ? i : 2;

            while (x < 6) {
                if (x != 4) {
                    sum = sum + x;
                } else {
                    sum = sum + 10;
                }

                x = x + 1;
            }
        }

        return sum;
    }
    ";

    assert_eq!(run(source), 93);
}
