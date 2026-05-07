# Build Stage
FROM rustlang/rust:nightly AS builder

## Add source code to the build stage.
ADD . /src
WORKDIR /src

RUN cargo install cargo-fuzz

RUN echo building instrumented harnesses && \
    bash -c "pushd biscuit-parser/fuzz && cargo +nightly fuzz build && popd" && \
    mv biscuit-parser/fuzz/target/x86_64-unknown-linux-gnu/release/parse_source /parse_source && \
    echo done

RUN echo building non-instrumented harnesses && \
    export RUSTFLAGS="--cfg fuzzing -Clink-dead-code -Cdebug-assertions -C codegen-units=1" && \
    bash -c "pushd biscuit-parser/fuzz && cargo +nightly build --release && popd" && \
    mv biscuit-parser/fuzz/target/release/parse_source /parse_source_no_inst && \
    echo done

# Package Stage
FROM rustlang/rust:nightly

COPY --from=builder /parse_source /parse_source_no_inst /
