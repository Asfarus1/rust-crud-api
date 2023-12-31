# Build stage
FROM rust:1.74.0 as builder
WORKDIR /app

ARG DB_URL
ENV DB_URL=$DB_URL

COPY . .
RUN cargo build --release

# Production stage
FROM debian:trixie
WORKDIR /usr/local/bin
COPY --from=builder /app/target/release/rust-crud-api .
CMD ["./rust-crud-api"]