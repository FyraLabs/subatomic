FROM golang:1.25.2-trixie as builder

WORKDIR /app

COPY . .

RUN go build -o /subatomic-cli ./subatomic-cli

FROM debian:trixie

COPY --from=builder /subatomic-cli /usr/bin/subatomic-cli

RUN apt update && apt install -y bash curl

CMD [ "/usr/bin/subatomic-cli" ]
