FROM rustlang/rust:nightly as builder

COPY . /app
RUN cd /app && cargo build --release

FROM ubuntu
COPY --from=builder /app/target/release/daprox /usr/bin/daprox

ENV DAPROX_LISTEN=[::]:80

CMD ["/usr/bin/daprox"]
