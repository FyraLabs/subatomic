FROM golang:1.23.0-bookworm as builder

WORKDIR /app

COPY . .

RUN go build -o /subatomic ./server

FROM debian:bookworm

COPY --from=builder /subatomic /subatomic

RUN apt update && apt install -y createrepo-c

EXPOSE 3000

CMD [ "/subatomic" ]
