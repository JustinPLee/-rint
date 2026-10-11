mod helpers;

use helpers::{assert_analysis_error, run};
use rint::analysis::AnalysisErrorKind;

#[test]
fn array_length_and_pass_by_reference() {
    let source = r#"
    int update(int[] values) {
        values[1] += 7;
        return length_array(values);
    }
    int main() {
        int[] values = alloc_array(int, 3);
        values[1] = 35;
        int length = update(values);
        return values[1] + length - 3;
    }
    "#;
    assert_eq!(run(source), 42);
}

#[test]
fn zero_length_and_nested_arrays() {
    let source = r#"
    int main() {
        int[] empty = alloc_array(int, 0);
        int[][] rows = alloc_array(int[], 2);
        rows[0] = alloc_array(int, 1);
        rows[0][0] = 42;
        return rows[0][0] + length_array(empty);
    }
    "#;
    assert_eq!(run(source), 42);
}

#[test]
fn array_type_errors() {
    assert_analysis_error(
        "int main() { return length_array(2); }",
        AnalysisErrorKind::LengthNonArray,
    );
    assert_analysis_error(
        "int main() { return 2[0]; }",
        AnalysisErrorKind::IndexNonArray,
    );
    assert_analysis_error(
        "int main() { int[] a = alloc_array(int, true); return 0; }",
        AnalysisErrorKind::TypeMismatch,
    );
}

#[test]
fn bounds_and_allocation_checks_abort() {
    assert_eq!(
        run("int main() { int[] a = alloc_array(int, 2); return a[-1]; }"),
        -1
    );
    assert_eq!(
        run("int main() { int[] a = alloc_array(int, 2); return a[2]; }"),
        -1
    );
    assert_eq!(
        run("int main() { int[] a = alloc_array(int, 0); return a[0]; }"),
        -1
    );
    assert_eq!(
        run("int main() { int[] a = alloc_array(int, -1); return 0; }"),
        -1
    );
    assert_eq!(run("int main() { int[] a = null; return length_array(a); }"), -1);
}

#[test]
fn compound_index_is_evaluated_once() {
    let source = r#"
    int next(int[] indexes) {
        indexes[0]++;
        return indexes[0];
    }
    int main() {
        int[] indexes = alloc_array(int, 1);
        int[] values = alloc_array(int, 3);
        values[1] = 40;
        values[next(indexes)]++;
        return values[1] + indexes[0];
    }
    "#;
    assert_eq!(run(source), 42);
}
