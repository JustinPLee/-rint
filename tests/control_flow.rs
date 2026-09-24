mod helpers;

use helpers::run;

#[test]
fn nested_if() {
    let source = r#"
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
    "#;

    assert_eq!(run(source), 10);
}

#[test]
fn while_loop() {
    let source = r#"
    int main() {
        int x = 0;

        while (x < 5) {
            x = x + 1;
        }

        return x;
    }
    "#;

    assert_eq!(run(source), 5);
}

#[test]
fn while_with_if() {
    let source = r#"
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
    "#;

    assert_eq!(run(source), 19);
}

#[test]
fn for_loop() {
    let source = r#"
    int main() {
        int sum = 0;

        for (int i = 0; i < 10; i = i + 1) {
            sum = sum + i;
        }

        return sum;
    }
    "#;

    assert_eq!(run(source), 45);
}

#[test]
fn for_with_if() {
    let source = r#"
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
    "#;

    assert_eq!(run(source), 25);
}

#[test]
fn nested_control_flow() {
    let source = r#"
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
    "#;

    assert_eq!(run(source), 93);
}

#[test]
fn break_in_while() {
    let source = r#"
    int main() {
        int sum = 0;
        int i = 0;

        while (i < 10) {
            if (i == 5) {
                break;
            }
            sum = sum + i;
            i = i + 1;
        }

        return sum;
    }
    "#;

    assert_eq!(run(source), 10);
}

#[test]
fn continue_in_while() {
    let source = r#"
    int main() {
        int sum = 0;
        int i = 0;

        while (i < 6) {
            i++;

            if (i == 3) {
                continue;
            }
            sum += i;
        }

        return sum;
    }
    "#;

    assert_eq!(run(source), 18);
}

#[test]
fn break_in_nested_while() {
    let source = r#"
    int main() {
        int sum = 0;
        int i = 0;

        while (i < 3) {
            int j = 0;

            while (j < 5) {
                if (j == 2) {
                    break;
                }
                sum++;
                j++;
            }

            i++;
        }

        return sum;
    }
    "#;

    assert_eq!(run(source), 6);
}

#[test]
fn continue_in_nested_while() {
    let source = r#"
    int main() {
        int sum = 0;
        int i = 0;

        while (i < 3) {
            int j = 0;

            while (j < 4) {
                j++;

                if (j == 2) {
                    continue;
                }
                sum += j;
            }

            i++;
        }

        return sum;
    }
    "#;

    assert_eq!(run(source), 24);
}

#[test]
fn break_and_continue() {
    let source = r#"
    int main() {
        int sum = 0;
        int i = 0;

        while (i < 10) {
            i += 1;

            if (i == 3) {
                continue;
            }

            if (i == 7) {
               break;
            }

            sum += 1;
        }

        return sum;
    }
    "#;

    assert_eq!(run(source), 5);
}

#[test]
fn for_without_init() {
    let source = r#"
    int main() {
        int i = 0;
        int sum = 0;

        for (; i < 5; i = i + 1) {
            sum = sum + i;
        }

        return sum;
    }
    "#;

    assert_eq!(run(source), 10);
}

#[test]
fn for_without_step() {
    let source = r#"
    int main() {
        int i = 0;
        int sum = 0;

        for (i = 0; i < 5;) {
            sum = sum + i;
            i = i + 1;
        }

        return sum;
    }
    "#;

    assert_eq!(run(source), 10);
}

#[test]
fn for_without_init_or_step() {
    let source = r#"
    int main() {
        int i = 0;
        int sum = 0;

        for (; i < 5;) {
            sum = sum + i;
            i = i + 1;
        }

        return sum;
    }
    "#;

    assert_eq!(run(source), 10);
}

#[test]
fn for_init_scope() {
    let source = r#"
    int main() {
        int i = 10;

        for (i = 0; i < 5; i++) {
            i += 1;
        }

        return i;
    }
    "#;

    assert_eq!(run(source), 6);
}

#[test]
fn continue_in_for() {
    let source = r#"
    int main() {
        int sum = 0;

        for (int i = 0; i < 5; i++) {
            if (i == 2) {
                continue;
            }
            sum += i;
        }

        return sum;
    }
    "#;

    assert_eq!(run(source), 8);
}
