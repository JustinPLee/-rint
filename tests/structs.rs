mod helpers;

use helpers::{assert_analysis_error, run};
use rint::analysis::AnalysisErrorKind;

#[test]
fn allocated_struct_fields_are_zeroed_and_addressable() {
    let source = r#"
    struct Point {
        int x;
        int y;
    };

    int main() {
        struct Point* point = alloc(struct Point);
        if (point->x != 0 || point->y != 0) {
            return 1;
        }

        point->x = 5;
        (*point).y = 7;
        return point->x + point->y;
    }
    "#;

    assert_eq!(run(source), 12);
}

#[test]
fn nested_struct_fields_have_correct_offsets() {
    let source = r#"
    struct Inner {
        bool enabled;
        int value;
    };

    struct Outer {
        bool prefix;
        struct Inner inner;
        int suffix;
    };

    int main() {
        struct Outer* outer = alloc(struct Outer);
        outer->prefix = true;
        outer->inner.enabled = true;
        outer->inner.value = 40;
        outer->suffix = 2;
        if (!outer->prefix || !outer->inner.enabled) {
            return 1;
        }
        return outer->inner.value + outer->suffix;
    }
    "#;

    assert_eq!(run(source), 42);
}

#[test]
fn recursive_pointer_through_nested_struct_fields() {
    let source = r#"
    struct Node;
    struct Link {
        struct Node* next;
    };
    struct Node {
        int value;
        struct Link link;
    };

    int main() {
        struct Node* first = alloc(struct Node);
        struct Node* second = alloc(struct Node);
        struct Node* third = alloc(struct Node);

        first->link.next = second;
        second->link.next = third;
        second->value = 17;
        third->value = 25;

        return first->link.next->value
             + first->link.next->link.next->value;
    }
    "#;

    assert_eq!(run(source), 42);
}

#[test]
fn adjacent_bool_and_pointer_fields_do_not_overlap() {
    let source = r#"
    struct Item { int value; };
    struct Record {
        bool first;
        struct Item* item;
        bool last;
        int count;
    };

    int main() {
        struct Record* record = alloc(struct Record);
        struct Item* item = alloc(struct Item);
        record->first = true;
        record->item = item;
        record->last = true;
        record->count = 40;
        record->item->value = 2;
        if (!record->first || !record->last || record->item != item) {
            return 1;
        }
        return record->count + item->value;
    }
    "#;

    assert_eq!(run(source), 42);
}

#[test]
fn nested_field_compound_assignment_preserves_neighbors() {
    let source = r#"
    struct Inner { int left; int right; };
    struct Outer { int before; struct Inner inner; int after; };

    int main() {
        struct Outer* outer = alloc(struct Outer);
        outer->before = 3;
        outer->inner.left = 10;
        outer->inner.right = 20;
        outer->after = 4;
        outer->inner.right += 5;
        return outer->before + outer->inner.left
             + outer->inner.right + outer->after;
    }
    "#;

    assert_eq!(run(source), 42);
}

#[test]
fn duplicate_field_names_in_different_structs_are_allowed() {
    let source = r#"
    struct Left { int value; };
    struct Right { int value; };

    int main() {
        struct Left* left = alloc(struct Left);
        struct Right* right = alloc(struct Right);
        left->value = 20;
        right->value = 22;
        return left->value + right->value;
    }
    "#;

    assert_eq!(run(source), 42);
}

#[test]
fn empty_struct_is_rejected() {
    let source = r#"
    struct Empty {};
    int main() { return 0; }
    "#;

    assert_analysis_error(source, AnalysisErrorKind::StructEmpty);
}

#[test]
fn duplicate_struct_definition_is_rejected() {
    let source = r#"
    struct Item { int value; };
    struct Item { int other; };
    int main() { return 0; }
    "#;

    assert_analysis_error(source, AnalysisErrorKind::StructRedefinition);
}

#[test]
fn duplicate_field_in_one_struct_is_rejected() {
    let source = r#"
    struct Item { int value; bool value; };
    int main() { return 0; }
    "#;

    assert_analysis_error(source, AnalysisErrorKind::StructFieldRedeclaration);
}

#[test]
fn unknown_field_is_rejected() {
    let source = r#"
    struct Item { int value; };
    int main() {
        struct Item* item = alloc(struct Item);
        return item->missing;
    }
    "#;

    assert_analysis_error(source, AnalysisErrorKind::StructFieldNotFound);
}

#[test]
fn field_access_on_scalar_is_rejected() {
    let source = r#"
    int main() {
        int value = 3;
        return value.field;
    }
    "#;

    assert_analysis_error(source, AnalysisErrorKind::FieldAccessOnNonStruct);
}

#[test]
fn incomplete_struct_field_is_rejected() {
    let source = r#"
    struct Later;
    struct Before { struct Later value; };
    int main() { return 0; }
    "#;

    assert_analysis_error(source, AnalysisErrorKind::UndefinedStruct);
}

#[test]
fn recursive_by_value_struct_is_rejected() {
    let source = r#"
    struct Node { struct Node next; };
    int main() { return 0; }
    "#;

    assert_analysis_error(source, AnalysisErrorKind::UndefinedStruct);
}

#[test]
fn struct_values_cannot_be_declared_by_value() {
    let source = r#"
    struct Item { int value; };
    int main() {
        struct Item item;
        return 0;
    }
    "#;

    assert_analysis_error(source, AnalysisErrorKind::StructValueNotAllowed);
}

#[test]
fn array_fields_are_rejected_until_arrays_are_supported() {
    let source = r#"
    struct Item { int[] values; };
    int main() { return 0; }
    "#;

    assert_analysis_error(source, AnalysisErrorKind::ArraysNotImplemented);
}
