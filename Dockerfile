FROM rust:latest AS builder

WORKDIR /chess

COPY . .

RUN cargo build --release

FROM ubuntu:24.04 

WORKDIR /chess

COPY --from=builder /chess/target/release /chess/chess

# Open the port your WebSocket listens on
EXPOSE 3000

# Run the binary
CMD ["./chess"]
