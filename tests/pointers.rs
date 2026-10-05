mod helpers;

use helpers::{assert_analysis_error, run};
use rint::analysis::AnalysisErrorKind;

#[test]
fn pointer_recursive_structs_can_link_allocations() {
    let source = r#"
    struct Node;
    struct Node {
        int value;
        struct Node* next;
    };

    int main() {
        struct Node* first = alloc(struct Node);
        struct Node* second = alloc(struct Node);
        first->value = 17;
        first->next = second;
        second->value = 25;
        return first->value + first->next->value;
    }
    "#;

    assert_eq!(run(source), 42);
}

#[test]
fn linked_lists_reverse_iteratively_and_recursively() {
    let source = r#"
    struct Node {
        int value;
        struct Node* next;
    };

    struct Node* reverse_list_iterative(struct Node* head) {
        struct Node* previous = null;
        struct Node* current = head;

        while (current != null) {
            struct Node* next = current->next;
            current->next = previous;
            previous = current;
            current = next;
        }

        return previous;
    }

    struct Node* reverse_list_recursive(struct Node* head) {
        if (head == null || head->next == null) {
            return head;
        }

        struct Node* reversed = reverse_list_recursive(head->next);
        head->next->next = head;
        head->next = null;
        return reversed;
    }

    int main() {
        struct Node* first = alloc(struct Node);
        struct Node* second = alloc(struct Node);
        struct Node* third = alloc(struct Node);
        first->value = 1;
        first->next = second;
        second->value = 2;
        second->next = third;
        third->value = 3;

        struct Node* iterative = reverse_list_iterative(first);
        if (iterative->value != 3
            || iterative->next->value != 2
            || iterative->next->next->value != 1
            || iterative->next->next->next != null) {
            return 1;
        }

        struct Node* fourth = alloc(struct Node);
        struct Node* fifth = alloc(struct Node);
        struct Node* sixth = alloc(struct Node);
        fourth->value = 4;
        fourth->next = fifth;
        fifth->value = 5;
        fifth->next = sixth;
        sixth->value = 6;

        struct Node* recursive = reverse_list_recursive(fourth);
        if (recursive->value != 6
            || recursive->next->value != 5
            || recursive->next->next->value != 4
            || recursive->next->next->next != null) {
            return 1;
        }

        return 42;
    }
    "#;

    assert_eq!(run(source), 42);
}

#[test]
fn opaque_pointer_can_be_passed_and_returned_before_definition() {
    let source = r#"
    struct Item;

    struct Item* identity(struct Item* item) {
        return item;
    }

    struct Item { int value; };

    int main() {
        struct Item* item = alloc(struct Item);
        item->value = 42;
        return identity(item)->value;
    }
    "#;

    assert_eq!(run(source), 42);
}

#[test]
fn null_pointer_compares_and_assigns_correctly() {
    let source = r#"
    struct Item { int value; };

    int main() {
        struct Item* item = null;
        if (item == null) {
            item = alloc(struct Item);
        }
        if (item != null) {
            item->value = 42;
        }
        return item->value;
    }
    "#;

    assert_eq!(run(source), 42);
}

#[test]
fn pointer_aliases_share_field_updates() {
    let source = r#"
    struct Item { int value; };

    int main() {
        struct Item* first = alloc(struct Item);
        struct Item* alias = first;
        struct Item* other = alloc(struct Item);
        first->value = 12;
        alias->value += 30;
        if (first != alias || first == other) {
            return 1;
        }
        return first->value;
    }
    "#;

    assert_eq!(run(source), 42);
}

#[test]
fn conditional_pointer_result_can_be_dereferenced() {
    let source = r#"
    struct Item { int value; };

    int main() {
        struct Item* item = alloc(struct Item);
        item->value = 42;
        struct Item* chosen = true ? item : null;
        if (chosen == null) {
            return 1;
        }
        return (*chosen).value;
    }
    "#;

    assert_eq!(run(source), 42);
}

#[test]
fn pointer_destinations_evaluate_the_base_once() {
    let source = r#"
    struct Item { int value; };
    struct State {
        int calls;
        struct Item* item;
    };

    struct Item* get_item(struct State* state) {
        state->calls++;
        return state->item;
    }

    int main() {
        struct State* state = alloc(struct State);
        struct Item* item = alloc(struct Item);
        item->value = 5;
        state->item = item;
        get_item(state)->value += 2;
        return state->calls * 10 + item->value;
    }
    "#;

    assert_eq!(run(source), 17);
}

#[test]
fn pointer_arguments_use_qword_slots_including_stack_arguments() {
    let source = r#"
    struct Item { int value; };

    int sum_values(
        struct Item* a, struct Item* b, struct Item* c,
        struct Item* d, struct Item* e, struct Item* f,
        struct Item* g
    ) {
        return a->value + b->value + c->value + d->value
             + e->value + f->value + g->value;
    }

    int main() {
        struct Item* item = alloc(struct Item);
        item->value = 6;
        return sum_values(item, item, item, item, item, item, item);
    }
    "#;

    assert_eq!(run(source), 42);
}

#[test]
fn null_dereference_aborts() {
    let source = r#"
    struct Item { int value; };
    int main() {
        struct Item* item = NULL;
        return item->value;
    }
    "#;

    assert_eq!(run(source), -1);
}

#[test]
fn dereferencing_a_non_pointer_is_rejected() {
    let source = r#"
    int main() {
        int value = 7;
        return *value;
    }
    "#;

    assert_analysis_error(source, AnalysisErrorKind::DereferenceNonPointer);
}

#[test]
fn allocating_an_unknown_struct_is_rejected() {
    let source = r#"
    int main() {
        struct Missing* value = alloc(struct Missing);
        return value == NULL;
    }
    "#;

    assert_analysis_error(source, AnalysisErrorKind::UnknownType);
}

#[test]
fn allocating_an_incomplete_struct_is_rejected() {
    let source = r#"
    struct Opaque;
    int main() {
        struct Opaque* value = alloc(struct Opaque);
        return value == NULL;
    }
    "#;

    assert_analysis_error(source, AnalysisErrorKind::UndefinedStruct);
}

#[test]
fn assigning_incompatible_pointer_types_is_rejected() {
    let source = r#"
    struct Left { int value; };
    struct Right { int value; };
    int main() {
        struct Left* left = alloc(struct Left);
        struct Right* right = alloc(struct Right);
        left = right;
        return 0;
    }
    "#;

    assert_analysis_error(source, AnalysisErrorKind::TypeMismatch);
}

#[test]
fn pointer_to_array_is_rejected_until_arrays_are_supported() {
    let source = r#"
    int main() {
        int[] values;
        return 0;
    }
    "#;

    assert_analysis_error(source, AnalysisErrorKind::ArraysNotImplemented);
}
