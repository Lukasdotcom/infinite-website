FROM rust:slim-bullseye as builder

WORKDIR /usr/src/infinite-website
COPY . .
RUN apt update 
RUN apt install -y pkg-config openssl libssl-dev
RUN cargo install --path . --profile release

FROM debian:bullseye-slim

COPY --from=builder /usr/local/cargo/bin/infinite-website /usr/local/bin/infinite-website
RUN apt update 
RUN apt install -y openssl libssl-dev ca-certificates
RUN rm -rf /var/lib/apt/lists/*

EXPOSE 8080
CMD ["infinite-website"]