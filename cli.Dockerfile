FROM golang:1.23.0-bookworm as builder

WORKDIR /app

COPY . .

RUN go build -o /subatomic-cli ./subatomic-cli

FROM debian:bookworm

COPY --from=builder /subatomic-cli /usr/bin/subatomic-cli

CMD [ "/usr/bin/subatomic-cli" ]
