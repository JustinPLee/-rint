lang_path := "src/lang.lang"

r:
    cargo run -- {{ lang_path }} -vv
t:
    cargo insta test -- --test-threads=1
to:
    cargo insta test -- --test-threads=1 --show-output
c:
    cargo clippy
