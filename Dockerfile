FROM rust:1.68-alpine as builder

WORKDIR /usr/src/infinite-website
COPY . .

RUN cargo install --path . --profile release

FROM alpine:3.17

COPY --from=builder /usr/local/cargo/bin/infinite-website /usr/local/bin/infinite-website

CMD ["infinite-website"]