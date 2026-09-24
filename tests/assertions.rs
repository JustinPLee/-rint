mod helpers;

use helpers::run;

#[test]
fn assertion_passes() {
    let source = r#"
    int main() {
        assert(true);
        return 7;
    }
    "#;

    assert_eq!(run(source), 7);
}

#[test]
fn assertion_aborts() {
    let source = r#"
    int main() {
        assert(false);
        return 7;
    }
    "#;

    assert_eq!(run(source), -1);
}

#[test]
fn assertion_inside_while() {
    let source = r#"
    int main() {
        int i = 0;

        while (i < 3) {
            assert(i < 3);
            i = i + 1;
        }

        return i;
    }
    "#;

    assert_eq!(run(source), 3);
}

#[test]
fn assertion_inside_branch() {
    let source = r#"
    int main() {
        int x = 1;

        if (x == 1) {
            assert(true);
        } else {
            assert(false);
        }

        return x;
    }
    "#;

    assert_eq!(run(source), 1);
}
