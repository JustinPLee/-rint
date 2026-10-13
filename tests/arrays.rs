mod helpers;

use helpers::{assert_analysis_error, run};
use rint::analysis::AnalysisErrorKind;

#[test]
fn array_length_and_pass_by_reference() {
    let source = r#"
    int update(int[] values) {
        values[1] += 7;
        return array_length(values);
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
        return rows[0][0] + array_length(empty);
    }
    "#;
    assert_eq!(run(source), 42);
}

#[test]
fn array_type_errors() {
    assert_analysis_error(
        "int main() { return array_length(2); }",
        AnalysisErrorKind::ArrayLengthNonArray,
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
    assert_eq!(run("int main() { int[] a = null; return array_length(a); }"), -1);
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

#[test]
fn bool_and_pointer_arrays_are_zeroed_and_keep_their_element_stride() {
    let source = r#"
    int main() {
        bool[] flags = alloc_array(bool, 3);
        int*[] pointers = alloc_array(int*, 2);
        if (flags[0] || flags[1] || flags[2]
            || pointers[0] != null || pointers[1] != null) {
            return 1;
        }

        flags[2] = true;
        int* value = alloc(int);
        *value = 42;
        pointers[1] = value;
        if (!flags[2] || array_length(flags) != 3 || array_length(pointers) != 2) {
            return 2;
        }
        return *pointers[1];
    }
    "#;

    assert_eq!(run(source), 42);
}

#[test]
fn arrays_can_be_returned_and_mutated_through_function_calls() {
    let source = r#"
    int[] make_values(int first) {
        int[] values = alloc_array(int, 2);
        values[0] = first;
        return values;
    }

    int[] update_values(int[] values) {
        values[1] += 5;
        return values;
    }

    int main() {
        int[] values = update_values(make_values(37));
        return values[0] + values[1];
    }
    "#;

    assert_eq!(run(source), 42);
}

#[test]
fn array_function_arguments_must_match_element_types() {
    let source = r#"
    int first_value(int[] values) {
        return values[0];
    }

    int main() {
        bool[] values = alloc_array(bool, 1);
        return first_value(values);
    }
    "#;

    assert_analysis_error(source, AnalysisErrorKind::FunctionArgsTypeMismatch);
}
