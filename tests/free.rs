mod helpers;

use helpers::{assert_analysis_error, run};
use rint::analysis::AnalysisErrorKind;

#[test]
fn allocated_pointer_can_be_deallocated() {
    let source = r#"
    int main() {
        int* value = alloc(int);
        *value = 42;
        int result = *value;
        dealloc(value);
        return result;
    }
    "#;

    assert_eq!(run(source), 42);
}

#[test]
fn allocated_array_can_be_deallocated() {
    let source = r#"
    int main() {
        int[] values = alloc_array(int, 2);
        values[0] = 42;
        int result = values[0];
        dealloc(values);
        return result;
    }
    "#;

    assert_eq!(run(source), 42);
}

#[test]
fn deallocating_null_is_safe() {
    assert_eq!(run("int main() { dealloc(null); return 42; }"), 42);
}

#[test]
fn dealloc_rejects_non_pointer_values() {
    assert_analysis_error(
        "int main() { dealloc(42); return 0; }",
        AnalysisErrorKind::DeallocNonPointer,
    );
}
