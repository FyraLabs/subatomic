FROM golang:1.25.2-trixie as builder

WORKDIR /app

COPY . .

RUN go build -o /subatomic ./server

FROM debian:trixie

COPY --from=builder /subatomic /subatomic

RUN apt update && apt install -y createrepo-c

EXPOSE 3000

CMD [ "/subatomic" ]
